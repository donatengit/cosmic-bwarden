//! Detail-pane field access: which rows are secret, which agent action loads
//! one on demand, and what plaintext the copy button should put on the
//! clipboard.
//!
//! Shared by the UI (which builds the buttons) and the E2E suite (which drives
//! the same decision against a real agent and Vaultwarden), so a button and its
//! test cannot drift apart. The keys are stable literals, matching
//! `view::vault::field::field_label` and the agent's `filled_secret_keys`.

use crate::db::{Entry, EntryData, Secret};
use crate::protocol::{Action, Response};

/// Whether a detail row must render masked and fetch its value on demand.
///
/// `is_hidden` is the caller's type- or custom-field answer. The label fallback
/// covers custom fields whose names look secret. A *public* key is explicitly
/// not a secret even though its label contains "key" — classifying it as one
/// routed its copy button through `GetEntry`, and `field_plaintext` had no
/// public-key arm, so the clipboard silently stayed unchanged. A hidden custom
/// field that happens to be named "Public Key" is still secret.
pub fn field_is_secret(label: &str, is_hidden: bool) -> bool {
    if !is_hidden && label.eq_ignore_ascii_case("public key") {
        return false;
    }
    is_hidden
        || label.eq_ignore_ascii_case("TOTP")
        || label.to_lowercase().contains("password")
        || label.to_lowercase().contains("key")
}

/// The agent action behind a detail field's copy/reveal button.
///
/// Login password → `GetPassword`; TOTP → `GetTotp` (the live code, never the
/// stored seed); everything else → `GetEntry`, which returns the full decrypted
/// entry for `field_plaintext` to read.
pub fn on_demand_secret(field: &str, id: String, reprompt_password: Option<String>) -> Action {
    match field {
        "Password" => Action::GetPassword {
            id,
            password: reprompt_password,
        },
        "TOTP" => Action::GetTotp {
            id,
            password: reprompt_password,
        },
        _ => Action::GetEntry {
            id,
            password: reprompt_password,
        },
    }
}

/// The parsed answer to [`on_demand_secret`].
///
/// `Entry` is boxed: that variant is ~600 bytes larger than the string
/// variants, which trips `clippy::large-enum-variant` (the workspace denies
/// warnings). Boxing is free here because the payload is transient.
#[derive(Clone)]
pub enum OnDemandPayload {
    Password(Secret),
    Totp(Secret),
    Entry(Box<Entry>),
}

/// Decode the agent response for an on-demand secret fetch.
pub fn parse_on_demand_response(
    field: &str,
    response: Response,
) -> Result<OnDemandPayload, String> {
    match (field, response) {
        ("Password", Response::Password { password }) => Ok(OnDemandPayload::Password(password)),
        ("TOTP", Response::Totp { code }) => Ok(OnDemandPayload::Totp(code)),
        (_, Response::Entry { entry }) if field != "Password" && field != "TOTP" => {
            Ok(OnDemandPayload::Entry(Box::new(entry)))
        }
        (_, Response::Error { message }) => Err(message),
        (_, other) => Err(format!("unexpected response: {other:?}")),
    }
}

