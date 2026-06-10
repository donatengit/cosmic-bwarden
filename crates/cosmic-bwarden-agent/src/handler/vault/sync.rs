use crate::server::with_refresh;
use crate::state::State;
use cosmic_bwarden_core::db::Secret;
use cosmic_bwarden_core::protocol::{Event, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_sync(state: &Arc<Mutex<State>>) -> Response {
    let res = with_refresh(state, |at| async move {
        let config = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy()?;
        let client = cosmic_bwarden_core::api::Client::new(
            &config.base_url(),
            &config.identity_url(),
        );
        client.sync(&at).await
    })
    .await;

    match res {
        Ok((protected_key, protected_private_key, protected_org_keys, entries)) => {
            let mut state_guard = state.lock().await;
            
            let pinned_ids: std::collections::HashSet<String> = entries
                .iter()
                .filter(|e| e.favorite)
                .map(|e| e.id.clone())
                .collect();

            let State { db, pinned_ids: state_pinned_ids, .. } = &mut *state_guard;

            if let Some(db) = db {
                db.protected_key = Some(Secret::from(protected_key));
                db.protected_private_key = protected_private_key.map(Secret::from);
                db.protected_org_keys = protected_org_keys
                    .into_iter()
                    .map(|(k, v)| (k, Secret::from(v)))
                    .collect();
                db.entries = entries;
                
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
                            message: "email not set in config".to_string(),
                        };
                    }
                };
                let server = config.server_name();
                let _ = db.save(&server, email);
            }

            *state_pinned_ids = pinned_ids;
            state_guard.name_cache.clear();
            state_guard.username_cache.clear();
            state_guard.broadcast(Event::VaultChanged);
            Response::Ack
        }
        Err(e) => Response::Error {
            message: format!("sync failed: {}", e),
        },
    }
}
