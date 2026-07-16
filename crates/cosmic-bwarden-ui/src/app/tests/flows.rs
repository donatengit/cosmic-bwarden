use crate::app::CosmicBWardenApp;
use crate::message::Message;
use crate::message::View;
use cosmic::widget;
use cosmic::Application;
use cosmic_bwarden_core::config::CosmicBWardenConfig;
use cosmic_bwarden_core::db::{Entry, EntryData};
use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};

fn sidebar(id: &str, name: &str) -> SidebarEntry {
    SidebarEntry {
        id: id.to_string(),
        name: name.to_string(),
        username: None,
        public_key: None,
        entry_type: EntryType::Login,
        is_pinned: false,
    }
}

fn full_entry(id: &str, name: &str) -> Entry {
    Entry {
        id: id.to_string(),
        org_id: None,
        folder: None,
        folder_id: None,
        name: name.to_string(),
        data: EntryData::Login {
            username: None,
            password: None,
            totp: None,
            uris: vec![],
        },
        fields: vec![],
        notes: None,
        history: vec![],
        key: None,
        master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
        favorite: false,
    }
}

#[test]
fn test_toggle_pin_flips_is_pinned() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        ..Default::default()
    };
    let id = "entry-1".to_string();

    app.entries = vec![SidebarEntry {
        id: id.clone(),
        name: "Test Entry".to_string(),
        username: None,
        public_key: None,
        entry_type: EntryType::Login,
        is_pinned: false,
    }];

    // Pin it
    let _ = app.update(Message::TogglePin(id.clone()));
    assert!(
        app.entries[0].is_pinned,
        "entry should be pinned after TogglePin"
    );

    // Unpin it
    let _ = app.update(Message::TogglePin(id.clone()));
    assert!(
        !app.entries[0].is_pinned,
        "entry should be unpinned after second TogglePin"
    );
}

#[test]
fn test_toggle_search_pinned_filter() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        ..Default::default()
    };
    let initial_search_id = app.search_id;
    app.all_entries = vec![
        SidebarEntry {
            id: "1".to_string(),
            name: "Pinned Entry".to_string(),
            username: None,
            public_key: None,
            entry_type: EntryType::Login,
            is_pinned: true,
        },
        SidebarEntry {
            id: "2".to_string(),
            name: "Regular Entry".to_string(),
            username: None,
            public_key: None,
            entry_type: EntryType::Login,
            is_pinned: false,
        },
    ];
    app.entries = app.all_entries.clone();

    assert!(!app.search_only_pinned);

    let _ = app.update(Message::ToggleSearchPinned);
    assert!(app.search_only_pinned);
    assert_eq!(
        app.search_id, initial_search_id,
        "ToggleSearchPinned must NOT trigger IPC fetch"
    );
    assert_eq!(app.entries.len(), 1);
    assert_eq!(app.entries[0].id, "1");

    let _ = app.update(Message::ToggleSearchPinned);
    assert!(!app.search_only_pinned);
    assert_eq!(
        app.entries.len(),
        2,
        "unpinning filter must restore all entries"
    );
}

#[tokio::test]
async fn test_e2e_user_flow_login_and_add_note() {
    let mut app = CosmicBWardenApp::default();

    // 1. Initial State -> Loading
    assert_eq!(app.view, View::Loading);

    // 2. Receive config (needs login)
    let config = CosmicBWardenConfig::default();
    let _ = app.update(Message::ConfigReceived(Ok((
        config, true, false, true, false,
    ))));
    assert_eq!(app.view, View::Setup);

    // 3. User enters credentials
    let _ = app.update(Message::EmailChanged("test@example.com".to_string()));
    let _ = app.update(Message::PasswordChanged("password".to_string()));
    assert_eq!(app.login_email, "test@example.com");

    // 4. Submit Login
    let _ = app.update(Message::LoginSubmitted);

    // 5. Simulate successful auth
    let _ = app.update(Message::AuthResult(Ok(())));
    assert_eq!(app.view, View::Vault);

    // 6. User clicks "Add New Entry"
    let _ = app.update(Message::AddEntryRequested);
    assert!(app.editing_entry.is_some());

    // 7. Change to Note type and set content
    let _ = app.update(Message::NewEntryTypeChanged(EntryType::SecureNote));
    if let Some(entry) = &app.editing_entry {
        assert!(matches!(entry.data, EntryData::SecureNote));
    }

    for c in "Note content".chars() {
        let _ = app.update(Message::NotesAction(widget::text_editor::Action::Edit(
            widget::text_editor::Edit::Insert(c),
        )));
    }

    // 8. Save
    let _ = app.update(Message::SaveEdit);
    let _ = app.update(Message::SaveEditResult(Ok(())));
    assert!(app.editing_entry.is_none());
}

