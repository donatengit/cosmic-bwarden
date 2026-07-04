use crate::keyring;
use crate::state::State;
use cosmic_bwarden_core::protocol::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_version() -> Response {
    let version = cosmic_bwarden_core::version().to_string();
    Response::Version {
        version: version.clone(),
        protocol_version: version,
    }
}

pub async fn handle_get_config(state: &Arc<Mutex<State>>) -> Response {
    let mut state_guard = state.lock().await;

    // Try to load DB if not already present
    if state_guard.db.is_none() {
        if let Ok(config) = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
            if let Some(email) = &config.email {
                if let Ok(mut db) = cosmic_bwarden_core::db::Db::load(&config.server_name(), email)
                {
                    // Try to load tokens from keyring
                    if let Ok(Some((at, rt))) =
                        keyring::get_tokens(&config.server_name(), email).await
                    {
                        db.access_token = Some(at.into());
                        db.refresh_token = Some(rt.into());
                    }
                    state_guard.db = Some(db);
                }
            }
        }
    }

    let is_locked = state_guard.keys.is_none();
    let has_account = state_guard.db.as_ref().is_some_and(|db| db.has_account());
    let needs_login = state_guard.db.as_ref().is_none_or(|db| db.needs_login());
    let sync_failed = state_guard.sync_failed;

    match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
        Ok(config) => Response::Config {
            config,
            needs_login,
            has_account,
            is_locked,
            sync_failed,
        },
        Err(e) => Response::Error {
            message: format!("failed to load config: {}", e),
        },
    }
}

pub async fn handle_lock(state: &Arc<Mutex<State>>) -> Response {
    let email = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy()
        .ok()
        .and_then(|c| c.email)
        .unwrap_or_else(|| "unknown".to_string());
    let mut state_guard = state.lock().await;
    if state_guard.keys.is_some() {
        log::info!("vault locked by user request (account: {})", email);
    }
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
            // Fire-and-forget: keyring D-Bus ops can be slow; result is not
            // meaningful for the logout flow itself.
            let server = config.server_name();
            let email_clone = email.clone();
            tokio::spawn(async move {
                if let Err(e) = keyring::delete_tokens(&server, &email_clone).await {
                    log::error!("failed to delete tokens from keyring on logout: {}", e);
                }
            });
        }
        let db = cosmic_bwarden_core::db::Db::new();
        if let Err(e) = db.save(&config.server_name(), email) {
            log::error!("failed to clear vault DB on logout: {}", e);
        }
    }
    log::info!(
        "logout: {} logged out (server: {})",
        config.email.as_deref().unwrap_or("unknown"),
        config.server_name()
    );
    let mut state_guard = state.lock().await;
    state_guard.lock();
    // Clear in-memory DB so the next GetConfig loads the empty on-disk
    // record and correctly reports has_account=false.
    state_guard.db = None;
    Response::Ack
}
