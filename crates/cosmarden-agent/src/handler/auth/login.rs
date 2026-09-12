use crate::keyring;
use crate::state::State;
use cosmarden_core::db::Secret;
use cosmarden_core::protocol::{Event, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

// Mirrors the Login IPC action's flat payload; a params struct is tracked in
// docs/roadmap.md ("API parameter structs").
#[allow(clippy::too_many_arguments)]
pub async fn handle_login(
    email: String,
    password: String,
    server_url: Option<String>,
    remember_me: bool,
    two_factor_token: Option<String>,
    two_factor_provider: Option<u32>,
    two_factor_code: Option<String>,
    device_verification_code: Option<String>,
    state: &Arc<Mutex<State>>,
) -> Response {
    let mut config = match cosmarden_core::config::CosmardenConfig::load_legacy() {
        Ok(c) => c,
        Err(e) => {
            log::debug!("login: no existing config ({}); starting fresh", e);
            cosmarden_core::config::CosmardenConfig::default()
        }
    };
    if let Some(url) = server_url {
        config.base_url = Some(url);
    }
    config.email = Some(email.clone());
    config.persist_session = remember_me;

    let client = cosmarden_core::api::Client::new(&config.base_url(), &config.identity_url());

    let (kdf, iterations, memory, parallelism) = match client.prelogin(&email).await {
        Ok(res) => res,
        Err(e) => {
            return Response::Error {
                message: format!("prelogin failed: {}", e),
            };
        }
    };

    let mut pw_vec = cosmarden_core::locked::Vec::new();
    pw_vec.extend(password.as_bytes().iter().copied());
    let pw = cosmarden_core::locked::Password::new(pw_vec);

    let device_id = match config.device_id().await {
        Ok(id) => id,
        Err(e) => {
            return Response::Error {
                message: format!("failed to get device id: {}", e),
            };
        }
    };

    let identity = match cosmarden_core::identity::Identity::new(
        &email,
        &pw,
        kdf,
        iterations,
        memory,
        parallelism,
    ) {
        Ok(id) => id,
        Err(e) => {
            return Response::Error {
                message: format!("identity derivation failed: {}", e),
            };
        }
    };

    let (access_token, refresh_token, protected_key) = match client
        .login(
            &email,
            &device_id,
            &identity.master_password_hash,
            two_factor_token.as_deref(),
            two_factor_provider,
            two_factor_code.as_deref(),
            device_verification_code.as_deref(),
        )
        .await
    {
        Ok(res) => res,
        Err(cosmarden_core::error::Error::TwoFactorRequired { providers, token }) => {
            return Response::TwoFactorRequired { token, providers };
        }
        Err(cosmarden_core::error::Error::NewDeviceVerificationRequired) => {
            return Response::NewDeviceVerificationRequired;
        }
        Err(e) => {
            return Response::Error {
                message: format!("login failed: {}", e),
            };
        }
    };

    if config.persist_session {
        if let Some(rt) = &refresh_token {
            if let Err(e) =
                keyring::store_tokens(&config.server_name(), &email, &access_token, rt).await
            {
                log::error!("failed to store tokens in keyring: {}", e);
            }
        }
    }

    let mut db = match cosmarden_core::db::Db::load(&config.server_name(), &email) {
        Ok(db) => db,
        Err(e) => {
            log::error!("could not load existing vault DB ({}); starting fresh — data may be lost if disk is full or permissions are wrong", e);
            cosmarden_core::db::Db::new()
        }
    };
    db.access_token = Some(access_token.clone().into());
    db.refresh_token = refresh_token.map(Into::into);
    db.kdf = Some(kdf);
    db.iterations = Some(iterations);
    db.memory = memory;
    db.parallelism = parallelism;
    db.protected_key = protected_key.map(Secret::from);

    match client.sync(&access_token).await {
        Ok((pk, ppk, pok, entries)) => {
            db.protected_key = Some(Secret::from(pk));
            db.protected_private_key = ppk.map(Secret::from);
            db.protected_org_keys = pok.into_iter().map(|(k, v)| (k, Secret::from(v))).collect();
            db.entries = entries;
        }
        Err(e) => {
            return Response::Error {
                message: format!("initial sync failed: {}", e),
            };
        }
    }

    if let Err(e) = db.save(&config.server_name(), &email) {
        return Response::Error {
            message: format!("failed to save db: {}", e),
        };
    }
    if let Err(e) = config.save_legacy() {
        return Response::Error {
            message: format!("failed to save config: {}", e),
        };
    }

    match cosmarden_core::vault::unlock(
        &email,
        &pw,
        kdf,
        iterations,
        memory,
        parallelism,
        db.protected_key.as_ref().map(|s| s.expose()).unwrap_or(""),
        db.protected_private_key.as_ref().map(|s| s.expose()),
        &db.protected_org_keys
            .iter()
            .map(|(k, v)| (k.clone(), v.expose().to_string()))
            .collect::<std::collections::HashMap<_, _>>(),
    ) {
        Ok((keys, org_keys)) => {
            // Persist the refresh token so a PIN unlock after a restart can
            // restore this session without the master password. Non-fatal:
            // the login itself already succeeded.
            match db.refresh_token.as_ref() {
                Some(rt) => {
                    if let Err(e) = crate::session_store::save(
                        &keys,
                        &config.server_name(),
                        &email,
                        rt.expose(),
                    ) {
                        log::error!("login: failed to persist the session envelope: {:#}", e);
                    }
                }
                None => log::error!(
                    "login: server returned no refresh token — a PIN unlock after \
                     restart will have no session to restore"
                ),
            }

            let mut state_guard = state.lock().await;

            state_guard.keys = Some(keys);
            state_guard.org_keys = Some(org_keys);
            state_guard.bump_epoch();

            // The initial `client.sync` above just replaced the local vault
            // with fresh server state, so the vault IS in sync at this point.
            // A stale out-of-sync flag from a previous session (e.g. a
            // degraded PIN unlock) must not survive a successful login and
            // keep the UI showing "Not synced" — only `handle_sync` clears
            // it otherwise.
            state_guard.sync_failed = false;
            state_guard.last_sync_error = None;

            state_guard.pinned_ids.clear();
            for entry in &db.entries {
                if entry.favorite {
                    state_guard.pinned_ids.insert(entry.id.clone());
                }
            }
            state_guard.db = Some(db);
            state_guard.rebuild_sidebar_cache();

            state_guard.broadcast(Event::Unlocked);
            log::info!(
                "login: {} authenticated (server: {})",
                email,
                config.server_name()
            );
            Response::Ack
        }
        Err(e) => Response::Error {
            message: format!("unlock failed after login: {}", e),
        },
    }
}
