use crate::app::CosmicBWardenApp;
use crate::message::Message;
use crate::message::View;
use cosmic::Application;
use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};

#[tokio::test]
async fn test_auth_window_transition() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Unlock;

    // Successful auth
    let _ = app.update(Message::AuthResult(Ok(())));

    assert_eq!(app.view, View::Vault);
}

#[tokio::test]
async fn test_reactive_events() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.selected_entry_id = Some("1".to_string());

    // 1. Receive Locked event
    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::Locked,
    ));
    assert_eq!(app.view, View::Unlock);
    assert!(app.selected_entry_id.is_none());

    // 2. Receive Unlocked event
    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::Unlocked,
    ));
    assert_eq!(app.view, View::Vault);
}

// Bug (d)/(f): EventReceived(Locked) must clear entries and any prior error
// immediately, regardless of whether the lock was triggered by the user or
// by a logout. Prior code preserved entries and error across the transition.
#[tokio::test]
async fn test_event_locked_clears_entries_and_error() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    let entry = SidebarEntry {
        id: "1".to_string(),
        name: "Some Entry".to_string(),
        username: None,
        public_key: None,
        entry_type: EntryType::Login,
        is_pinned: false,
    };
    app.all_entries = vec![entry.clone()];
    app.entries = vec![entry];
    app.error = Some("sync failed: no API session token".to_string());
    app.selected_entry_id = Some("1".to_string());

    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::Locked,
    ));

    assert_eq!(app.view, View::Unlock);
    assert!(app.entries.is_empty(), "EventReceived(Locked) must clear sidebar entries");
    assert!(app.all_entries.is_empty(), "EventReceived(Locked) must clear all_entries cache");
    assert!(app.error.is_none(), "EventReceived(Locked) must clear app.error");
    assert!(app.selected_entry_id.is_none());
}

#[tokio::test]
async fn test_unlock_requested_event_shows_unlock_view() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.selected_entry_id = Some("1".to_string());
    // Readiness: an account must be configured, otherwise unlocking is pointless.
    app.has_account = true;
    app.config.email = Some("user@example.com".to_string());

    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::UnlockRequested,
    ));
    assert_eq!(app.view, View::Unlock);
    assert!(app.selected_entry_id.is_none());
}

#[tokio::test]
async fn test_unlock_requested_ignored_without_account() {
    // No account/email configured — an unlock request must NOT force the unlock
    // prompt (it couldn't succeed). The view stays put; config is re-queried.
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    assert!(!app.unlock_prompt_ready());

    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::UnlockRequested,
    ));
    assert_eq!(app.view, View::Vault, "must not prompt unlock without an account");
    assert!(!app.show_pin_unlock);
}

#[tokio::test]
async fn test_pin_requested_ignored_without_account() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::PinRequested,
    ));
    assert!(!app.show_pin_unlock, "no PIN prompt without a configured account");
    assert_eq!(app.view, View::Vault);
}

#[tokio::test]
async fn test_open_entry_event_dispatches_select_entry_when_in_vault() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;

    // EventReceived(OpenEntry) returns Task::done(SelectEntry(id)).
    // In tests the runtime doesn't execute returned tasks, so we simulate the
    // two-step dispatch: OpenEntry → SelectEntry.
    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::OpenEntry { id: "entry-42".to_string() },
    ));
    // View must not have changed.
    assert_eq!(app.view, View::Vault);

    // Now simulate the runtime delivering the inner SelectEntry message.
    let _ = app.update(Message::SelectEntry("entry-42".to_string()));
    assert_eq!(app.selected_entry_id.as_deref(), Some("entry-42"));
}

#[tokio::test]
async fn test_open_entry_event_stores_pending_when_not_in_vault() {
    let mut app = CosmicBWardenApp::default();
    // View::Loading is the initial state before ConfigReceived
    assert_eq!(app.view, View::Loading);

    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::OpenEntry { id: "entry-42".to_string() },
    ));
    // Not in vault → stored for later, SelectEntry NOT dispatched yet
    assert!(app.selected_entry_id.is_none());
    assert_eq!(app.pending_vault_entry.as_deref(), Some("entry-42"));
}
