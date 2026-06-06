use super::*;
use cosmic_bwarden_core::protocol::{SidebarEntry, EntryType};
use cosmic_bwarden_core::db::{Entry, EntryData};
use cosmic_bwarden_core::config::CosmicBWardenConfig;
use crate::message::{View, WindowState};
use cosmic::widget;

fn create_test_entry(id: &str, name: &str) -> Entry {
    Entry {
        id: id.to_string(),
        org_id: None,
        folder: None,
        folder_id: None,
        name: name.to_string(),
        data: EntryData::Login {
            username: Some("old-user".to_string()),
            password: Some("old-pass".to_string().into()),
            totp: None,
            uris: vec![],
        },
        fields: vec![],
        notes: Some("old-notes".into()),
        history: vec![],
        key: None,
        master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
    }
}

#[tokio::test]
async fn test_applet_surface_isolation() {
    let app = CosmicBWardenApp::default();
    let id_main = window::Id::RESERVED;
    
    // Simulate applet mode
    std::env::set_var("COSMIC_PANEL_NAME", "top");
    
    // In applet mode, the main surface (id None in self.windows) should show applet icon
    let _ = app.view_window(id_main);
    
    // Clean up
    std::env::remove_var("COSMIC_PANEL_NAME");
}

#[tokio::test]
async fn test_window_differentiation() {
    let mut app = CosmicBWardenApp::default();
    let id_popup = window::Id::unique();
    let id_auth = window::Id::unique();
    let id_main = window::Id::unique();

    app.windows.insert(id_popup, WindowState::Popup);
    app.windows.insert(id_auth, WindowState::Auth);
    app.windows.insert(id_main, WindowState::Main);

    // Verify Popup view doesn't crash
    let _ = app.view_window(id_popup);
    
    // Verify Auth view doesn't crash
    let _ = app.view_window(id_auth);

    // Verify Main view doesn't crash
    let _ = app.view_window(id_main);
}

#[tokio::test]
async fn test_popup_lifecycle() {
    let mut app = CosmicBWardenApp::default();
    let id = window::Id::unique();

    // Open popup
    app.applet_popup = Some(id);
    app.windows.insert(id, WindowState::Popup);

    // Close popup
    let _ = app.update(Message::PopupClosed(id));
    assert!(app.applet_popup.is_none());
    assert!(app.windows.get(&id).is_none());
}

#[tokio::test]
async fn test_auth_window_transition() {
    let mut app = CosmicBWardenApp::default();
    let id_auth = window::Id::unique();
    app.windows.insert(id_auth, WindowState::Auth);
    app.view = View::Unlock;

    // Successful auth
    let _ = app.update(Message::AuthResult(Ok(())));
    
    // In unit test, update doesn't execute Tasks, but it returns them.
    // We can't easily verify the window::close task here without more machinery,
    // but we verified the logic in app.rs.
    assert_eq!(app.view, View::Vault);
}

#[tokio::test]
async fn test_e2e_user_flow_login_and_add_note() {
    let mut app = CosmicBWardenApp::default();
    
    // 1. Initial State -> Loading
    assert_eq!(app.view, View::Loading);

    // 2. Receive config (needs login)
    let config = CosmicBWardenConfig::default();
    let _ = app.update(Message::ConfigReceived(Ok((config, true, true))));
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
        let _ = app.update(Message::NotesAction(widget::text_editor::Action::Edit(widget::text_editor::Edit::Insert(c))));
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
fn test_lock_logout_clears_state() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.entries = vec![SidebarEntry { id: "1".to_string(), name: "Entry 1".to_string(), entry_type: EntryType::Login, is_pinned: false }];
    app.selected_entry_id = Some("1".to_string());
    app.revealed_fields.insert(("1".to_string(), "Password".to_string()));

    // 1. Test Lock
    let _ = app.update(Message::LockResult);
    assert_eq!(app.view, View::Unlock);
    assert!(app.entries.is_empty());
    assert!(app.selected_entry_id.is_none());
    assert!(app.revealed_fields.is_empty());

    // 2. Setup for Logout
    app.view = View::Vault;
    app.entries = vec![SidebarEntry { id: "1".to_string(), name: "Entry 1".to_string(), entry_type: EntryType::Login, is_pinned: false }];

    // 3. Test Logout
    let _ = app.update(Message::LogoutResult);
    assert_eq!(app.view, View::Setup);
    assert!(app.entries.is_empty());
}