/// Whether `entry` already holds plaintext for `field` (no extra IPC).
/// TOTP always fetches: the pane shows a live code, not the stored seed.
pub fn secret_is_loaded(entry: &Entry, field: &str) -> bool {
    // Notes are a secret-class slot: a body in the entry means a `GetEntry`
    // brought it back, since every meta/bulk read redacts it.
    if field == "Notes" {
        return entry.notes.is_some();
    }
    match (&entry.data, field) {
        (
            EntryData::Login {
                password: Some(_), ..
            },
            "Password",
        ) => true,
        (EntryData::Login { .. }, "TOTP") => false,
        (
            EntryData::SshKey {
                private_key: Some(_),
                ..
            },
            "Private Key",
        ) => true,
        (
            EntryData::SshKey {
                public_key: Some(_),
                ..
            },
            "Public Key",
        ) => true,
        (
            EntryData::Card {
                number: Some(_), ..
            },
            "Card Number",
        ) => true,
        (EntryData::Card { code: Some(_), .. }, "Security Code") => true,
        (
            EntryData::BankAccount {
                account_number: Some(_),
                ..
            },
            "Account Number",
        ) => true,
        (
            EntryData::BankAccount {
                routing_number: Some(_),
                ..
            },
            "Routing Number",
        ) => true,
        (EntryData::BankAccount { pin: Some(_), .. }, "PIN") => true,
        (EntryData::BankAccount { iban: Some(_), .. }, "IBAN") => true,
        (
            EntryData::BankAccount {
                swift_code: Some(_),
                ..
            },
            "SWIFT Code",
        ) => true,
        (
            EntryData::BankAccount {
                branch_number: Some(_),
                ..
            },
            "Branch Number",
        ) => true,
        (EntryData::Identity { ssn: Some(_), .. }, "SSN") => true,
        (
            EntryData::Identity {
                license_number: Some(_),
                ..
            },
            "License Number",
        ) => true,
        (
            EntryData::Identity {
                passport_number: Some(_),
                ..
            },
            "Passport Number",
        ) => true,
        (
            EntryData::DriversLicense {
                license_number: Some(_),
                ..
            },
            "License Number",
        ) => true,
        (
            EntryData::Passport {
                passport_number: Some(_),
                ..
            },
            "Passport Number",
        ) => true,
        (
            EntryData::Passport {
                national_identification_number: Some(_),
                ..
            },
            "National Identification Number",
        ) => true,
        (
            EntryData::Passport {
                date_of_birth: Some(_),
                ..
            },
            "Date of Birth",
        ) => true,
        _ => entry
            .fields
            .iter()
            .any(|f| f.name.as_deref() == Some(field) && f.value.is_some()),
    }
}

