use crate::state::State;
use cosmarden_core::db::{Db, Entry, EntryData};
use cosmarden_core::protocol::{EntryType, Response, SidebarEntry};
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
        if field.ty == Some(cosmarden_core::api::FieldType::Hidden) {
            field.value = None;
        }
    }
}

/// Names of the currently-filled secret slots in `entry`, as stable field keys
/// (the same keys the UI's `field_label` mapping and the desktop detail pane
/// use for dispatch). Computed *before* [`redact_entry_secrets`] so presence
/// survives redaction; the values themselves are never returned. Custom
/// (user-defined) fields appear by their plaintext `name`. Mirrors the set of
/// slots that [`redact_entry_secrets`] blanks, plus secondary secrets the
/// origin redaction also clears (`branch_number` here too), and `notes` (a
/// secret-class slot) — so a detail view can render a masked row for every
/// filled secret without ever pulling plaintext into the client.
pub fn filled_secret_keys(entry: &Entry) -> Vec<String> {
    use cosmarden_core::db::EntryData;
    let mut keys: Vec<String> = Vec::new();
    match &entry.data {
        EntryData::Login { password, totp, .. } => {
            if password.is_some() {
                keys.push("Password".into());
            }
            if totp.is_some() {
                keys.push("TOTP".into());
            }
        }
        EntryData::Card { number, code, .. } => {
            if number.is_some() {
                keys.push("Card Number".into());
            }
            if code.is_some() {
                keys.push("Security Code".into());
            }
        }
        EntryData::SshKey { private_key, .. } => {
            if private_key.is_some() {
                keys.push("Private Key".into());
            }
        }
        EntryData::BankAccount {
            account_number,
            routing_number,
            branch_number,
            pin,
            swift_code,
            iban,
            ..
        } => {
            if account_number.is_some() {
                keys.push("Account Number".into());
            }
            if routing_number.is_some() {
                keys.push("Routing Number".into());
            }
            if branch_number.is_some() {
                keys.push("Branch Number".into());
            }
            if pin.is_some() {
                keys.push("PIN".into());
            }
            if swift_code.is_some() {
                keys.push("SWIFT Code".into());
            }
            if iban.is_some() {
                keys.push("IBAN".into());
            }
        }
        EntryData::DriversLicense { license_number, .. } => {
            if license_number.is_some() {
                keys.push("License Number".into());
            }
        }
        EntryData::Passport {
            passport_number,
            national_identification_number,
            date_of_birth,
            ..
        } => {
            if passport_number.is_some() {
                keys.push("Passport Number".into());
            }
            if national_identification_number.is_some() {
                keys.push("National Identification Number".into());
            }
            if date_of_birth.is_some() {
                keys.push("Date of Birth".into());
            }
        }
        EntryData::Identity {
            ssn,
            license_number,
            passport_number,
            ..
        } => {
            if ssn.is_some() {
                keys.push("SSN".into());
            }
            if license_number.is_some() {
                keys.push("License Number".into());
            }
            if passport_number.is_some() {
                keys.push("Passport Number".into());
            }
        }
        EntryData::SecureNote => {}
    }
    if entry.notes.is_some() {
        keys.push("Notes".into());
    }
    // Hidden (user-designated secret) custom fields: their names are plain and
    // included (the value stays redacted), but their presence must be known to
    // render a masked row. Visible Text fields are NOT secret slots — their
    // values already travel in the redacted meta entry, so they are omitted.
    for field in &entry.fields {
        if field.ty == Some(cosmarden_core::api::FieldType::Hidden) && field.value.is_some() {
            if let Some(name) = field.name.as_deref() {
                keys.push(name.to_string());
            }
        }
    }
    keys
}

