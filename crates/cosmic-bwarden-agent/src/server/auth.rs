use std::sync::Arc;
use tokio::sync::Mutex;
use crate::state::State;
use crate::keyring;
use cosmic_bwarden_core::db::Secret;

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

pub async fn with_refresh<F, Fut, T>(state: &Arc<Mutex<State>>, f: F) -> Result<T, String>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = cosmic_bwarden_core::error::Result<T>>,
{
    let config = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy()
        .map_err(|e| e.to_string())?;

    let (access_token, refresh_token, base_url, identity_url) = {
        let mut state_guard = state.lock().await;
        let db = state_guard.db.as_mut().ok_or("agent is locked")?;

        if db.access_token.is_none() && config.persist_session {
            if let Some(email) = config.email.as_deref() {
                match keyring::get_tokens(&config.server_name(), email).await {
                    Ok(Some((at, rt))) => {
                        db.access_token = Some(Secret::from(at));
                        db.refresh_token = Some(Secret::from(rt));
                    }
                    Ok(None) => {}
                    Err(e) => log::warn!("failed to load tokens from keyring: {}", e),
                }
            }
        }

        (
            db.access_token
                .as_ref()
                .ok_or("no API session token — please logout and log in again to restore server sync")?
                .expose()
                .to_string(),
            db.refresh_token
                .as_ref()
                .ok_or("no refresh token")?
                .expose()
                .to_string(),
            config.base_url(),
            config.identity_url(),
        )
    };

    match f(access_token).await {
        Ok(res) => Ok(res),
        Err(e) if is_unauthorized(&e) => {
            log::info!("access token expired, refreshing...");
            let client = cosmic_bwarden_core::api::Client::new(&base_url, &identity_url);
            match client.exchange_refresh_token(&refresh_token).await {
                Ok((new_at, new_rt, _new_key)) => {
                    {
                        let mut state_guard = state.lock().await;
                        if let Some(db) = &mut state_guard.db {
                            db.access_token = Some(Secret::from(new_at.clone()));
                            if let Some(rt) = new_rt {
                                db.refresh_token = Some(Secret::from(rt));
                            }

                            if config.persist_session {
                                if let Some(email) = config.email.as_deref() {
                                    if let (Some(at), Some(rt)) = (db.access_token.as_ref(), db.refresh_token.as_ref()) {
                                        if let Err(e) = keyring::store_tokens(
                                            &config.server_name(),
                                            email,
                                            at.expose(),
                                            rt.expose(),
                                        ).await {
                                            log::error!("failed to persist refreshed tokens to keyring: {}", e);
                                        }
                                    }
                                } else {
                                    log::error!("cannot persist tokens: email not set in config");
                                }
                            }

                            if let Some(email) = config.email.as_deref() {
                                if let Err(e) = db.save(&config.server_name(), email) {
                                    log::error!("failed to save vault DB after token refresh: {}", e);
                                }
                            } else {
                                log::error!("cannot save vault DB after token refresh: email not set in config");
                            }
                        }
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

    #[test]
    fn non_auth_errors_do_not_trigger_refresh() {
        assert!(!is_unauthorized(&Error::RequestFailed { status: 500 }));
        assert!(!is_unauthorized(&Error::RequestFailed { status: 403 }));
        assert!(!is_unauthorized(&Error::Other("connection refused".into())));
    }
}