/// The plaintext a detail field's copy button should copy, read from a fully
/// decrypted entry (`GetEntry`, or a meta read for slots that are not secret).
pub fn field_plaintext(entry: &Entry, field: &str) -> Option<String> {
    // Notes any entry type can carry, and one of the slots the meta read
    // redacts: the plaintext is only here once an explicit reveal loaded it.
    if field == "Notes" {
        return entry.notes.as_ref().map(|n| n.expose().to_string());
    }
    match (&entry.data, field) {
        (
            EntryData::Login {
                password: Some(p), ..
            },
            "Password",
        ) => Some(p.expose().to_string()),
        (
            EntryData::SshKey {
                private_key: Some(p),
                ..
            },
            "Private Key",
        ) => Some(p.expose().to_string()),
        (
            EntryData::SshKey {
                public_key: Some(k),
                ..
            },
            "Public Key",
        ) => Some(k.clone()),
        (
            EntryData::Card {
                number: Some(n), ..
            },
            "Card Number",
        ) => Some(n.expose().to_string()),
        (EntryData::Card { code: Some(c), .. }, "Security Code") => Some(c.expose().to_string()),
        (
            EntryData::BankAccount {
                account_number: Some(v),
                ..
            },
            "Account Number",
        ) => Some(v.expose().to_string()),
        (
            EntryData::BankAccount {
                routing_number: Some(v),
                ..
            },
            "Routing Number",
        ) => Some(v.expose().to_string()),
        (EntryData::BankAccount { pin: Some(v), .. }, "PIN") => Some(v.expose().to_string()),
        (EntryData::BankAccount { iban: Some(v), .. }, "IBAN") => Some(v.clone()),
        (
            EntryData::BankAccount {
                swift_code: Some(v),
                ..
            },
            "SWIFT Code",
        ) => Some(v.clone()),
        (
            EntryData::BankAccount {
                branch_number: Some(v),
                ..
            },
            "Branch Number",
        ) => Some(v.clone()),
        (EntryData::Identity { ssn: Some(v), .. }, "SSN") => Some(v.clone()),
        (
            EntryData::Identity {
                license_number: Some(v),
                ..
            },
            "License Number",
        ) => Some(v.clone()),
        (
            EntryData::Identity {
                passport_number: Some(v),
                ..
            },
            "Passport Number",
        ) => Some(v.clone()),
        (
            EntryData::DriversLicense {
                license_number: Some(v),
                ..
            },
            "License Number",
        ) => Some(v.expose().to_string()),
        (
            EntryData::Passport {
                passport_number: Some(v),
                ..
            },
            "Passport Number",
        ) => Some(v.expose().to_string()),
        (
            EntryData::Passport {
                national_identification_number: Some(v),
                ..
            },
            "National Identification Number",
        ) => Some(v.clone()),
        (
            EntryData::Passport {
                date_of_birth: Some(v),
                ..
            },
            "Date of Birth",
        ) => Some(v.clone()),
        _ => entry.fields.iter().find_map(|f| {
            (f.name.as_deref() == Some(field))
                .then(|| f.value.as_ref().map(|s| s.expose().to_string()))
                .flatten()
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ssh_entry() -> Entry {
        Entry {
            id: "ssh-1".into(),
            org_id: None,
            folder: None,
            folder_id: None,
            name: "key".into(),
            data: EntryData::SshKey {
                private_key: Some("PRIVATE".into()),
                public_key: Some("ssh-ed25519 AAAA test".into()),
                fingerprint: Some("SHA256:abc".into()),
            },
            fields: Vec::new(),
            notes: None,
            history: Vec::new(),
            key: None,
            master_password_reprompt: crate::api::CipherRepromptType::None,
            favorite: false,
        }
    }

    #[test]
    fn public_key_is_not_a_secret() {
        assert!(!field_is_secret("Public Key", false));
        // A *hidden* custom field with that name is still secret.
        assert!(field_is_secret("Public Key", true));
        assert!(field_is_secret("Private Key", false));
        assert!(field_is_secret("Password", false));
        assert!(field_is_secret("TOTP", false));
        assert!(!field_is_secret("Username", false));
        assert!(!field_is_secret("Notes", false));
        assert!(field_is_secret("API Key", false));
    }

    #[test]
    fn ssh_public_and_private_keys_are_copyable() {
        let entry = ssh_entry();
        assert_eq!(
            field_plaintext(&entry, "Public Key").as_deref(),
            Some("ssh-ed25519 AAAA test")
        );
        assert_eq!(
            field_plaintext(&entry, "Private Key").as_deref(),
            Some("PRIVATE")
        );
        assert!(secret_is_loaded(&entry, "Public Key"));
        assert!(secret_is_loaded(&entry, "Private Key"));
    }

    #[test]
    fn card_number_and_security_code_both_have_plaintext() {
        let entry = Entry {
            id: "card-1".into(),
            org_id: None,
            folder: None,
            folder_id: None,
            name: "card".into(),
            data: EntryData::Card {
                cardholder_name: None,
                number: Some("4111111111111111".into()),
                brand: None,
                exp_month: None,
                exp_year: None,
                code: Some("321".into()),
            },
            fields: Vec::new(),
            notes: None,
            history: Vec::new(),
            key: None,
            master_password_reprompt: crate::api::CipherRepromptType::None,
            favorite: false,
        };
        assert!(secret_is_loaded(&entry, "Card Number"));
        assert!(secret_is_loaded(&entry, "Security Code"));
        assert_eq!(
            field_plaintext(&entry, "Card Number").as_deref(),
            Some("4111111111111111")
        );
        assert_eq!(
            field_plaintext(&entry, "Security Code").as_deref(),
            Some("321")
        );
    }

    #[test]
    fn redacted_ssh_entry_reports_private_key_unloaded_but_public_key_loaded() {
        let mut entry = ssh_entry();
        if let EntryData::SshKey { private_key, .. } = &mut entry.data {
            *private_key = None;
        }
        assert!(!secret_is_loaded(&entry, "Private Key"));
        assert!(secret_is_loaded(&entry, "Public Key"));
        assert_eq!(field_plaintext(&entry, "Private Key"), None);
        assert_eq!(
            field_plaintext(&entry, "Public Key").as_deref(),
            Some("ssh-ed25519 AAAA test")
        );
    }
}
