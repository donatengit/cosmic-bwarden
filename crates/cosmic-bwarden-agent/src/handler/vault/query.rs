use crate::state::State;
use cosmic_bwarden_core::db::{Db, Entry, EntryData};
use cosmic_bwarden_core::protocol::{EntryType, Response, SidebarEntry};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Strip every on-demand secret from a decrypted entry, leaving only metadata
/// (name, username, uris, identity/card non-secret fields, public key). Used by
/// bulk/`meta` reads so secrets are never returned without an explicit, per-secret
/// request that can enforce master-password reprompt.
pub fn redact_entry_secrets(entry: &mut Entry) {
    match &mut entry.data {
        EntryData::Login { password, totp, .. } => {
            *password = None;
            *totp = None;
        }
        EntryData::Card { number, code, .. } => {
            *number = None;
            *code = None;
        }
        EntryData::SshKey { private_key, .. } => {
            *private_key = None;
        }
        EntryData::SecureNote | EntryData::Identity { .. } => {}
    }
    entry.notes = None;
    // Blank the value of any hidden (user-designated secret) custom field.
    for field in &mut entry.fields {
        if field.ty == Some(cosmic_bwarden_core::api::FieldType::Hidden) {
            field.value = None;
        }
    }
}

/// Verify the master password for a reprompt-gated entry against the stored hash.
/// Returns `Some(error)` if verification is required and failed/absent; `None` on
/// success. Runs synchronously (KDF) while the caller holds the state lock, matching
/// the existing reprompt path.
pub(super) fn verify_reprompt(
    provided: Option<String>,
    db: &Db,
    state: &State,
) -> Option<Response> {
    let password = match provided {
        Some(p) => p,
        None => {
            return Some(Response::Error {
                message: "reprompt_required".to_string(),
            })
        }
    };

    let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
        Ok(c) => c,
        Err(e) => {
            return Some(Response::Error {
                message: format!("failed to load config: {}", e),
            })
        }
    };
    let email = match config.email.as_ref() {
        Some(e) => e,
        None => {
            return Some(Response::Error {
                message: "email not set in config".to_string(),
            })
        }
    };

    let mut pw_vec = cosmic_bwarden_core::locked::Vec::new();
    pw_vec.extend(password.as_bytes().iter().copied());
    let pw = cosmic_bwarden_core::locked::Password::new(pw_vec);

    let kdf = db.kdf.unwrap_or(cosmic_bwarden_core::api::KdfType::Pbkdf2);
    let iterations = db.iterations.unwrap_or(100_000);

    let identity = match cosmic_bwarden_core::identity::Identity::new(
        email, &pw, kdf, iterations, db.memory, db.parallelism,
    ) {
        Ok(id) => id,
        Err(e) => {
            return Some(Response::Error {
                message: format!("identity derivation failed: {}", e),
            })
        }
    };

    match &state.master_password_hash {
        Some(stored_hash) => {
            if identity.master_password_hash.hash() != stored_hash.hash() {
                Some(Response::Error {
                    message: "incorrect password".to_string(),
                })
            } else {
                None
            }
        }
        None => Some(Response::Error {
            message: "agent state inconsistent".to_string(),
        }),
    }
}

pub async fn handle_get_sidebar_entries(
    query: Option<String>,
    entry_type: Option<EntryType>,
    only_pinned: bool,
    state: &Arc<Mutex<State>>,
) -> Response {
    let state_guard = state.lock().await;
    if state_guard.keys.is_none() {
        return Response::Error {
            message: "agent is locked".to_string(),
        };
    }

    let q = query.as_deref().map(str::to_lowercase);
    let entries: Vec<SidebarEntry> = state_guard
        .sidebar_cache
        .iter()
        .filter(|e| {
            if only_pinned && !e.is_pinned {
                return false;
            }
            if let Some(et) = entry_type {
                let type_match = matches!(
                    (&e.entry_type, et),
                    (EntryType::Login, EntryType::Login)
                        | (EntryType::Card, EntryType::Card)
                        | (EntryType::Identity, EntryType::Identity)
                        | (EntryType::SecureNote, EntryType::SecureNote)
                        | (EntryType::SshKey, EntryType::SshKey)
                );
                if !type_match {
                    return false;
                }
            }
            if let Some(q) = &q {
                if e.name.to_lowercase().contains(q.as_str()) {
                    return true;
                }
                if e.id == *q {
                    return true;
                }
                return e
                    .username
                    .as_ref()
                    .map(|u| u.to_lowercase().contains(q.as_str()))
                    .unwrap_or(false);
            }
            true
        })
        .cloned()
        .collect();

    Response::SidebarEntries { entries }
}