/// Verify the master password for a reprompt-gated entry.
/// Returns `Some(error)` if verification is required and failed/absent; `None` on
/// success. Runs synchronously (KDF) while the caller holds the state lock, matching
/// the existing reprompt path.
pub(super) fn verify_reprompt(provided: Option<String>, db: &Db) -> Option<Response> {
    let password = match provided {
        Some(p) => p,
        None => {
            return Some(Response::Error {
                message: "reprompt_required".to_string(),
            })
        }
    };

    let config = match cosmarden_core::config::CosmardenConfig::load_legacy() {
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

    let mut pw_vec = cosmarden_core::locked::Vec::new();
    pw_vec.extend(password.as_bytes().iter().copied());
    let pw = cosmarden_core::locked::Password::new(pw_vec);

    let kdf = db.kdf.unwrap_or(cosmarden_core::api::KdfType::Pbkdf2);
    let iterations = db.iterations.unwrap_or(100_000);

    let identity = match cosmarden_core::identity::Identity::new(
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

    // Prove the password by re-deriving the vault key and decrypting
    // `protected_key` — the same proof `unlock` uses, and the reason the agent
    // need not keep the master-password hash in memory at all. It also works
    // after a PIN unlock, where no hash is ever derived; comparing against a
    // stored hash would fail outright there.
    let empty_org_keys: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    match cosmarden_core::vault::unlock_from_keys(
        &identity.keys,
        db.protected_key.as_ref().map(|s| s.expose()).unwrap_or(""),
        None,
        &empty_org_keys,
    ) {
        Ok(_) => None,
        Err(_) => Some(Response::Error {
            message: "incorrect password".to_string(),
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
            .and_then(cosmarden_core::domain::host_from_uri),
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
                    .any(|h| cosmarden_core::domain::hosts_match(h, d));
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
                    (EntryType::Login, cosmarden_core::db::EntryData::Login { .. }) => (),
                    (EntryType::Card, cosmarden_core::db::EntryData::Card { .. }) => (),
                    (EntryType::Identity, cosmarden_core::db::EntryData::Identity { .. }) => {}
                    (EntryType::SecureNote, cosmarden_core::db::EntryData::SecureNote) => (),
                    (EntryType::SshKey, cosmarden_core::db::EntryData::SshKey { .. }) => (),
                    (EntryType::BankAccount, cosmarden_core::db::EntryData::BankAccount { .. }) => {
                        ()
                    }
                    (
                        EntryType::DriversLicense,
                        cosmarden_core::db::EntryData::DriversLicense { .. },
                    ) => (),
                    (EntryType::Passport, cosmarden_core::db::EntryData::Passport { .. }) => {}
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
                    if let cosmarden_core::db::EntryData::Login {
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
            let filled_secrets = filled_secret_keys(&decrypted);
            redact_entry_secrets(&mut decrypted);
            Response::EntryMeta {
                entry: decrypted,
                filled_secrets,
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
                if let Some(err) = verify_reprompt(password, db) {
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
mod reprompt_proof_site {
    /// The reprompt proves the password by decrypting `protected_key` (MAC
    /// verified before decrypt), so the agent never needs the master-password
    /// hash in memory. Guard both halves — a future "optimisation" that caches
    /// the hash to skip the KDF would silently reintroduce it.
    #[test]
    fn reprompt_proves_by_decryption_and_stores_no_hash() {
        let src = include_str!("query.rs");
        assert!(
            src.contains("unlock_from_keys"),
            "reprompt must prove the password by decrypting protected_key"
        );
        let field = ["master", "_password_", "hash"].concat();
        assert!(
            !src.contains(&field),
            "reprompt must not consult a stored master-password hash"
        );
        assert!(
            !include_str!("../../state.rs").contains(&field),
            "State must not retain the master-password hash"
        );
    }
}

#[cfg(test)]
mod redact_pii_tests {
    use super::redact_entry_secrets;
    use cosmarden_core::api::CipherRepromptType;
    use cosmarden_core::db::{Entry, EntryData};

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

    #[test]
    fn filled_secret_keys_reports_filled_slots_before_redaction() {
        use super::filled_secret_keys;
        let mut e = blank_entry(EntryData::Login {
            username: Some("u".into()),
            password: Some("p".into()),
            totp: Some("seed".into()),
            uris: Vec::new(),
        });
        e.notes = Some("note".into());
        e.set_field("Token", "abc", cosmarden_core::api::FieldType::Hidden);
        e.set_field("Public", "x", cosmarden_core::api::FieldType::Text);

        let keys = filled_secret_keys(&e);
        assert!(keys.contains(&"Password".to_string()));
        assert!(keys.contains(&"TOTP".to_string()));
        assert!(keys.contains(&"Notes".to_string()));
        // Only hidden custom fields are secret slots; visible Text fields are
        // not (their values already travel in the redacted meta entry).
        assert!(keys.contains(&"Token".to_string()));
        assert!(!keys.contains(&"Public".to_string()));

        // After redaction the values are gone, so a *second* derivation would
        // report fewer keys — which is exactly why handle_get_entry_meta calls
        // filled_secret_keys before redact_entry_secrets.
        redact_entry_secrets(&mut e);
        let keys_after = filled_secret_keys(&e);
        assert!(keys_after
            .iter()
            .find(|k| k.as_str() == "Password")
            .is_none());
        assert!(keys_after.iter().find(|k| k.as_str() == "Token").is_none());
    }

    #[test]
    fn filled_secret_keys_omits_empty_slots() {
        use super::filled_secret_keys;
        let e = blank_entry(EntryData::Login {
            username: None,
            password: None,
            totp: None,
            uris: Vec::new(),
        });
        let keys = filled_secret_keys(&e);
        assert!(!keys.contains(&"Password".to_string()));
        assert!(!keys.contains(&"TOTP".to_string()));
        assert!(!keys.contains(&"Notes".to_string()));
    }

    /// Cross-check that `filled_secret_keys` stays a mirror of what
    /// `redact_entry_secrets` blanks: a reported "filled secret" must be a slot
    /// that redaction actually clears, and a visible plaintext field must never
    /// be reported. Keeps the two lists from drifting when a new EntryData
    /// variant or secret field is added.
    #[test]
    fn filled_secret_keys_are_exactly_the_redacted_slots() {
        use super::filled_secret_keys;
        // Each slot is independent of locale/shape; check a few representative
        // variants. `#[track_caller]`-free helper inlined for clarity.
        fn assert_mirror(
            e: &Entry,
            expected_after_redaction_present: &[&str],
            visible_not_listed: &[&str],
        ) {
            let keys = filled_secret_keys(e);
            for k in expected_after_redaction_present {
                // Present *before* redaction (the handler computes it pre-redact),
                // confirmed by redact_entry_secrets actually blanking it.
                assert!(
                    keys.iter().any(|s| s.as_str() == *k),
                    "expected filled_secret_keys to contain {k} (redaction blanks it)"
                );
            }
            for v in visible_not_listed {
                assert!(
                    !keys.iter().any(|s| s.as_str() == *v),
                    "{v} is a visible field, must not appear in filled_secret_keys"
                );
            }
        }

        let login = blank_entry(EntryData::Login {
            username: Some("u".into()),
            password: Some("p".into()),
            totp: Some("seed".into()),
            uris: Vec::new(),
        });
        assert_mirror(&login, &["Password", "TOTP"], &["Username"]);

        let bank = blank_entry(EntryData::BankAccount {
            bank_name: Some("B".into()),
            name_on_account: None,
            account_type: None,
            account_number: Some("1".into()),
            routing_number: Some("R".into()),
            branch_number: Some("BR".into()),
            pin: Some("p".into()),
            swift_code: Some("SW".into()),
            iban: Some("DE".into()),
            bank_contact_phone: None,
        });
        assert_mirror(
            &bank,
            &[
                "Account Number",
                "Routing Number",
                "PIN",
                "IBAN",
                "SWIFT Code",
                "Branch Number",
            ],
            &["Bank Name", "Account Type"],
        );

        let card = blank_entry(EntryData::Card {
            cardholder_name: Some("C".into()),
            number: Some("4111".into()),
            code: Some("123".into()),
            brand: None,
            exp_month: None,
            exp_year: None,
        });
        assert_mirror(&card, &["Card Number", "Security Code"], &["Cardholder"]);

        let ssh = blank_entry(EntryData::SshKey {
            private_key: Some("pk".into()),
            public_key: Some("pub".into()),
            fingerprint: Some("fp".into()),
        });
        assert_mirror(&ssh, &["Private Key"], &["Public Key", "Fingerprint"]);
    }
}
