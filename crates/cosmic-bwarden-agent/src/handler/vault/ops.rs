use crate::server::update_entry_on_server;
use crate::state::State;
use cosmic_bwarden_core::db::{Entry, EntryData};
use cosmic_bwarden_core::protocol::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_get_totp(
    id: String,
    password: Option<String>,
    state: &Arc<Mutex<State>>,
) -> Response {
    let state_guard = state.lock().await;
    let entry = if let Some(db) = &state_guard.db {
        db.entries.iter().find(|e| e.id == id).cloned()
    } else {
        None
    };
    // TOTP is an on-demand secret: enforce master-password reprompt just like
    // GetPassword before releasing a code.
    if let (Some(entry), Some(db)) = (&entry, &state_guard.db) {
        if entry.master_password_reprompt() {
            if let Some(err) = super::query::verify_reprompt(password, db, &state_guard) {
                return err;
            }
        }
    }
    let keys = state_guard.keys.clone();
    let org_keys = state_guard.org_keys.clone().unwrap_or_default();
    drop(state_guard);

    if let (Some(entry), Some(keys)) = (entry, keys) {
        let entry = entry.decrypt(&keys, &org_keys);
        if let EntryData::Login {
            totp: Some(totp_secret),
            ..
        } = &entry.data
        {
            match super::totp::generate_code(totp_secret.expose()) {
                Ok(code) => Response::Totp { code },
                Err(message) => Response::Error { message },
            }
        } else {
            Response::Error {
                message: "Entry has no TOTP secret".to_string(),
            }
        }
    } else {
        Response::Error {
            message: "Agent is locked or entry not found".to_string(),
        }
    }
}

pub async fn handle_delete_entry(id: String, state: &Arc<Mutex<State>>) -> Response {
    log::debug!("entry delete: id={}", id);
    let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
        Ok(c) => c,
        Err(e) => {
            return Response::Error {
                message: format!("failed to load config: {}", e),
            };
        }
    };

    let res = crate::server::with_refresh(state, |at| {
        let id = id.clone();
        let base_url = config.base_url();
        let identity_url = config.identity_url();
        async move {
            let client = cosmic_bwarden_core::api::Client::new(&base_url, &identity_url);
            client.delete_cipher(&at, &id).await
        }
    })
    .await;

    match res {
        Ok(_) => crate::handler::vault::sync::handle_sync(state).await,
        Err(e) => {
            {
                let mut g = state.lock().await;
                g.sync_failed = true;
                g.last_sync_error = Some(e.clone());
            }
            log::error!(
                "delete entry failed on server (local delete will be undone by next sync): {}",
                e
            );
            Response::Error {
                message: format!("delete failed: {}", e),
            }
        }
    }
}

pub async fn handle_update_entry(mut entry: Entry, state: &Arc<Mutex<State>>) -> Response {
    log::debug!("entry update: id={}", entry.id);
    // Redacted (`None`) secrets in the incoming entry mean "unchanged", not
    // "clear" — restore them from the stored entry so an update built from a
    // bulk read can never wipe secrets on the server (see merge.rs).
    {
        let g = state.lock().await;
        if let (Some(db), Some(keys)) = (&g.db, &g.keys) {
            let empty_org_keys = std::collections::HashMap::new();
            let org_keys = g.org_keys.as_ref().unwrap_or(&empty_org_keys);
            if let Some(stored) = db.entries.iter().find(|e| e.id == entry.id) {
                let stored = stored.decrypt(keys, org_keys);
                super::merge::merge_redacted_secrets(&mut entry, &stored);
            }
        }
    }
    // Keep SSH key custom fields in sync with entry.data so the plaintext fallback
    // path in decrypt() always returns the latest values.
    if let cosmic_bwarden_core::db::EntryData::SshKey {
        private_key,
        public_key,
        ..
    } = &entry.data.clone()
    {
        if let Some(pk) = private_key {
            entry.set_field(
                "private_key",
                pk.expose(),
                cosmic_bwarden_core::api::FieldType::Hidden,
            );
        }
        if let Some(pk) = public_key {
            entry.set_field("public_key", pk, cosmic_bwarden_core::api::FieldType::Text);
        }
    }
    let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
        Ok(c) => c,
        Err(e) => {
            return Response::Error {
                message: format!("failed to load config: {}", e),
            };
        }
    };
    let keys = {
        let state_guard = state.lock().await;
        state_guard.keys.clone()
    };
    if let Some(keys) = keys {
        match update_entry_on_server(state, &entry, &config, &keys).await {
            Ok(()) => crate::handler::vault::sync::handle_sync(state).await,
            Err(e) => {
                {
                    let mut g = state.lock().await;
                    g.sync_failed = true;
                    g.last_sync_error = Some(e.to_string());
                }
                log::error!(
                    "update entry failed on server (local edit will be undone by next sync): {}",
                    e
                );
                Response::Error {
                    message: format!("failed to update entry on server: {}", e),
                }
            }
        }
    } else {
        Response::Error {
            message: "agent is locked".to_string(),
        }
    }
}