pub async fn handle_get_entries(
    query: Option<String>,
    entry_type: Option<EntryType>,
    only_pinned: bool,
    state: &Arc<Mutex<State>>,
) -> Response {
    let state = state.lock().await;
    if let (Some(db), Some(keys)) = (&state.db, &state.keys) {
        let empty_org_keys = std::collections::HashMap::new();
        let org_keys = state.org_keys.as_ref().unwrap_or(&empty_org_keys);
        let mut entries = Vec::new();
        for entry in &db.entries {
            if only_pinned && !entry.favorite {
                continue;
            }
            if let Some(et) = entry_type {
                match (et, &entry.data) {
                    (EntryType::Login, cosmic_bwarden_core::db::EntryData::Login { .. }) => (),
                    (EntryType::Card, cosmic_bwarden_core::db::EntryData::Card { .. }) => (),
                    (EntryType::Identity, cosmic_bwarden_core::db::EntryData::Identity { .. }) => (),
                    (EntryType::SecureNote, cosmic_bwarden_core::db::EntryData::SecureNote) => (),
                    (EntryType::SshKey, cosmic_bwarden_core::db::EntryData::SshKey { .. }) => (),
                    _ => continue,
                }
            }
            // Bulk read: never return secrets here. Secrets are fetched per-entry
            // via GetEntry/GetPassword/GetTotp, which enforce reprompt. Returning
            // them in bulk would bypass reprompt entirely.
            let mut decrypted = entry.decrypt(keys, org_keys);
            redact_entry_secrets(&mut decrypted);
            entries.push(decrypted);
        }

        let entries = if let Some(q) = query {
            let q = q.to_lowercase();
            entries
                .into_iter()
                .filter(|e| {
                    if e.name.to_lowercase().contains(&q) || e.id == q {
                        return true;
                    }
                    if let cosmic_bwarden_core::db::EntryData::Login {
                        username: Some(u),
                        ..
                    } = &e.data
                    {
                        if u.to_lowercase().contains(&q) {
                            return true;
                        }
                    }
                    false
                })
                .collect()
        } else {
            entries
        };
        Response::Entries { entries }
    } else {
        Response::Error {
            message: "agent is locked".to_string(),
        }
    }
}

pub async fn handle_get_entry_meta(id: String, state: &Arc<Mutex<State>>) -> Response {
    // Meta returns no secrets, so it must not require a reprompt (which would
    // otherwise block the detail view for reprompt-gated entries). Decrypt
    // directly and redact.
    let state = state.lock().await;
    if let (Some(db), Some(keys)) = (&state.db, &state.keys) {
        let empty_org_keys = std::collections::HashMap::new();
        let org_keys = state.org_keys.as_ref().unwrap_or(&empty_org_keys);
        if let Some(entry) = db.entries.iter().find(|e| e.id == id) {
            let mut decrypted = entry.decrypt(keys, org_keys);
            redact_entry_secrets(&mut decrypted);
            Response::Entry { entry: decrypted }
        } else {
            Response::Error {
                message: "entry not found".to_string(),
            }
        }
    } else {
        Response::Error {
            message: "agent is locked".to_string(),
        }
    }
}

pub async fn handle_get_entry(
    id: String,
    password: Option<String>,
    state: &Arc<Mutex<State>>,
) -> Response {
    let state = state.lock().await;
    if let (Some(db), Some(keys)) = (&state.db, &state.keys) {
        let empty_org_keys = std::collections::HashMap::new();
        let org_keys = state.org_keys.as_ref().unwrap_or(&empty_org_keys);
        if let Some(entry) = db.entries.iter().find(|e| e.id == id) {
            if entry.master_password_reprompt() {
                if let Some(err) = verify_reprompt(password, db, &state) {
                    return err;
                }
            }

            Response::Entry {
                entry: entry.decrypt(keys, org_keys),
            }
        } else {
            Response::Error {
                message: "entry not found".to_string(),
            }
        }
    } else {
        Response::Error {
            message: "agent is locked".to_string(),
        }
    }
}
