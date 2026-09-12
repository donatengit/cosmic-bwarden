//! Pure `state -> AgentAction` builders for the vault's agent round-trips.
//!
//! Every one of these used to be constructed *inside* the async block handed
//! to `Task::perform`, where no unit test can reach it: the test discards the
//! `Task`, so the action is built and thrown away unexamined. A wrong variant
//! there is invisible until it hits the server (see `entry_save` in core —
//! new entries were sent as `UpdateEntry` and rejected with HTTP 400).
//!
//! Keeping the *decision* out here — which variant, which fields — makes it
//! assertable without a runtime, an agent, or a network.

use cosmarden_core::protocol::{Action as AgentAction, EntryType};

/// Fetch an entry for the detail pane. A plain selection uses `GetEntryMeta`
/// (no secrets). Answering a master-password reprompt sends `GetEntry`.
pub fn fetch_entry(id: String, reprompt_password: Option<String>) -> AgentAction {
    match reprompt_password {
        None => AgentAction::GetEntryMeta { id },
        Some(password) => AgentAction::GetEntry {
            id,
            password: Some(password),
        },
    }
}

/// Full decrypted entry for edit. Secrets are only fetched on this explicit
/// gesture (or a reprompt), never on mere selection.
pub fn fetch_full_entry(id: String, reprompt_password: Option<String>) -> AgentAction {
    AgentAction::GetEntry {
        id,
        password: reprompt_password,
    }
}

/// Delete an entry the user has confirmed removing.
pub fn delete_entry(id: String) -> AgentAction {
    AgentAction::DeleteEntry { id }
}

/// Persist a pin toggle. `now_pinned` is the state the UI has already applied
/// optimistically, so this must send the action that *matches* it — sending
/// the inverse silently reverts the user's click on the next sync.
pub fn toggle_pin(id: String, now_pinned: bool) -> AgentAction {
    if now_pinned {
        AgentAction::PinEntry { id }
    } else {
        AgentAction::UnpinEntry { id }
    }
}

/// List entries for the vault sidebar. `domain` stays `None` here: domain
/// filtering is the browser extension's concern, and passing one would
/// silently hide entries from the desktop list.
pub fn sidebar_entries(
    query: Option<String>,
    entry_type: Option<EntryType>,
    only_pinned: bool,
) -> AgentAction {
    AgentAction::GetSidebarEntries {
        query,
        entry_type,
        only_pinned,
        domain: None,
    }
}

/// List entries for the applet's search field. Unlike the sidebar there is no
/// type filter — the applet shows every kind — so this is a distinct builder
/// rather than a defaulted call.
pub fn applet_search(query: Option<String>, only_pinned: bool) -> AgentAction {
    AgentAction::GetSidebarEntries {
        query,
        entry_type: None,
        only_pinned,
        domain: None,
    }
}

/// Fetch one entry's password on demand. `reprompt_password` is `Some` only
/// when answering a master-password reprompt.
pub fn fetch_password(id: String, reprompt_password: Option<String>) -> AgentAction {
    AgentAction::GetPassword {
        id,
        password: reprompt_password,
    }
}

/// TOTP *code* (not the seed) for the detail pane.
pub fn fetch_totp(id: String, reprompt_password: Option<String>) -> AgentAction {
    AgentAction::GetTotp {
        id,
        password: reprompt_password,
    }
}

/// On-demand secret for a detail-pane field after a `GetEntryMeta` selection.
/// Login password → `GetPassword`; TOTP → `GetTotp`; everything else → `GetEntry`.
pub fn on_demand_secret(field: &str, id: String, reprompt_password: Option<String>) -> AgentAction {
    match field {
        "Password" => fetch_password(id, reprompt_password),
        "TOTP" => fetch_totp(id, reprompt_password),
        _ => fetch_full_entry(id, reprompt_password),
    }
}

