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

use cosmic_bwarden_core::protocol::{Action as AgentAction, EntryType};

/// Fetch an entry's full contents. `reprompt_password` is `Some` only when the
/// user is answering a master-password reprompt; a plain selection sends
/// `None` and lets the agent decide whether a reprompt is required.
pub fn fetch_entry(id: String, reprompt_password: Option<String>) -> AgentAction {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_selection_sends_no_reprompt_password() {
        match fetch_entry("abc".to_string(), None) {
            AgentAction::GetEntry { id, password } => {
                assert_eq!(id, "abc");
                assert!(password.is_none());
            }
            other => panic!("expected GetEntry, got {}", other.variant_name()),
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
}
