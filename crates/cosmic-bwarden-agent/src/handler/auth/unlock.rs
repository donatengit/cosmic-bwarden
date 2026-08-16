use crate::keyring;
use crate::state::State;
use cosmic_bwarden_core::protocol::{Event, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_unlock(password: String, state: &Arc<Mutex<State>>) -> Response {
    let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
        Ok(c) => c,
        Err(e) => {
            return Response::Error {
                message: format!("failed to load config: {}", e),
            };
        }
    };
    let email = match config.email.as_ref() {
        Some(e) => e,
        None => {
            return Response::Error {
                message: "email not set in config. Please login.".to_string(),
            };
        }
    };
    let mut db = match cosmic_bwarden_core::db::Db::load(&config.server_name(), email) {
        Ok(d) => d,
        Err(e) => {
            return Response::Error {
                message: format!("failed to load db: {}", e),
            };
        }
    };

    if !db.has_account() {
        return Response::Error {
            message: "No account configured on this agent. Please login first.".to_string(),
        };
    }

    if db.access_token.is_none() && config.persist_session {
        match keyring::get_tokens(&config.server_name(), email).await {
            Ok(Some((at, rt))) => {
                db.access_token = Some(at.into());
                db.refresh_token = Some(rt.into());
            }
            Ok(None) => {}
            Err(e) => log::warn!("unlock: could not load tokens from keyring: {}", e),
        }
    }

    let mut pw_vec = cosmic_bwarden_core::locked::Vec::new();
    pw_vec.extend(password.as_bytes().iter().copied());
    let pw = cosmic_bwarden_core::locked::Password::new(pw_vec);

    let kdf = db.kdf.unwrap_or(cosmic_bwarden_core::api::KdfType::Pbkdf2);
    let iterations = db.iterations.unwrap_or(100_000);

    let identity = match cosmic_bwarden_core::identity::Identity::new(
        email,
        &pw,
        kdf,
        iterations,
        db.memory,
        db.parallelism,
    ) {
        Ok(id) => id,
        Err(e) => {
            return Response::Error {
                message: format!("identity derivation failed: {}", e),
            };
        }
    };

    match cosmic_bwarden_core::vault::unlock(
        email,
        &pw,
        kdf,
        iterations,
        db.memory,
        db.parallelism,
        db.protected_key.as_ref().map(|s| s.expose()).unwrap_or(""),
        db.protected_private_key.as_ref().map(|s| s.expose()),
        &db.protected_org_keys
            .iter()
            .map(|(k, v)| (k.clone(), v.expose().to_string()))
            .collect::<std::collections::HashMap<_, _>>(),
    ) {
        Ok((keys, org_keys)) => {
            // Determine whether we need a silent re-auth before committing state,
            // so we can release the lock during the network call.
            let master_password_hash = identity.master_password_hash.clone();

            let needs_reauth = {
                let mut state_guard = state.lock().await;

                state_guard.keys = Some(keys);
                state_guard.org_keys = Some(org_keys);
                state_guard.master_password_hash = Some(identity.master_password_hash);
                state_guard.bump_epoch();

                state_guard.pinned_ids.clear();
                for entry in &db.entries {
                    if entry.favorite {
                        state_guard.pinned_ids.insert(entry.id.clone());
                    }
                }

                // access_token/refresh_token are `#[serde(skip)]` — not on disk.
                // Try the previous in-memory DB first (covers lock→unlock without
                // agent restart).
                if db.access_token.is_none() {
                    if let Some(prev_db) = &state_guard.db {
                        db.access_token = prev_db.access_token.clone();
                        db.refresh_token = prev_db.refresh_token.clone();
                    }
                }

                let needs_reauth = db.access_token.is_none();
                state_guard.db = Some(db);
                state_guard.rebuild_sidebar_cache();
                state_guard.broadcast(Event::Unlocked);
                log::info!(
                    "vault unlocked via master password for {} (server: {})",
                    email,
                    config.server_name()
                );
                needs_reauth
            };

            // If tokens are still missing (agent restart, or persist_session=false),
            // silently re-authenticate using the master-password hash we just derived.
            // The lock is intentionally released here so the network call doesn't block
            // other requests.
            let mut can_sync = !needs_reauth;
            if needs_reauth {
                log::info!("unlock: no session token available, attempting silent re-auth");
                let client = cosmic_bwarden_core::api::Client::new(
                    &config.base_url(),
                    &config.identity_url(),
                );
                match config.device_id().await {
                    Ok(device_id) => {
                        match client
                            .login(
                                email,
                                &device_id,
                                &master_password_hash,
                                None,
                                None,
                                None,
                                None,
                            )
                            .await
                        {
                            Ok((access_token, refresh_token, _protected_key)) => {
                                log::info!("unlock: silent re-auth succeeded");
                                can_sync = true;
                                {
                                    let mut g = state.lock().await;
                                    if let Some(db) = &mut g.db {
                                        db.access_token = Some(access_token.clone().into());
                                        db.refresh_token =
                                            refresh_token.as_ref().map(|rt| rt.clone().into());
                                    }
                                }
                                if config.persist_session {
                                    if let Some(rt) = &refresh_token {
                                        if let Err(e) = keyring::store_tokens(
                                            &config.server_name(),
                                            email,
                                            &access_token,
                                            rt,
                                        )
                                        .await
                                        {
                                            log::error!(
                                                "failed to store refreshed tokens in keyring: {}",
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                // Server may be unreachable or require 2FA — the vault
                                // is still usable locally, but sync is unavailable until
                                // the next unlock. Surface it: set the out-of-sync flag
                                // so the UI shows "Not synced" instead of lying.
                                can_sync = false;
                                log::error!(
                                    "unlock: silent re-auth failed (sync will be unavailable): {}",
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        can_sync = false;
                        log::error!(
                            "unlock: could not obtain device_id for silent re-auth (sync will be unavailable): {}",
                            e
                        );
                    }
                }
            }

            if can_sync {
                // Catch the vault up with the server now that we are unlocked
                // and have a session. This also clears a stale out-of-sync flag
                // truthfully instead of letting a lock cycle whitewash it.
                let sync_state = Arc::clone(state);
                tokio::spawn(async move {
                    // If the user re-locked before this runs, a sync would fail
                    // for lack of tokens and mark the state out-of-sync falsely.
                    let has_token = {
                        let g = sync_state.lock().await;
                        g.db.as_ref().is_some_and(|db| db.access_token.is_some())
                    };
                    if has_token {
                        let _ = crate::handler::vault::sync::handle_sync(&sync_state).await;
                    }
                });
            } else {
                let mut g = state.lock().await;
                g.sync_failed = true;
                g.last_sync_error = Some(
                    "no session token after unlock — sync unavailable until you log in again"
                        .to_string(),
                );
            }

            Response::Ack
        }
        Err(e) => Response::Error {
            message: format!("unlock failed: {}", e),
        },
    }
}
