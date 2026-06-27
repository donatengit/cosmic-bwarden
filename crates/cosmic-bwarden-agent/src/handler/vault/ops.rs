use crate::server::update_entry_on_server;
use crate::state::State;
use cosmic_bwarden_core::db::{Entry, EntryData, Field, Secret};
use cosmic_bwarden_core::protocol::{EntryType, Response};
use std::sync::Arc;
use tokio::sync::Mutex;
use totp_rs::{Algorithm, TOTP};

pub async fn handle_get_totp(id: String, state: &Arc<Mutex<State>>) -> Response {
    let state_guard = state.lock().await;
    let entry = if let Some(db) = &state_guard.db {
        db.entries.iter().find(|e| e.id == id).cloned()
    } else {
        None
    };
    let keys = state_guard.keys.clone();
    drop(state_guard);

    if let (Some(entry), Some(keys)) = (entry, keys) {
        let entry = entry.decrypt(&keys);
        if let EntryData::Login {
            totp: Some(totp_secret),
            ..
        } = &entry.data
        {
            let secret = totp_secret.expose();
            // Bitwarden stores TOTP as either the secret key or an otpauth:// URL
            let secret_key = if secret.starts_with("otpauth://") {
                match TOTP::from_url(secret) {
                    Ok(t) => t.get_secret_base32(),
                    Err(_) => secret.to_string(),
                }
            } else {
                secret.to_string()
            };

            match TOTP::new(
                Algorithm::SHA1,
                6,
                1,
                30,
                secret_key.as_bytes().to_vec(),
                None,
                "".to_string(),
            ) {
                Ok(totp) => match totp.generate_current() {
                    Ok(code) => Response::Totp { code },
                    Err(e) => Response::Error {
                        message: format!("TOTP generation failed: {}", e),
                    },
                },
                Err(e) => Response::Error {
                    message: format!("Invalid TOTP secret: {}", e),
                },
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
        Err(e) => Response::Error {
            message: format!("delete failed: {}", e),
        },
    }
}

pub async fn handle_update_entry(entry: Entry, state: &Arc<Mutex<State>>) -> Response {
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
            Err(e) => Response::Error {
                message: format!("failed to update entry on server: {}", e),
            },
        }
    } else {
        Response::Error {
            message: "agent is locked".to_string(),
        }
    }
}

pub async fn handle_pin_entry(id: String, state: &Arc<Mutex<State>>) -> Response {
    let mut state_guard = state.lock().await;
    let entry = if let Some(db) = &mut state_guard.db {
        // Update in-memory immediately to prevent concurrent reads from
        // seeing stale favorite values (e.g. GetSidebarEntries requesting
        // only_pinned between this handler and the eventual handle_sync).
        if let Some(e) = db.entries.iter_mut().find(|e| e.id == id) {
            e.favorite = true;
            Some(e.clone())
        } else {
            None
        }
    } else {
        None
    };
    let keys = state_guard.keys.clone();
    drop(state_guard);

    let keys_is_some = keys.is_some();
    if let (Some(entry), Some(keys)) = (entry, keys) {
        let entry = entry.decrypt(&keys);
        let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
            Ok(c) => c,
            Err(e) => {
                return Response::Error {
                    message: format!("failed to load config: {}", e),
                };
            }
        };
        match update_entry_on_server(state, &entry, &config, &keys).await {
            Ok(()) => crate::handler::vault::sync::handle_sync(state).await,
            Err(e) => Response::Error {
                message: format!("failed to pin entry on server: {}", e),
            },
        }
    } else {
        Response::Error {
            message: if keys_is_some {
                "entry not found"
            } else {
                "agent is locked"
            }
            .to_string(),
        }
    }
}

pub async fn handle_unpin_entry(id: String, state: &Arc<Mutex<State>>) -> Response {
    let mut state_guard = state.lock().await;
    let entry = if let Some(db) = &mut state_guard.db {
        if let Some(e) = db.entries.iter_mut().find(|e| e.id == id) {
            e.favorite = false;
            Some(e.clone())
        } else {
            None
        }
    } else {
        None
    };
    let keys = state_guard.keys.clone();
    drop(state_guard);

    let keys_is_some = keys.is_some();
    if let (Some(entry), Some(keys)) = (entry, keys) {
        let entry = entry.decrypt(&keys);
        let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
            Ok(c) => c,
            Err(e) => {
                return Response::Error {
                    message: format!("failed to load config: {}", e),
                };
            }
        };
        match update_entry_on_server(state, &entry, &config, &keys).await {
            Ok(()) => crate::handler::vault::sync::handle_sync(state).await,
            Err(e) => Response::Error {
                message: format!("failed to unpin entry on server: {}", e),
            },
        }
    } else {
        Response::Error {
            message: if keys_is_some {
                "entry not found"
            } else {
                "agent is locked"
            }
            .to_string(),
        }
    }
}

