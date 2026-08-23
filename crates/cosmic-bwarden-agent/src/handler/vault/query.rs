use crate::state::State;
use cosmic_bwarden_core::db::{Db, Entry, EntryData};
use cosmic_bwarden_core::protocol::{EntryType, Response, SidebarEntry};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Strip every on-demand secret from a decrypted entry, leaving only metadata
/// (name, username, uris, public key, and non-PII identity/card labels). Used by
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
        EntryData::BankAccount {
            account_number,
            routing_number,
            pin,
            iban,
            swift_code,
            branch_number,
            ..
        } => {
            *account_number = None;
            *routing_number = None;
            *pin = None;
            *iban = None;
            *swift_code = None;
            *branch_number = None;
        }
        EntryData::DriversLicense { license_number, .. } => {
            *license_number = None;
        }
        EntryData::Passport {
            passport_number,
            national_identification_number,
            date_of_birth,
            ..
        } => {
            *passport_number = None;
            *national_identification_number = None;
            *date_of_birth = None;
        }
        EntryData::Identity {
            ssn,
            license_number,
            passport_number,
            ..
        } => {
            *ssn = None;
            *license_number = None;
            *passport_number = None;
        }
        EntryData::SecureNote => {}
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
        email,
        &pw,
        kdf,
        iterations,
        db.memory,
        db.parallelism,
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
            if !cosmic_bwarden_core::ct_eq(identity.master_password_hash.hash(), stored_hash.hash())
            {
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
    domain: Option<String>,
    state: &Arc<Mutex<State>>,
) -> Response {
    let state_guard = state.lock().await;
    if state_guard.keys.is_none() {
        return Response::Error {
            message: "agent is locked".to_string(),
        };
    }

    let q = query.as_deref().map(str::to_lowercase);
    // A typed search wins over the tab-domain filter (the popup sends one or
    // the other; defined here so mixed clients behave predictably).
    let d = match &q {
        None => domain
            .as_deref()
            .and_then(cosmic_bwarden_core::domain::host_from_uri),
        Some(_) => None,
    };
    let entries: Vec<SidebarEntry> = state_guard
        .sidebar_cache
        .iter()
        .filter(|c| {
            let e = &c.entry;
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
            if let Some(d) = &d {
                return c
                    .hosts
                    .iter()
                    .any(|h| cosmic_bwarden_core::domain::hosts_match(h, d));
            }
            true
        })
        .map(|c| c.entry.clone())
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
                    (EntryType::Identity, cosmic_bwarden_core::db::EntryData::Identity { .. }) => {}
                    (EntryType::SecureNote, cosmic_bwarden_core::db::EntryData::SecureNote) => (),
                    (EntryType::SshKey, cosmic_bwarden_core::db::EntryData::SshKey { .. }) => (),
                    (
                        EntryType::BankAccount,
                        cosmic_bwarden_core::db::EntryData::BankAccount { .. },
                    ) => (),
                    (
                        EntryType::DriversLicense,
                        cosmic_bwarden_core::db::EntryData::DriversLicense { .. },
                    ) => (),
                    (EntryType::Passport, cosmic_bwarden_core::db::EntryData::Passport { .. }) => {}
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
                        username: Some(u), ..
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

#[cfg(test)]
mod ct_eq_site {
    #[test]
    fn verify_reprompt_uses_ct_eq() {
        let src = include_str!("query.rs");
        assert!(
            src.contains("cosmic_bwarden_core::ct_eq"),
            "reprompt hash compare must use ct_eq"
        );
        let non_ct = ["hash()", " != ", "stored_hash.hash()"].concat();
        assert!(!src.contains(&non_ct), "non-CT != compare must not remain");
    }
}

#[cfg(test)]
mod redact_pii_tests {
    use super::redact_entry_secrets;
    use cosmic_bwarden_core::api::CipherRepromptType;
    use cosmic_bwarden_core::db::{Entry, EntryData};

    fn blank_entry(data: EntryData) -> Entry {
        Entry {
            id: "id".into(),
            org_id: None,
            folder: None,
            folder_id: None,
            name: "n".into(),
            favorite: false,
            data,
            fields: Vec::new(),
            notes: None,
            history: Vec::new(),
            key: None,
            master_password_reprompt: CipherRepromptType::None,
        }
    }

    #[test]
    fn identity_pii_is_stripped_from_meta() {
        let mut e = blank_entry(EntryData::Identity {
            title: None,
            first_name: Some("Ada".into()),
            middle_name: None,
            last_name: Some("Lovelace".into()),
            address1: None,
            address2: None,
            address3: None,
            city: None,
            state: None,
            postal_code: None,
            country: None,
            phone: None,
            email: None,
            ssn: Some("111-22-3333".into()),
            license_number: Some("DL-9".into()),
            passport_number: Some("P-1".into()),
            username: Some("ada".into()),
        });
        redact_entry_secrets(&mut e);
        match &e.data {
            EntryData::Identity {
                first_name,
                ssn,
                license_number,
                passport_number,
                username,
                ..
            } => {
                assert_eq!(first_name.as_deref(), Some("Ada"));
                assert_eq!(username.as_deref(), Some("ada"));
                assert!(ssn.is_none());
                assert!(license_number.is_none());
                assert!(passport_number.is_none());
            }
            _ => panic!("identity"),
        }
    }

    #[test]
    fn bank_iban_class_is_stripped_from_meta() {
        let mut e = blank_entry(EntryData::BankAccount {
            bank_name: Some("Bank".into()),
            name_on_account: None,
            account_type: None,
            account_number: Some("123".into()),
            routing_number: None,
            branch_number: Some("BR-1".into()),
            pin: None,
            swift_code: Some("SWFT".into()),
            iban: Some("DE00".into()),
            bank_contact_phone: None,
        });
        redact_entry_secrets(&mut e);
        match &e.data {
            EntryData::BankAccount {
                bank_name,
                iban,
                swift_code,
                branch_number,
                account_number,
                ..
            } => {
                assert_eq!(bank_name.as_deref(), Some("Bank"));
                assert!(iban.is_none());
                assert!(swift_code.is_none());
                assert!(branch_number.is_none());
                assert!(account_number.is_none());
            }
            _ => panic!("bank"),
        }
    }
}
