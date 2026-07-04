//! Defense against secret-wiping updates.
//!
//! Bulk reads (`GetEntries`, sidebar) redact secrets to `None` so that
//! browsing never carries plaintext (see `query::redact_entry_secrets`). If a
//! client echoes such a redacted copy back through `UpdateEntry`, `None` must
//! mean "unchanged", not "clear" — otherwise the server copy of the secret is
//! silently destroyed. Clearing a secret on purpose is expressed as
//! `Some("")`, which is what the UI edit form sends.
//!
//! `notes` is intentionally NOT merged: the UI maps an emptied notes editor to
//! `None`, so `None` there is a legitimate clear.

use cosmic_bwarden_core::api::FieldType;
use cosmic_bwarden_core::db::{Entry, EntryData};

/// Fill redacted (`None`) secret slots in `entry` from the decrypted `stored`
/// entry: login password/TOTP, card number/code, SSH private key, and hidden
/// custom-field values.
pub fn merge_redacted_secrets(entry: &mut Entry, stored: &Entry) {
    match (&mut entry.data, &stored.data) {
        (
            EntryData::Login { password, totp, .. },
            EntryData::Login {
                password: stored_password,
                totp: stored_totp,
                ..
            },
        ) => {
            if password.is_none() {
                *password = stored_password.clone();
            }
            if totp.is_none() {
                *totp = stored_totp.clone();
            }
        }
        (
            EntryData::Card { number, code, .. },
            EntryData::Card {
                number: stored_number,
                code: stored_code,
                ..
            },
        ) => {
            if number.is_none() {
                *number = stored_number.clone();
            }
            if code.is_none() {
                *code = stored_code.clone();
            }
        }
        (
            EntryData::SshKey { private_key, .. },
            EntryData::SshKey {
                private_key: stored_private_key,
                ..
            },
        )
            if private_key.is_none() => {
                *private_key = stored_private_key.clone();
            }
        _ => {}
    }
    for field in &mut entry.fields {
        if field.ty == Some(FieldType::Hidden) && field.value.is_none() {
            if let Some(stored_field) = stored
                .fields
                .iter()
                .find(|f| f.name == field.name && f.ty == Some(FieldType::Hidden))
            {
                field.value = stored_field.value.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_bwarden_core::api::CipherRepromptType;
    use cosmic_bwarden_core::db::Field;

    fn login_entry(password: Option<&str>, totp: Option<&str>) -> Entry {
        Entry {
            id: "id1".to_string(),
            org_id: None,
            folder: None,
            folder_id: None,
            name: "e".to_string(),
            favorite: false,
            data: EntryData::Login {
                username: Some("u".to_string()),
                password: password.map(|p| p.to_string().into()),
                totp: totp.map(|t| t.to_string().into()),
                uris: Vec::new(),
            },
            fields: Vec::new(),
            notes: None,
            history: Vec::new(),
            key: None,
            master_password_reprompt: CipherRepromptType::None,
        }
    }

    #[test]
    fn redacted_password_and_totp_are_restored() {
        let mut redacted = login_entry(None, None);
        let stored = login_entry(Some("secret"), Some("totp-key"));
        merge_redacted_secrets(&mut redacted, &stored);
        match &redacted.data {
            EntryData::Login { password, totp, .. } => {
                assert_eq!(password.as_deref(), Some("secret"));
                assert_eq!(totp.as_deref(), Some("totp-key"));
            }
            _ => panic!("expected login"),
        }
    }

    #[test]
    fn explicit_empty_password_is_preserved_as_clear() {
        let mut edited = login_entry(Some(""), None);
        let stored = login_entry(Some("secret"), None);
        merge_redacted_secrets(&mut edited, &stored);
        match &edited.data {
            EntryData::Login { password, .. } => {
                assert_eq!(password.as_deref(), Some(""), "Some(\"\") must stay a clear");
            }
            _ => panic!("expected login"),
        }
    }

    #[test]
    fn hidden_field_value_is_restored_by_name() {
        let mut redacted = login_entry(Some("p"), None);
        redacted.fields.push(Field {
            ty: Some(FieldType::Hidden),
            name: Some("api-key".to_string()),
            value: None,
            linked_id: None,
        });
        let mut stored = login_entry(Some("p"), None);
        stored.fields.push(Field {
            ty: Some(FieldType::Hidden),
            name: Some("api-key".to_string()),
            value: Some("hidden-value".into()),
            linked_id: None,
        });
        merge_redacted_secrets(&mut redacted, &stored);
        assert_eq!(redacted.fields[0].value.as_deref(), Some("hidden-value"));
    }
}
