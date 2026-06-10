use crate::keyring;
use crate::state::State;
use cosmic_bwarden_core::protocol::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_version() -> Response {
    Response::Version {
        version: cosmic_bwarden_core::version().to_string(),
    }
}

pub async fn handle_get_config(state: &Arc<Mutex<State>>) -> Response {
    let mut state_guard = state.lock().await;
    
    // Try to load DB if not already present
    if state_guard.db.is_none() {
        if let Ok(config) = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
            if let Some(email) = &config.email {
                if let Ok(mut db) = cosmic_bwarden_core::db::Db::load(&config.server_name(), email) {
                    // Try to load tokens from keyring
                    if let Ok(Some((at, rt))) = keyring::get_tokens(&config.server_name(), email).await {
                        db.access_token = Some(at.into());
                        db.refresh_token = Some(rt.into());
                    }
                    state_guard.db = Some(db);
                }
            }
        }
    }

    let is_locked = state_guard.keys.is_none();
    let needs_login = state_guard.db.as_ref().map_or(true, |db| db.needs_login());

    match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
        Ok(config) => {
            Response::Config {
                config,
                needs_login,
                is_locked,
            }
        }
        Err(e) => Response::Error {
            message: format!("failed to load config: {}", e),
        },
    }
}

pub async fn handle_lock(state: &Arc<Mutex<State>>) -> Response {
    let mut state_guard = state.lock().await;
    state_guard.lock();
    Response::Ack
}

pub async fn handle_logout(state: &Arc<Mutex<State>>) -> Response {
    let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
        Ok(c) => c,
        Err(e) => {
            return Response::Error {
                message: format!("failed to load config: {}", e),
            };
        }
    };
    if let Some(email) = &config.email {
        if config.persist_session {
            let _ = keyring::delete_tokens(&config.server_name(), email).await;
        }
        let db = cosmic_bwarden_core::db::Db::new();
        let _ = db.save(&config.server_name(), email);
    }
    let mut state_guard = state.lock().await;
    state_guard.lock();
    Response::Ack
}
