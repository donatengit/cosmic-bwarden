use crate::state::State;
use cosmic_bwarden_core::protocol::{EntryType, Response, SidebarEntry};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_get_entries(query: Option<String>, entry_type: Option<EntryType>, only_pinned: bool, state: &Arc<Mutex<State>>) -> Response {
    let state = state.lock().await;
    if let (Some(db), Some(keys)) = (&state.db, &state.keys) {
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

            entries.push(entry.decrypt(keys));
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

pub async fn handle_get_sidebar_entries(query: Option<String>, entry_type: Option<EntryType>, only_pinned: bool, state: &Arc<Mutex<State>>) -> Response {
    let mut state_guard = state.lock().await;
    if let (Some(db), Some(keys)) = (&state_guard.db, &state_guard.keys) {
        let mut entries = Vec::new();
        let mut new_names = Vec::new();
        let mut new_usernames = Vec::new();

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

            let decrypted_name = if let Some(name) = state_guard.name_cache.get(&entry.id) {
                name.clone()
            } else {
                let name = match cosmic_bwarden_core::vault::decrypt(
                    &entry.name,
                    keys,
                    entry.key.as_deref(),
                ) {
                    Ok(n) => n,
                    Err(_) => entry.name.clone(),
                };
                new_names.push((entry.id.clone(), name.clone()));
                name
            };

            let username_dec = if let Some(u) = state_guard.username_cache.get(&entry.id) {
                Some(u.clone())
            } else {
                let mut u_dec = None;
                if let cosmic_bwarden_core::db::EntryData::Login { username, .. } =
                    &entry.data
                {
                    if let Some(u) = username {
                        u_dec = Some(
                            match cosmic_bwarden_core::vault::decrypt(
                                u,
                                keys,
                                entry.key.as_deref(),
                            ) {
                                Ok(dec_u) => dec_u,
                                Err(_) => u.clone(),
                            },
                        );
                    }
                }
                if let Some(u) = &u_dec {
                    new_usernames.push((entry.id.clone(), u.clone()));
                }
                u_dec
            };

            entries.push((
                SidebarEntry {
                    id: entry.id.clone(),
                    name: decrypted_name,
                    username: username_dec.clone(),
                    entry_type: match &entry.data {
                        cosmic_bwarden_core::db::EntryData::Login { .. } => EntryType::Login,
                        cosmic_bwarden_core::db::EntryData::Card { .. } => EntryType::Card,
                        cosmic_bwarden_core::db::EntryData::Identity { .. } => EntryType::Identity,
                        cosmic_bwarden_core::db::EntryData::SecureNote => EntryType::SecureNote,
                        cosmic_bwarden_core::db::EntryData::SshKey { .. } => EntryType::SshKey,
                    },
                    is_pinned: entry.favorite,
                },
                username_dec,
            ));
        }

        for (id, name) in new_names {
            state_guard.name_cache.insert(id, name);
        }
        for (id, username) in new_usernames {
            state_guard.username_cache.insert(id, username);
        }

        let entries = if let Some(q) = query {
            let q = q.to_lowercase();
            entries
                .into_iter()
                .filter(|(e, u)| {
                    e.name.to_lowercase().contains(&q)
                        || e.id == q
                        || u.as_ref()
                            .map(|un| un.to_lowercase().contains(&q))
                            .unwrap_or(false)
                })
                .map(|(e, _)| e)
                .collect()
        } else {
            entries.into_iter().map(|(e, _)| e).collect()
        };
        Response::SidebarEntries { entries }
    } else {
        Response::Error {
            message: "agent is locked".to_string(),
        }
    }
}

pub async fn handle_get_entry(id: String, password: Option<String>, state: &Arc<Mutex<State>>) -> Response {
    let state = state.lock().await;
    if let (Some(db), Some(keys)) = (&state.db, &state.keys) {
        if let Some(entry) = db.entries.iter().find(|e| e.id == id) {
            if entry.master_password_reprompt() {
                let password = match password {
                    Some(p) => p,
                    None => {
                        return Response::Error {
                            message: "reprompt_required".to_string(),
                        };
                    }
                };

                let config =
                    match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
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

                if let Some(stored_hash) = &state.master_password_hash {
                    if identity.master_password_hash.hash() != stored_hash.hash() {
                        return Response::Error {
                            message: "incorrect password".to_string(),
                        };
                    }
                } else {
                    return Response::Error {
                        message: "agent state inconsistent".to_string(),
                    };
                }
            }

            Response::Entry {
                entry: entry.decrypt(keys),
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

pub async fn handle_get_top_frequent(limit: usize, state: &Arc<Mutex<State>>) -> Response {
    let state_guard = state.lock().await;
    if let Some(db) = &state_guard.db {
        let mut entries = Vec::new();
        for id in state_guard.pinned_ids.iter().take(limit) {
            if let Some(entry) = db.entries.iter().find(|e| &e.id == id) {
                let name = state_guard
                    .name_cache
                    .get(&entry.id)
                    .cloned()
                    .unwrap_or_else(|| entry.name.clone());
                let username = state_guard.username_cache.get(&entry.id).cloned();
                entries.push(SidebarEntry {
                    id: entry.id.clone(),
                    name,
                    username,
                    entry_type: match &entry.data {
                        cosmic_bwarden_core::db::EntryData::Login { .. } => EntryType::Login,
                        cosmic_bwarden_core::db::EntryData::Card { .. } => EntryType::Card,
                        cosmic_bwarden_core::db::EntryData::Identity { .. } => EntryType::Identity,
                        cosmic_bwarden_core::db::EntryData::SecureNote => EntryType::SecureNote,
                        cosmic_bwarden_core::db::EntryData::SshKey { .. } => EntryType::SshKey,
                    },
                    is_pinned: true,
                });
            }
        }
        Response::SidebarEntries { entries }
    } else {
        Response::Error {
            message: "agent is locked".to_string(),
        }
    }
}
