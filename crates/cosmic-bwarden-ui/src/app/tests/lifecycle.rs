use crate::message::WindowState;
use crate::app::CosmicBWardenApp;
use crate::message::Message;
use crate::message::View;
use cosmic::Application;
use cosmic::iced::window;
use cosmic::widget;
use cosmic_bwarden_core::config::CosmicBWardenConfig;
use cosmic_bwarden_core::db::{Entry, EntryData};
use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};

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
        favorite: false,
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

    app.windows.insert(id_popup, WindowState::Popup);

    // Verify Popup view doesn't crash
    let _ = app.view_window(id_popup);
}

#[tokio::test]
async fn test_popup_lifecycle() {
    let mut app = CosmicBWardenApp::default();
    let id = window::Id::unique();

    // Open popup
    app.applet_popup = Some(id);
    app.windows.insert(id, WindowState::Popup);

    // Close popup
    let _ = app.update(Message::WindowClosed(id));
    assert!(app.applet_popup.is_none());
    assert!(app.windows.get(&id).is_none());
}

#[tokio::test]
async fn test_lock_logout_clears_state() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.entries = vec![SidebarEntry {
        id: "1".to_string(),
        name: "Entry 1".to_string(),
        username: None,
        public_key: None,
        entry_type: EntryType::Login,
        is_pinned: false,
    }];
    app.selected_entry_id = Some("1".to_string());
    app.revealed_fields
        .insert(("1".to_string(), "Password".to_string()));

    // 1. Test Lock
    let _ = app.update(Message::LockResult);
    assert_eq!(app.view, View::Unlock);
    assert!(app.entries.is_empty());
    assert!(app.selected_entry_id.is_none());
    assert!(app.revealed_fields.is_empty());

    // 2. Setup for Logout
    app.view = View::Vault;
    app.entries = vec![SidebarEntry {
        id: "1".to_string(),
        name: "Entry 1".to_string(),
        username: None,
        public_key: None,
        entry_type: EntryType::Login,
        is_pinned: false,
    }];

    // 3. Test Logout
    let _ = app.update(Message::LogoutResult);
    assert_eq!(app.view, View::Setup);
    assert!(app.entries.is_empty());
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
async fn test_config_received_routes_by_has_account_not_needs_login() {
    let mut app = CosmicBWardenApp::default();
    let config = CosmicBWardenConfig::default();

    // After an agent restart with persist_session disabled, access/refresh tokens
    // are lost (they're #[serde(skip)]), so needs_login stays true even once the
    // vault is unlocked. has_account (protected_key on disk) is what should drive
    // routing, not needs_login.
    let _ = app.update(Message::ConfigReceived(Ok((config.clone(), true, true, false))));
    assert_eq!(app.view, View::Vault);

    // Locked but with an account on disk -> Unlock, not Setup.
    let _ = app.update(Message::ConfigReceived(Ok((config.clone(), true, true, true))));
    assert_eq!(app.view, View::Unlock);

    // No account on disk at all -> Setup, regardless of is_locked.
    let _ = app.update(Message::ConfigReceived(Ok((config, true, false, false))));
    assert_eq!(app.view, View::Setup);
}

#[test]
fn test_settings_view_keeps_sidebar_entries() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.entries = vec![SidebarEntry {
        id: "1".to_string(),
        name: "Entry 1".to_string(),
        username: None,
        public_key: None,
        entry_type: EntryType::Login,
        is_pinned: false,
    }];

    let _ = app.update(Message::SettingsViewClicked);
    assert_eq!(app.view, View::Settings);
    assert!(!app.entries.is_empty());

    // Settings is rendered as the right panel alongside the sidebar.
    let _ = app.view_window(window::Id::RESERVED);

    let _ = app.update(Message::VaultViewClicked);
    assert_eq!(app.view, View::Vault);
}

#[tokio::test]
async fn test_applet_messages() {
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