/// Whether `selected_entry` already holds plaintext for `field` (no extra IPC).
/// TOTP always fetches: the pane shows a live code, not the stored seed.
pub fn secret_is_loaded(entry: &cosmarden_core::db::Entry, field: &str) -> bool {
    use cosmarden_core::db::EntryData;
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

/// Resume a master-password reprompt with the intent that triggered it.
pub fn submit_reprompt_action(
    intent: Option<&crate::message::RepromptIntent>,
    id: String,
    password: String,
) -> AgentAction {
    match intent {
        Some(crate::message::RepromptIntent::Reveal { field })
        | Some(crate::message::RepromptIntent::Copy { field }) => {
            on_demand_secret(field, id, Some(password))
        }
        Some(crate::message::RepromptIntent::Edit) | None => fetch_full_entry(id, Some(password)),
    }
}

/// Decode the agent response for an on-demand secret fetch.
pub fn parse_on_demand_response(
    field: &str,
    response: cosmarden_core::protocol::Response,
) -> Result<crate::message::OnDemandPayload, String> {
    use crate::message::OnDemandPayload;
    use cosmarden_core::protocol::Response;
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

pub fn field_plaintext(entry: &cosmarden_core::db::Entry, field: &str) -> Option<String> {
    use cosmarden_core::db::EntryData;
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
            EntryData::Card {
                number: Some(n), ..
            },
            "Card Number",
        ) => Some(n.expose().to_string()),
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

    #[test]
    fn plain_selection_sends_get_entry_meta() {
        match fetch_entry("abc".to_string(), None) {
            AgentAction::GetEntryMeta { id } => {
                assert_eq!(id, "abc");
            }
            other => panic!("expected GetEntryMeta, got {}", other.variant_name()),
        }
    }

    #[test]
    fn reprompt_carries_the_master_password() {
        match fetch_entry("abc".to_string(), Some("master".to_string())) {
            AgentAction::GetEntry { password, .. } => {
                assert_eq!(password.as_deref(), Some("master"));
            }
            other => panic!("expected GetEntry, got {}", other.variant_name()),
        }
    }

    #[test]
    fn edit_builder_sends_get_entry() {
        match fetch_full_entry("abc".to_string(), None) {
            AgentAction::GetEntry { id, password } => {
                assert_eq!(id, "abc");
                assert!(password.is_none());
            }
            other => panic!("expected GetEntry, got {}", other.variant_name()),
        }
    }

    #[test]
    fn delete_targets_the_confirmed_id() {
        match delete_entry("doomed".to_string()) {
            AgentAction::DeleteEntry { id } => assert_eq!(id, "doomed"),
            other => panic!("expected DeleteEntry, got {}", other.variant_name()),
        }
    }

    #[test]
    fn pin_toggle_matches_the_optimistic_state() {
        assert!(matches!(
            toggle_pin("x".to_string(), true),
            AgentAction::PinEntry { .. }
        ));
        assert!(matches!(
            toggle_pin("x".to_string(), false),
            AgentAction::UnpinEntry { .. }
        ));
    }

    #[test]
    fn sidebar_listing_forwards_its_filters_and_never_a_domain() {
        match sidebar_entries(Some("mail".to_string()), Some(EntryType::Login), true) {
            AgentAction::GetSidebarEntries {
                query,
                entry_type,
                only_pinned,
                domain,
            } => {
                assert_eq!(query.as_deref(), Some("mail"));
                assert!(matches!(entry_type, Some(EntryType::Login)));
                assert!(only_pinned);
                assert!(domain.is_none(), "desktop listing must not domain-filter");
            }
            other => panic!("expected GetSidebarEntries, got {}", other.variant_name()),
        }
    }

    #[test]
    fn applet_search_does_not_filter_by_type() {
        match applet_search(Some("mail".to_string()), false) {
            AgentAction::GetSidebarEntries {
                entry_type,
                only_pinned,
                domain,
                ..
            } => {
                assert!(entry_type.is_none(), "the applet lists every entry kind");
                assert!(!only_pinned);
                assert!(domain.is_none());
            }
            other => panic!("expected GetSidebarEntries, got {}", other.variant_name()),
        }
    }

    #[test]
    fn password_fetch_carries_the_reprompt_answer_only_when_given() {
        assert!(matches!(
            fetch_password("id".to_string(), None),
            AgentAction::GetPassword { password: None, .. }
        ));
        match fetch_password("id".to_string(), Some("master".to_string())) {
            AgentAction::GetPassword { password, .. } => {
                assert_eq!(password.as_deref(), Some("master"))
            }
            other => panic!("expected GetPassword, got {}", other.variant_name()),
        }
    }

    #[test]
    fn on_demand_password_is_get_password() {
        match on_demand_secret("Password", "id".into(), None) {
            AgentAction::GetPassword { id, password } => {
                assert_eq!(id, "id");
                assert!(password.is_none());
            }
            other => panic!("expected GetPassword, got {}", other.variant_name()),
        }
    }

    #[test]
    fn on_demand_totp_is_get_totp() {
        assert!(matches!(
            on_demand_secret("TOTP", "id".into(), None),
            AgentAction::GetTotp { .. }
        ));
    }

    #[test]
    fn on_demand_iban_is_get_entry() {
        match on_demand_secret("IBAN", "id".into(), Some("master".into())) {
            AgentAction::GetEntry { id, password } => {
                assert_eq!(id, "id");
                assert_eq!(password.as_deref(), Some("master"));
            }
            other => panic!("expected GetEntry, got {}", other.variant_name()),
        }
    }

    #[test]
    fn edit_reprompt_resumes_get_entry() {
        match submit_reprompt_action(
            Some(&crate::message::RepromptIntent::Edit),
            "id".into(),
            "master".into(),
        ) {
            AgentAction::GetEntry { password, .. } => {
                assert_eq!(password.as_deref(), Some("master"));
            }
            other => panic!("expected GetEntry, got {}", other.variant_name()),
        }
    }

    #[test]
    fn reveal_reprompt_resumes_get_password() {
        match submit_reprompt_action(
            Some(&crate::message::RepromptIntent::Reveal {
                field: "Password".into(),
            }),
            "id".into(),
            "master".into(),
        ) {
            AgentAction::GetPassword { password, .. } => {
                assert_eq!(password.as_deref(), Some("master"));
            }
            other => panic!("expected GetPassword, got {}", other.variant_name()),
        }
    }
}
