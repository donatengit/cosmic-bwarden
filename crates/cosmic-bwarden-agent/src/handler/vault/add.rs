//! Entry-creation handlers: build an `Entry` from the flat Add* action
//! payloads and push it to the server.

use crate::state::State;
use cosmic_bwarden_core::db::{Entry, Field, Secret, Uri};
use cosmic_bwarden_core::protocol::{EntryType, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

// Mirrors the AddEntry IPC action's flat payload; a params struct is tracked in
// docs/roadmap.md ("API parameter structs").
#[allow(clippy::too_many_arguments)]
pub async fn handle_add_entry(
    name: String,
    entry_type: EntryType,
    username: Option<String>,
    password: Option<Secret>,
    notes: Option<Secret>,
    fields: Vec<Field>,
    totp: Option<Secret>,
    uris: Vec<Uri>,
    state: &Arc<Mutex<State>>,
) -> Response {
    let entry_data = match entry_type {
        EntryType::Login => cosmic_bwarden_core::db::EntryData::Login {
            username,
            password,
            totp,
            uris,
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

// Mirrors the AddCard IPC action's flat payload; a params struct is tracked in
// docs/roadmap.md ("API parameter structs").
#[allow(clippy::too_many_arguments)]
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

// Mirrors the AddIdentity IPC action's flat payload; a params struct is tracked
// in docs/roadmap.md ("API parameter structs").
#[allow(clippy::too_many_arguments)]
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
    let entry_type = match &entry.data {
        cosmic_bwarden_core::db::EntryData::Login { .. } => "login",
        cosmic_bwarden_core::db::EntryData::SecureNote => "secure_note",
        cosmic_bwarden_core::db::EntryData::SshKey { .. } => "ssh_key",
        cosmic_bwarden_core::db::EntryData::Card { .. } => "card",
        cosmic_bwarden_core::db::EntryData::Identity { .. } => "identity",
    };
    log::debug!("entry create: type={} (id assigned by server)", entry_type);
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
            Err(e) => {
                {
                    let mut g = state.lock().await;
                    g.sync_failed = true;
                    g.last_sync_error = Some(e.to_string());
                }
                log::error!(
                    "add entry failed on server (entry will disappear on next sync): {}",
                    e
                );
                Response::Error {
                    message: format!("add failed: {}", e),
                }
            }
        }
    } else {
        Response::Error {
            message: "agent is locked".to_string(),
        }
    }
}
