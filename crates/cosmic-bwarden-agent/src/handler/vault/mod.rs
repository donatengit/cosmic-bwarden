pub mod sync;
pub mod query;
pub mod ops;

use crate::state::State;
use cosmic_bwarden_core::protocol::{Action, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_request(action: Action, state: &Arc<Mutex<State>>) -> Response {
    match action {
        Action::Sync => sync::handle_sync(state).await,
        Action::GetEntries { query, entry_type, only_pinned } => {
            query::handle_get_entries(query, entry_type, only_pinned, state).await
        }
        Action::GetSidebarEntries { query, entry_type, only_pinned } => {
            query::handle_get_sidebar_entries(query, entry_type, only_pinned, state).await
        }
        Action::GetEntry { id, password } => query::handle_get_entry(id, password, state).await,
        Action::GetPassword { id, password } => {
            match query::handle_get_entry(id, password, state).await {
                Response::Entry { entry } => {
                    let password = match entry.data {
                        cosmic_bwarden_core::db::EntryData::Login {
                            password: Some(p), ..
                        } => p.expose().to_string(),
                        cosmic_bwarden_core::db::EntryData::SshKey {
                            private_key: Some(pk),
                            ..
                        } => pk.expose().to_string(),
                        cosmic_bwarden_core::db::EntryData::Card {
                            number: Some(n), ..
                        } => n.expose().to_string(),
                        cosmic_bwarden_core::db::EntryData::SecureNote => {
                            match &entry.notes {
                                Some(n) => n.expose().to_string(),
                                None => {
                                    return Response::Error {
                                        message: "entry has no notes".to_string(),
                                    }
                                }
                            }
                        }
                        _ => {
                            return Response::Error {
                                message: "entry has no password".to_string(),
                            }
                        }
                    };
                    Response::Password { password }
                }
                r => r,
            }
        }
        Action::GetTotp { id } => ops::handle_get_totp(id, state).await,
        Action::DeleteEntry { id } => ops::handle_delete_entry(id, state).await,
        Action::UpdateEntry { entry } => ops::handle_update_entry(entry, state).await,
        Action::CopyToClipboard { .. } => Response::Error {
            message: "CopyToClipboard not implemented in agent; handle in client".to_string(),
        },
        Action::PinEntry { id } => ops::handle_pin_entry(id, state).await,
        Action::UnpinEntry { id } => ops::handle_unpin_entry(id, state).await,
        Action::AddEntry {
            name,
            entry_type,
            username,
            password,
            notes,
            fields,
        } => ops::handle_add_entry(name, entry_type, username, password, notes, fields, state).await,
        Action::AddSecureNote {
            name,
            notes,
            fields,
        } => {
            ops::handle_add_entry(
                name,
                cosmic_bwarden_core::protocol::EntryType::SecureNote,
                None,
                None,
                Some(notes),
                fields,
                state,
            )
            .await
        }
        Action::AddSshKey {
            name,
            private_key,
            public_key,
            notes,
            fields,
        } => {
            ops::handle_add_ssh_key(name, private_key, public_key, notes, fields, state).await
        }
        Action::AddCard {
            name,
            cardholder_name,
            brand,
            number,
            exp_month,
            exp_year,
            code,
            notes,
            fields,
        } => {
            ops::handle_add_card(
                name,
                cardholder_name,
                brand,
                number,
                exp_month,
                exp_year,
                code,
                notes,
                fields,
                state,
            )
            .await
        }
        Action::AddIdentity {
            name,
            first_name,
            last_name,
            address1,
            city,
            state: state_code,
            postal_code,
            country,
            email,
            phone,
            notes,
            fields,
        } => {
            ops::handle_add_identity(
                name,
                first_name,
                last_name,
                address1,
                city,
                state_code,
                postal_code,
                country,
                email,
                phone,
                notes,
                fields,
                state,
            )
            .await
        }
        Action::GetTopFrequent { limit, .. } => query::handle_get_top_frequent(limit, state).await,
        _ => Response::Error {
            message: "not implemented in vault handler".to_string(),
        },
    }
}