async fn set_favorite(id: String, favorite: bool, state: &Arc<Mutex<State>>) -> Response {
    log::debug!("entry favorite: id={}, pinned={}", id, favorite);
    let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
        Ok(c) => c,
        Err(e) => {
            return Response::Error {
                message: format!("failed to load config: {}", e),
            };
        }
    };

    // Update in-memory state immediately so concurrent reads see the new value.
    let folder_id;
    {
        let mut g = state.lock().await;
        if g.keys.is_none() {
            return Response::Error {
                message: "agent is locked".to_string(),
            };
        }
        let db = match &mut g.db {
            Some(d) => d,
            None => {
                return Response::Error {
                    message: "agent is locked".to_string(),
                };
            }
        };
        match db.entries.iter_mut().find(|e| e.id == id) {
            Some(e) => {
                e.favorite = favorite;
                // The partial-update endpoint sets folder and favorite
                // together; keep the entry's current folder.
                folder_id = e.folder_id.clone();
            }
            None => {
                return Response::Error {
                    message: "entry not found".to_string(),
                };
            }
        }
        if favorite {
            g.pinned_ids.insert(id.clone());
        } else {
            g.pinned_ids.remove(&id);
        }
        g.rebuild_sidebar_cache();
        let email = config.email.as_deref().unwrap_or("");
        let server = config.server_name();
        if let Some(db) = &g.db {
            if let Err(e) = db.save(&server, email) {
                log::error!("failed to persist favorite change to disk: {}", e);
            }
        }
    }

    let id_clone = id.clone();
    let res = crate::server::with_refresh(state, move |at| {
        let id = id_clone.clone();
        let base_url = config.base_url();
        let identity_url = config.identity_url();
        let folder_id = folder_id.clone();
        async move {
            let client = cosmic_bwarden_core::api::Client::new(&base_url, &identity_url);
            client
                .update_favorite(&at, &id, favorite, folder_id.as_deref())
                .await
        }
    })
    .await;

    match res {
        Ok(()) => {
            state
                .lock()
                .await
                .broadcast(cosmic_bwarden_core::protocol::Event::VaultChanged);
            Response::Ack
        }
        Err(e) => {
            {
                let mut g = state.lock().await;
                g.sync_failed = true;
                g.last_sync_error = Some(e.clone());
            }
            log::error!(
                "update favorite failed on server (local change will be undone by next sync): {}",
                e
            );
            Response::Error {
                message: format!("failed to update favorite on server: {}", e),
            }
        }
    }
}

pub async fn handle_pin_entry(id: String, state: &Arc<Mutex<State>>) -> Response {
    set_favorite(id, true, state).await
}

pub async fn handle_unpin_entry(id: String, state: &Arc<Mutex<State>>) -> Response {
    set_favorite(id, false, state).await
}