pub async fn handle_add_entry(
    name: String,
    entry_type: EntryType,
    username: Option<String>,
    password: Option<Secret>,
    notes: Option<Secret>,
    fields: Vec<Field>,
    state: &Arc<Mutex<State>>,
) -> Response {
    let entry_data = match entry_type {
        EntryType::Login => cosmic_bwarden_core::db::EntryData::Login {
            username,
            password,
            totp: None,
            uris: Vec::new(),
        },
        EntryType::SecureNote => cosmic_bwarden_core::db::EntryData::SecureNote,
        _ => {
            return Response::Error {
                message: format!("unsupported entry type for AddEntry: {:?}", entry_type),
            }
        }
    };

    let entry = Entry {
        id: String::new(), // Server will assign ID
        org_id: None,
        folder: None,
        folder_id: None,
        name,
        favorite: false,
        data: entry_data,
        fields,
        notes,
        history: Vec::new(),
        key: None,
        master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
    };

    add_entry_to_server(entry, state).await
}

pub async fn handle_add_ssh_key(
    name: String,
    private_key: Secret,
    public_key: Option<String>,
    notes: Option<Secret>,
    fields: Vec<Field>,
    state: &Arc<Mutex<State>>,
) -> Response {
    let mut fields = fields;
    fields.push(Field {
        name: Some("private_key".to_string()),
        value: Some(private_key.clone()),
        ty: Some(cosmic_bwarden_core::api::FieldType::Hidden),
        linked_id: None,
    });
    if let Some(pubk) = &public_key {
        fields.push(Field {
            name: Some("public_key".to_string()),
            value: Some(Secret::from(pubk.clone())),
            ty: Some(cosmic_bwarden_core::api::FieldType::Text),
            linked_id: None,
        });
    }

    let entry = Entry {
        id: String::new(),
        org_id: None,
        folder: None,
        folder_id: None,
        name,
        favorite: false,
        data: cosmic_bwarden_core::db::EntryData::SshKey {
            private_key: Some(private_key),
            public_key,
            fingerprint: None,
        },
        fields,
        notes,
        history: Vec::new(),
        key: None,
        master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
    };

    add_entry_to_server(entry, state).await
}

pub async fn handle_add_card(
    name: String,
    cardholder_name: Option<String>,
    brand: Option<String>,
    number: Option<Secret>,
    exp_month: Option<String>,
    exp_year: Option<String>,
    code: Option<Secret>,
    notes: Option<Secret>,
    fields: Vec<Field>,
    state: &Arc<Mutex<State>>,
) -> Response {
    let entry = Entry {
        id: String::new(),
        org_id: None,
        folder: None,
        folder_id: None,
        name,
        favorite: false,
        data: cosmic_bwarden_core::db::EntryData::Card {
            cardholder_name,
            brand,
            number,
            exp_month,
            exp_year,
            code,
        },
        fields,
        notes,
        history: Vec::new(),
        key: None,
        master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
    };

    add_entry_to_server(entry, state).await
}

pub async fn handle_add_identity(
    name: String,
    first_name: Option<String>,
    last_name: Option<String>,
    address1: Option<String>,
    city: Option<String>,
    state_code: Option<String>,
    postal_code: Option<String>,
    country: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    notes: Option<Secret>,
    fields: Vec<Field>,
    state: &Arc<Mutex<State>>,
) -> Response {
    let entry = Entry {
        id: String::new(),
        org_id: None,
        folder: None,
        folder_id: None,
        name,
        favorite: false,
        data: cosmic_bwarden_core::db::EntryData::Identity {
            title: None,
            first_name,
            middle_name: None,
            last_name,
            address1,
            address2: None,
            address3: None,
            city,
            state: state_code,
            postal_code,
            country,
            phone,
            email,
            ssn: None,
            license_number: None,
            passport_number: None,
            username: None,
        },
        fields,
        notes,
        history: Vec::new(),
        key: None,
        master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
    };

    add_entry_to_server(entry, state).await
}

async fn add_entry_to_server(entry: Entry, state: &Arc<Mutex<State>>) -> Response {
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
        match crate::server::add_entry_on_server(state, &entry, &config, &keys).await {
            Ok(_) => crate::handler::vault::sync::handle_sync(state).await,
            Err(e) => Response::Error {
                message: format!("add failed: {}", e),
            },
        }
    } else {
        Response::Error {
            message: "agent is locked".to_string(),
        }
    }
}