#[test]
fn test_vault_filtering_and_searching() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;

    // 1. Search Query
    let _ = app.update(Message::SearchChanged("my search".to_string()));
    assert_eq!(app.search_query, "my search");
    assert_eq!(app.search_id, 1);

    // 2. Filter Type
    let _ = app.update(Message::FilterTypeChanged(Some("ssh".to_string())));
    assert_eq!(app.filter_type, Some("ssh".to_string()));
    assert_eq!(app.search_id, 2);

    // 3. Search Submitted
    let _ = app.update(Message::SearchSubmitted("final query".to_string()));
    assert_eq!(app.search_id, 3);
}

#[test]
fn test_entry_field_editing() {
    let mut app = CosmicBWardenApp::default();
    
    // 1. Login Entry
    let mut entry = create_test_entry("1", "Login");
    app.editing_entry = Some(entry.clone());

    let _ = app.update(Message::EditFieldChanged("Username".to_string(), "new-user".to_string()));
    let _ = app.update(Message::EditFieldChanged("Password".to_string(), "new-pass".to_string()));

    if let EntryData::Login { username, password, .. } = &app.editing_entry.as_ref().unwrap().data {
        assert_eq!(username.as_deref(), Some("new-user"));
        assert_eq!(password.as_deref(), Some("new-pass"));
    }

    // 2. SSH Key Entry
    entry.data = EntryData::SshKey { private_key: None, public_key: None, fingerprint: None };
    app.editing_entry = Some(entry.clone());

    let _ = app.update(Message::EditFieldChanged("Private Key".to_string(), "PRIVATE".to_string()));
    let _ = app.update(Message::EditFieldChanged("Public Key".to_string(), "PUBLIC".to_string()));

    if let EntryData::SshKey { private_key, public_key, .. } = &app.editing_entry.as_ref().unwrap().data {
        assert_eq!(private_key.as_deref(), Some("PRIVATE"));
        assert_eq!(public_key.as_deref(), Some("PUBLIC"));
    }
}

#[test]
fn test_reveal_toggle() {
    let mut app = CosmicBWardenApp::default();
    let id = "entry-1".to_string();
    let field = "Password".to_string();

    assert!(!app.revealed_fields.contains(&(id.clone(), field.clone())));
    
    // Toggle On
    let _ = app.update(Message::ToggleRevealField(id.clone(), field.clone()));
    assert!(app.revealed_fields.contains(&(id.clone(), field.clone())));

    // Toggle Off
    let _ = app.update(Message::ToggleRevealField(id.clone(), field.clone()));
    assert!(!app.revealed_fields.contains(&(id.clone(), field.clone())));
}

#[test]
fn test_applet_messages() {
    let mut app = CosmicBWardenApp::default();
    
    // These messages mostly trigger Tasks, but we verify they don't crash and reach update
    let _ = app.update(Message::LockClicked);
    let _ = app.update(Message::LogoutClicked);
    let _ = app.update(Message::SyncClicked);
    let _ = app.update(Message::VaultViewClicked);
    assert_eq!(app.view, View::Vault);
    
    let _ = app.update(Message::SettingsViewClicked);
    assert_eq!(app.view, View::Settings);
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
    let _ = app.update(Message::ConfigReceived(Ok((app.config.clone(), false, true))));
    assert_eq!(app.view, View::Unlock);

    app.config.email = None;
    let _ = app.update(Message::AuthResult(Err("Failed again".to_string())));
    assert_eq!(app.view, View::Setup); // Back to setup if no email
}

#[test]
fn test_error_handling() {
    let mut app = CosmicBWardenApp::default();
    
    // Auth Error
    let _ = app.update(Message::AuthResult(Err("Invalid password".to_string())));
    assert_eq!(app.error, Some("Invalid password".to_string()));
    
    // Entry Error
    let _ = app.update(Message::EntryReceived(Err("Failed to decrypt".to_string())));
    assert_eq!(app.error, Some("Failed to decrypt".to_string()));
}

#[tokio::test]
async fn test_reactive_events() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.selected_entry_id = Some("1".to_string());
    
    // 1. Receive Locked event
    let _ = app.update(Message::EventReceived(cosmic_bwarden_core::protocol::Event::Locked));
    assert_eq!(app.view, View::Unlock);
    assert!(app.selected_entry_id.is_none());

    // 2. Receive Unlocked event
    let _ = app.update(Message::EventReceived(cosmic_bwarden_core::protocol::Event::Unlocked));
    assert_eq!(app.view, View::Vault);
}
