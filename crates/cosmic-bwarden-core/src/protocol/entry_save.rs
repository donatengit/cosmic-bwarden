//! Maps an edited `Entry` to the agent action that persists it.
//!
//! A client composing a brand-new entry has no server id yet, so it holds the
//! draft under a placeholder (`new-<unix_secs>`, minted by the UI's
//! `Message::AddEntryRequested`). Routing that draft through `UpdateEntry`
//! makes the agent `PUT /ciphers/new-...`, which Bitwarden rejects with HTTP
//! 400 ("The value 'new-…' is not valid"), so creation must dispatch the
//! matching `Add*` action instead.
//!
//! This lives in core rather than in the UI so the E2E suite can drive the
//! exact mapping the UI uses against a real server — the seam where that bug
//! hid, with the UI tested on one side and the agent on the other.

use crate::db::{Entry, EntryData, Secret};
use crate::protocol::{Action as AgentAction, EntryType};

/// Prefix of the placeholder id given to entries that exist only client-side.
pub const NEW_ENTRY_ID_PREFIX: &str = "new-";

/// Mint the placeholder id for a draft the server has not seen yet. Minting
/// and detection share this constant so a client can't drift into producing
/// ids that [`is_new`] fails to recognize — which would route the draft
/// straight back into `UpdateEntry`.
pub fn new_placeholder_id() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}{}", NEW_ENTRY_ID_PREFIX, secs)
}

/// True when `entry` has never been persisted to the server.
pub fn is_new(entry: &Entry) -> bool {
    entry.id.is_empty() || entry.id.starts_with(NEW_ENTRY_ID_PREFIX)
}

/// Drop fields the user left blank so creation doesn't store empty secrets.
fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.is_empty())
}

fn non_empty_secret(value: Option<Secret>) -> Option<Secret> {
    value.filter(|v| !v.expose().is_empty())
}

/// Pick the agent action that saves `entry`: an `Add*` for a new entry,
/// `UpdateEntry` for one the server already knows.
pub fn save_action(entry: Entry) -> AgentAction {
    if !is_new(&entry) {
        return AgentAction::UpdateEntry { entry };
    }

    let Entry {
        name,
        data,
        fields,
        notes,
        ..
    } = entry;

    match data {
        EntryData::Login {
            username,
            password,
            totp,
            uris,
        } => AgentAction::AddEntry {
            name,
            entry_type: EntryType::Login,
            username: non_empty(username),
            password: non_empty_secret(password),
            notes,
            fields,
            totp: non_empty_secret(totp),
            uris,
        },
        EntryData::SecureNote => AgentAction::AddEntry {
            name,
            entry_type: EntryType::SecureNote,
            username: None,
            password: None,
            notes,
            fields,
            totp: None,
            uris: Vec::new(),
        },
        EntryData::SshKey {
            private_key,
            public_key,
            ..
        } => AgentAction::AddSshKey {
            name,
            private_key: non_empty_secret(private_key).unwrap_or_else(|| Secret::from("")),
            public_key: non_empty(public_key),
            notes,
            fields,
        },
        EntryData::Card {
            cardholder_name,
            brand,
            number,
            exp_month,
            exp_year,
            code,
        } => AgentAction::AddCard {
            name,
            cardholder_name: non_empty(cardholder_name),
            number: non_empty_secret(number),
            brand: non_empty(brand),
            exp_month: non_empty(exp_month),
            exp_year: non_empty(exp_year),
            code: non_empty_secret(code),
            notes,
            fields,
        },
        EntryData::Identity {
            first_name,
            last_name,
            address1,
            city,
            state,
            postal_code,
            country,
            email,
            phone,
            ..
        } => AgentAction::AddIdentity {
            name,
            first_name: non_empty(first_name),
            last_name: non_empty(last_name),
            address1: non_empty(address1),
            city: non_empty(city),
            state: non_empty(state),
            postal_code: non_empty(postal_code),
            country: non_empty(country),
            email: non_empty(email),
            phone: non_empty(phone),
            notes,
            fields,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, data: EntryData) -> Entry {
        Entry {
            id: id.to_string(),
            org_id: None,
            folder: None,
            folder_id: None,
            name: "Example".to_string(),
            favorite: false,
            data,
            fields: Vec::new(),
            notes: None,
            history: Vec::new(),
            key: None,
            master_password_reprompt: crate::api::CipherRepromptType::None,
        }
    }

    fn login() -> EntryData {
        EntryData::Login {
            username: Some("alice".to_string()),
            password: Some(Secret::from("hunter2")),
            totp: None,
            uris: Vec::new(),
        }
    }

    #[test]
    fn new_login_creates_instead_of_updating() {
        // Regression: a `new-<ts>` id sent via UpdateEntry became
        // `PUT /ciphers/new-<ts>` and was rejected with HTTP 400.
        match save_action(entry("new-1785237518", login())) {
            AgentAction::AddEntry {
                entry_type,
                username,
                password,
                ..
            } => {
                assert!(matches!(entry_type, EntryType::Login));
                assert_eq!(username.as_deref(), Some("alice"));
                assert_eq!(
                    password.map(|p| p.expose().to_string()),
                    Some("hunter2".to_string())
                );
            }
            other => panic!("expected AddEntry, got {:?}", other.variant_name()),
        }
    }

    #[test]
    fn existing_entry_still_updates() {
        let action = save_action(entry("48d20cf2-a97a-422b-844e-acf400d8e8d1", login()));
        assert!(matches!(action, AgentAction::UpdateEntry { .. }));
    }

    #[test]
    fn minted_placeholder_ids_are_recognized_as_new() {
        // Minting and detection must not drift apart: an id this module hands
        // out has to be one it also classifies as unsaved.
        let drafted = entry(&new_placeholder_id(), login());
        assert!(is_new(&drafted));
        assert!(matches!(save_action(drafted), AgentAction::AddEntry { .. }));
    }

    #[test]
    fn empty_id_counts_as_new() {
        assert!(matches!(
            save_action(entry("", login())),
            AgentAction::AddEntry { .. }
        ));
    }

    #[test]
    fn blank_fields_are_dropped_on_create() {
        let data = EntryData::Login {
            username: Some(String::new()),
            password: Some(Secret::from("")),
            totp: Some(Secret::from("")),
            uris: Vec::new(),
        };
        match save_action(entry("new-1", data)) {
            AgentAction::AddEntry {
                username,
                password,
                totp,
                ..
            } => {
                assert!(username.is_none());
                assert!(password.is_none());
                assert!(totp.is_none());
            }
            other => panic!("expected AddEntry, got {:?}", other.variant_name()),
        }
    }

    #[test]
    fn new_secure_note_and_ssh_key_route_to_their_actions() {
        assert!(matches!(
            save_action(entry("new-2", EntryData::SecureNote)),
            AgentAction::AddEntry {
                entry_type: EntryType::SecureNote,
                ..
            }
        ));

        let ssh = EntryData::SshKey {
            private_key: Some(Secret::from("PRIVATE")),
            public_key: Some("ssh-ed25519 AAAA".to_string()),
            fingerprint: None,
        };
        assert!(matches!(
            save_action(entry("new-3", ssh)),
            AgentAction::AddSshKey { .. }
        ));
    }
}
