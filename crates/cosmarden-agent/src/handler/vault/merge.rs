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

use cosmarden_core::api::FieldType;
use cosmarden_core::db::{Entry, EntryData};

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
        ) if private_key.is_none() => {
            *private_key = stored_private_key.clone();
        }
        (
            EntryData::BankAccount {
                account_number,
                routing_number,
                pin,
                iban,
                swift_code,
                branch_number,
                ..
            },
            EntryData::BankAccount {
                account_number: stored_number,
                routing_number: stored_routing,
                pin: stored_pin,
                iban: stored_iban,
                swift_code: stored_swift,
                branch_number: stored_branch,
                ..
            },
        ) => {
            if account_number.is_none() {
                *account_number = stored_number.clone();
            }
            if routing_number.is_none() {
                *routing_number = stored_routing.clone();
            }
            if pin.is_none() {
                *pin = stored_pin.clone();
            }
            if iban.is_none() {
                *iban = stored_iban.clone();
            }
            if swift_code.is_none() {
                *swift_code = stored_swift.clone();
            }
            if branch_number.is_none() {
                *branch_number = stored_branch.clone();
            }
        }
        (
            EntryData::DriversLicense { license_number, .. },
            EntryData::DriversLicense {
                license_number: stored_license_number,
                ..
            },
        ) if license_number.is_none() => {
            *license_number = stored_license_number.clone();
        }
        (
            EntryData::Passport {
                passport_number,
                national_identification_number,
                date_of_birth,
                ..
            },
            EntryData::Passport {
                passport_number: stored_passport_number,
                national_identification_number: stored_nin,
                date_of_birth: stored_dob,
                ..
            },
        ) => {
            if passport_number.is_none() {
                *passport_number = stored_passport_number.clone();
            }
            if national_identification_number.is_none() {
                *national_identification_number = stored_nin.clone();
            }
            if date_of_birth.is_none() {
                *date_of_birth = stored_dob.clone();
            }
        }
        (
            EntryData::Identity {
                ssn,
                license_number,
                passport_number,
                ..
            },
            EntryData::Identity {
                ssn: stored_ssn,
                license_number: stored_license,
                passport_number: stored_passport,
                ..
            },
        ) => {
            if ssn.is_none() {
                *ssn = stored_ssn.clone();
            }
            if license_number.is_none() {
                *license_number = stored_license.clone();
            }
            if passport_number.is_none() {
                *passport_number = stored_passport.clone();
            }
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
    use cosmarden_core::api::CipherRepromptType;
    use cosmarden_core::db::Field;

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
                assert_eq!(
                    password.as_deref(),
                    Some(""),
                    "Some(\"\") must stay a clear"
                );
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

    fn identity_entry(ssn: Option<&str>) -> Entry {
        Entry {
            id: "id1".to_string(),
            org_id: None,
            folder: None,
            folder_id: None,
            name: "e".to_string(),
            favorite: false,
            data: EntryData::Identity {
                title: None,
                first_name: None,
                middle_name: None,
                last_name: None,
                address1: None,
                address2: None,
                address3: None,
                city: None,
                state: None,
                postal_code: None,
                country: None,
                phone: None,
                email: None,
                ssn: ssn.map(str::to_string),
                license_number: None,
                passport_number: None,
                username: None,
            },
            fields: Vec::new(),
            notes: None,
            history: Vec::new(),
            key: None,
            master_password_reprompt: CipherRepromptType::None,
        }
    }

    #[test]
    fn redacted_ssn_is_restored_not_cleared() {
        let mut redacted = identity_entry(None);
        let stored = identity_entry(Some("111-22-3333"));
        merge_redacted_secrets(&mut redacted, &stored);
        match &redacted.data {
            EntryData::Identity { ssn, .. } => {
                assert_eq!(ssn.as_deref(), Some("111-22-3333"));
            }
            _ => panic!("expected identity"),
        }
    }
}
