use crate::keyring;
use crate::state::State;
use cosmic_bwarden_core::db::Secret;
use cosmic_bwarden_core::protocol::{Event, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_unlock(
    password: String,
    state: &Arc<Mutex<State>>,
) -> Response {
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
                db.access_token = Some(Secret::from(at));
                db.refresh_token = Some(Secret::from(rt));
            }
            _ => {}
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
            let mut state_guard = state.lock().await;

            state_guard.keys = Some(keys);
            state_guard.org_keys = Some(org_keys);
            state_guard.master_password_hash = Some(identity.master_password_hash);

            state_guard.pinned_ids.clear();
            for entry in &db.entries {
                if entry.favorite {
                    state_guard.pinned_ids.insert(entry.id.clone());
                }
            }

            // access_token/refresh_token are `#[serde(skip)]` and thus lost on
            // reload from disk; carry over the in-memory tokens from the
            // pre-lock state if the keyring didn't already restore them.
            if db.access_token.is_none() {
                if let Some(prev_db) = &state_guard.db {
                    db.access_token = prev_db.access_token.clone();
                    db.refresh_token = prev_db.refresh_token.clone();
                }
            }
            state_guard.db = Some(db);

            state_guard.broadcast(Event::Unlocked);
            Response::Ack
        }
        Err(e) => Response::Error {
            message: format!("unlock failed: {}", e),
        },
    }
}