#[test]
fn test_settings_flow() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        ..Default::default()
    };
    app.config.lock_timeout = 600; // 10 mins

    // 1. Open Settings
    let _ = app.update(Message::SettingsViewClicked);
    assert_eq!(app.view, View::Settings);

    // 2. Click Edit
    let _ = app.update(Message::SettingsEditClicked);
    assert!(app.editing_config.is_some());
    assert_eq!(app.editing_config.as_ref().unwrap().lock_timeout, 600);

    // 3. Change Lock Timeout via slider (20 minutes)
    let _ = app.update(Message::SettingsLockTimeoutChanged(20));
    assert_eq!(app.editing_config.as_ref().unwrap().lock_timeout, 1200);

    // 4. Save
    let _ = app.update(Message::SettingsSaveClicked);
    assert_eq!(app.config.lock_timeout, 1200); // 20 * 60
    assert!(app.editing_config.is_none());

    // 6. Test Cancel
    let _ = app.update(Message::SettingsEditClicked);
    let _ = app.update(Message::SettingsLockTimeoutChanged(30));
    let _ = app.update(Message::SettingsCancelClicked);
    assert_eq!(app.config.lock_timeout, 1200); // Remained 20 mins
    assert!(app.editing_config.is_none());
}

// Bug: pressing "Not synced" (SyncClicked → SyncResult(Ok(()))) used to leave
// sync_failed=true because only ConfigReceived cleared it. The fix adds an
// immediate clear in the Ok arm so the button resets without waiting for the
// VaultChanged → RefreshStateInternal → GetConfig async round-trip.
#[test]
fn test_sync_result_ok_clears_sync_failed_immediately() {
    let mut app = CosmicBWardenApp {
        sync_failed: true,
        error: Some("delete failed: connection refused".to_string()),
        ..Default::default()
    };

    let _ = app.update(Message::SyncResult(Ok(())));

    assert!(
        !app.sync_failed,
        "SyncResult(Ok) must clear sync_failed immediately"
    );
    assert!(
        app.error.is_none(),
        "SyncResult(Ok) must clear the error banner"
    );
}

#[test]
fn test_sync_result_err_sets_sync_failed() {
    let mut app = CosmicBWardenApp::default();
    assert!(!app.sync_failed);

    let _ = app.update(Message::SyncResult(Err("connection refused".to_string())));

    assert!(app.sync_failed);
    assert_eq!(app.error.as_deref(), Some("connection refused"));
}

// After delete succeeds on the server but the follow-up sync fails, the
// entry stays in the stale local cache. When the user presses "Not synced"
// and the retry sync succeeds, EntriesReceived delivers a list without the
// deleted entry. The detail panel must auto-clear so the user doesn't see a
// ghost entry.
#[test]
fn test_entries_received_clears_stale_selected_entry() {
    let mut app = CosmicBWardenApp::default();
    app.all_entries = vec![sidebar("1", "Keep"), sidebar("del", "ToDelete")];
    app.entries = app.all_entries.clone();
    app.selected_entry_id = Some("del".to_string());
    app.selected_entry = Some(full_entry("del", "ToDelete"));
    app.editing_entry = None;
    let search_id = app.search_id;

    // Simulate EntriesReceived after sync that no longer includes "del"
    let _ = app.update(Message::EntriesReceived(
        search_id,
        Ok(vec![sidebar("1", "Keep")]),
    ));

    assert_eq!(app.all_entries.len(), 1);
    assert!(
        app.selected_entry_id.is_none(),
        "stale selection must be cleared"
    );
    assert!(
        app.selected_entry.is_none(),
        "stale detail panel must be cleared"
    );
}

#[test]
fn test_entries_received_preserves_valid_selection() {
    let mut app = CosmicBWardenApp::default();
    app.all_entries = vec![sidebar("1", "Keep"), sidebar("2", "Also Keep")];
    app.entries = app.all_entries.clone();
    app.selected_entry_id = Some("1".to_string());
    app.selected_entry = Some(full_entry("1", "Keep"));
    let search_id = app.search_id;

    let _ = app.update(Message::EntriesReceived(
        search_id,
        Ok(vec![sidebar("1", "Keep"), sidebar("2", "Also Keep")]),
    ));

    // Selection that still exists must NOT be cleared
    assert_eq!(app.selected_entry_id.as_deref(), Some("1"));
    assert!(app.selected_entry.is_some());
}

#[test]
fn test_remember_and_auth_failure() {
    let mut app = CosmicBWardenApp::default();

    // Test Remember Email toggle
    assert!(app.login_remember);
    let _ = app.update(Message::RememberChanged(false));
    assert!(!app.login_remember);

    // Test Auth Failure transitions
    app.view = View::Loading;
    app.config.email = Some("test@example.com".to_string());
    let _ = app.update(Message::AuthResult(Err("Failed".to_string())));
    assert_eq!(app.view, View::Unlock); // Back to unlock if we have an email
    assert_eq!(app.error, Some("Failed".to_string()));

    // Simulate Config update showing it's still locked
    let _ = app.update(Message::ConfigReceived(Ok((
        app.config.clone(),
        false,
        true,
        true,
        false,
    ))));
    assert_eq!(app.view, View::Unlock);

    app.config.email = None;
    let _ = app.update(Message::AuthResult(Err("Failed again".to_string())));
    assert_eq!(app.view, View::Setup); // Back to setup if no email
}
