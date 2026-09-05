use crate::keyring;
use crate::state::State;
use std::sync::Arc;
use tokio::sync::Mutex;
use zeroize::Zeroizing;

/// True when an API error means the access token was rejected (HTTP 401) and a
/// refresh should be attempted. The API client returns 401s in two shapes —
/// `RequestUnauthorized` from endpoints that special-case it, and
/// `RequestFailed { status: 401 }` from those that don't. The `Other`-variant
/// string check is kept as a safety net for wrapped/legacy error paths.
/// (A previous version matched only `Other("…401…")`, which no API call
/// produces — the refresh arm was unreachable and an expired access token
/// permanently killed server sync until re-login.)
fn is_unauthorized(e: &cosmic_bwarden_core::error::Error) -> bool {
    use cosmic_bwarden_core::error::Error;
    match e {
        Error::RequestUnauthorized => true,
        Error::RequestFailed { status: 401 } => true,
        Error::Other(msg) => msg.contains("401"),
        _ => false,
    }
}

/// Run `f` with a live access token, refreshing once on a 401.
///
/// The token is handed over as `Zeroizing<String>` rather than a bare `String`:
/// it is a bearer credential, and every copy this function makes — including
/// the one the closure holds for the duration of the request — is scrubbed on
/// drop rather than left in freed heap for a swap or hibernate image to pick
/// up. (The HTTP stack still copies it into its own request and TLS buffers,
/// which we cannot reach; this shrinks the exposure, it does not remove it.)
pub async fn with_refresh<F, Fut, T>(state: &Arc<Mutex<State>>, f: F) -> Result<T, String>
where
    F: Fn(Zeroizing<String>) -> Fut,
    Fut: std::future::Future<Output = cosmic_bwarden_core::error::Result<T>>,
{
    let config = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy()
        .map_err(|e| e.to_string())?;

    let (current_access, vault_keys, refresh_lock, base_url, identity_url) = {
        let mut state_guard = state.lock().await;
        let db = state_guard.db.as_mut().ok_or("agent is locked")?;

        if db.access_token.is_none() && config.persist_session {
            if let Some(email) = config.email.as_deref() {
                match keyring::get_tokens(&config.server_name(), email).await {
                    Ok(Some((at, rt))) => {
                        db.access_token = Some(at.into());
                        db.refresh_token = Some(rt.into());
                    }
                    Ok(None) => log::warn!(
                        "persist_session is on but the keyring holds no session for {}",
                        email
                    ),
                    Err(e) => log::warn!("failed to load tokens from keyring: {}", e),
                }
            }
        }

        let current_access = db
            .access_token
            .as_ref()
            .map(|t| Zeroizing::new(t.expose().to_string()));
        // The envelope is encrypted under the vault keys, so it can only be
        // opened while unlocked. Cloned here so the network call below runs
        // without the state lock held.
        let vault_keys = state_guard.keys.clone();

        (
            current_access,
            vault_keys,
            state_guard.refresh_lock.clone(),
            config.base_url(),
            config.identity_url(),
        )
    };

    // No token in memory. Rather than fail — which pushes the user at a
    // master-password prompt — re-mint one from the stored refresh token. This
    // is what makes a transient outage during unlock self-healing: the unlock's
    // own attempt failed, but the envelope is still on disk and still valid, so
    // the next request simply retries it.
    let access_token = match current_access {
        Some(token) => token,
        None => {
            let email = config
                .email
                .as_deref()
                .ok_or("email not set in config")?
                .to_string();
            let keys = vault_keys.ok_or_else(|| {
                format!(
                    "{} — the vault is locked, so the saved session cannot be opened",
                    cosmic_bwarden_core::protocol::ERR_NO_SESSION
                )
            })?;

            // Serialize with the refresh path below: concurrent requests must
            // not each spend a restore.
            let _guard = refresh_lock.lock().await;
            let already = {
                let g = state.lock().await;
                g.db.as_ref()
                    .and_then(|db| db.access_token.as_ref())
                    .map(|t| Zeroizing::new(t.expose().to_string()))
            };
            match already {
                Some(token) => token,
                None => {
                    crate::handler::auth::reauth::restore_session(
                        state, &config, &email, &keys, None,
                    )
                    .await
                    .map_err(|reason| {
                        format!(
                            "{}: {reason}",
                            cosmic_bwarden_core::protocol::ERR_NO_SESSION
                        )
                    })?;
                    let g = state.lock().await;
                    g.db.as_ref()
                        .and_then(|db| db.access_token.as_ref())
                        .map(|t| Zeroizing::new(t.expose().to_string()))
                        .ok_or_else(|| {
                            format!(
                                "{} — session restore reported success but left no token",
                                cosmic_bwarden_core::protocol::ERR_NO_SESSION
                            )
                        })?
                }
            }
        }
    };

    match f(access_token.clone()).await {
        Ok(res) => Ok(res),
        Err(e) if is_unauthorized(&e) => {
            log::info!("access token expired, refreshing...");

            // Serializes refresh: Vaultwarden rotates the refresh token on use,
            // so two concurrent 401s racing `exchange_refresh_token` with the
            // same (single-use) token would leave the loser's response
            // clobbering good tokens with a rejection or a stale pair (`[F2-4]`).
            let _refresh_guard = refresh_lock.lock().await;

            // Someone else may have already refreshed while we waited for the
            // lock. If the access token in state has moved on from the one we
            // just tried, retry with it instead of refreshing a second time.
            let refreshed_access_token = {
                let state_guard = state.lock().await;
                state_guard
                    .db
                    .as_ref()
                    .and_then(|db| db.access_token.as_ref())
                    .map(|t| Zeroizing::new(t.expose().to_string()))
            };
            if let Some(current) = refreshed_access_token {
                if *current != *access_token {
                    log::info!("token already refreshed by a concurrent request, retrying with it");
                    return f(current).await.map_err(|e| e.to_string());
                }
            }

            let refresh_token = {
                let state_guard = state.lock().await;
                state_guard
                    .db
                    .as_ref()
                    .and_then(|db| db.refresh_token.as_ref())
                    .map(|t| Zeroizing::new(t.expose().to_string()))
                    .ok_or("no refresh token")?
            };

            let client = cosmic_bwarden_core::api::Client::new(&base_url, &identity_url);
            match client.exchange_refresh_token(&refresh_token).await {
                Ok((new_at, new_rt, _new_key)) => {
                    let new_at = Zeroizing::new(new_at);
                    {
                        let mut state_guard = state.lock().await;
                        if let Some(db) = &mut state_guard.db {
                            db.access_token = Some(new_at.as_str().into());
                            if let Some(rt) = new_rt {
                                db.refresh_token = Some(rt.into());
                            }

                            if config.persist_session {
                                if let Some(email) = config.email.as_deref() {
                                    if let (Some(at), Some(rt)) =
                                        (db.access_token.as_ref(), db.refresh_token.as_ref())
                                    {
                                        if let Err(e) = keyring::store_tokens(
                                            &config.server_name(),
                                            email,
                                            at.expose(),
                                            rt.expose(),
                                        )
                                        .await
                                        {
                                            log::error!(
                                                "failed to persist refreshed tokens to keyring: {}",
                                                e
                                            );
                                        }
                                    }
                                } else {
                                    log::error!("cannot persist tokens: email not set in config");
                                }
                            }

                            if let Some(email) = config.email.as_deref() {
                                if let Err(e) = db.save(&config.server_name(), email) {
                                    log::error!(
                                        "failed to save vault DB after token refresh: {}",
                                        e
                                    );
                                }
                            } else {
                                log::error!("cannot save vault DB after token refresh: email not set in config");
                            }
                        }
                    }
                    // Roll the on-disk envelope forward to the token we
                    // just received. Vaultwarden keeps the previous refresh
                    // JWT valid until its own `exp`, so a missed write is not
                    // fatal there — but Bitwarden cloud's policy is not ours
                    // to assume, and re-persisting also extends the validity
                    // window that a later PIN unlock depends on.
                    if let Some(email) = config.email.as_deref() {
                        crate::session_store::persist_current(state, &config.server_name(), email)
                            .await;
                    }

                    // Retry
                    f(new_at).await.map_err(|e| e.to_string())
                }
                Err(e) => Err(format!("refresh failed: {}", e)),
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::is_unauthorized;
    use cosmic_bwarden_core::error::Error;

    // Regression for the dead-refresh-arm bug: both real 401 shapes produced by
    // the API client must trigger a refresh.
    #[test]
    fn real_api_401_shapes_trigger_refresh() {
        assert!(is_unauthorized(&Error::RequestUnauthorized));
        assert!(is_unauthorized(&Error::RequestFailed { status: 401 }));
    }

    #[test]
    fn other_variant_string_fallback_still_works() {
        assert!(is_unauthorized(&Error::Other("HTTP 401 from proxy".into())));
    }

    /// Every copy this module lifts out of a `locked::Token` must land in a
    /// `Zeroizing` wrapper. A bare `expose().to_string()` leaves a bearer
    /// credential in freed heap, where a swap or hibernate image can pick it
    /// up — the same reason the tokens are mlocked in the first place.
    #[test]
    fn token_copies_are_all_zeroizing() {
        let src = include_str!("auth.rs");
        // Built at runtime so this test's own source doesn't match the pattern.
        let needle = ["expose()", ".to_string()"].concat();
        for (i, line) in src.lines().enumerate() {
            // Prose about the rule is not a violation of it.
            if line.trim_start().starts_with("//") {
                continue;
            }
            if line.contains(&needle) {
                assert!(
                    line.contains("Zeroizing::new"),
                    "auth.rs:{}: unwrapped token copy: {}",
                    i + 1,
                    line.trim()
                );
            }
        }
    }

    #[test]
    fn non_auth_errors_do_not_trigger_refresh() {
        assert!(!is_unauthorized(&Error::RequestFailed { status: 500 }));
        assert!(!is_unauthorized(&Error::RequestFailed { status: 403 }));
        assert!(!is_unauthorized(&Error::Other("connection refused".into())));
    }
}
