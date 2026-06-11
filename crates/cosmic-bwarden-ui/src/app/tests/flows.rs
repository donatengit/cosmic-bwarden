use crate::message::WindowState;
use crate::app::CosmicBWardenApp;
use crate::message::Message;
use crate::message::View;
use cosmic::Application;
use cosmic::widget;
use cosmic_bwarden_core::config::CosmicBWardenConfig;
use cosmic_bwarden_core::db::{Entry, EntryData};
use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};

#[tokio::test]
async fn test_e2e_user_flow_login_and_add_note() {
    let mut app = CosmicBWardenApp::default();

    // 1. Initial State -> Loading
    assert_eq!(app.view, View::Loading);

    // 2. Receive config (needs login)
    let config = CosmicBWardenConfig::default();
    let _ = app.update(Message::ConfigReceived(Ok((config, true, false, true))));
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
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.config.lock_timeout = 600; // 10 mins

    // 1. Open Settings
    let _ = app.update(Message::SettingsViewClicked);
    assert_eq!(app.view, View::Settings);

    // 2. Click Edit
    let _ = app.update(Message::SettingsEditClicked);
    assert!(app.editing_config.is_some());
    assert_eq!(app.settings_lock_timeout, "10");

    // 3. Change Lock Timeout
    let _ = app.update(Message::SettingsLockTimeoutChanged("20".to_string()));
    assert_eq!(app.settings_lock_timeout, "20");

    // 4. Change Popular Count
    let _ = app.update(Message::SettingsPopularCountChanged("15".to_string()));
    assert_eq!(app.settings_popular_count, "15");

    // 5. Save
    let _ = app.update(Message::SettingsSaveClicked);
    assert_eq!(app.config.lock_timeout, 1200); // 20 * 60
    assert_eq!(app.config.top_popular_count, 15);
    assert!(app.editing_config.is_none());

    // 6. Test Cancel
    let _ = app.update(Message::SettingsEditClicked);
    let _ = app.update(Message::SettingsLockTimeoutChanged("30".to_string()));
    let _ = app.update(Message::SettingsCancelClicked);
    assert_eq!(app.config.lock_timeout, 1200); // Remained 20 mins
    assert!(app.editing_config.is_none());
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
    ))));
    assert_eq!(app.view, View::Unlock);

    app.config.email = None;
    let _ = app.update(Message::AuthResult(Err("Failed again".to_string())));
    assert_eq!(app.view, View::Setup); // Back to setup if no email
}
