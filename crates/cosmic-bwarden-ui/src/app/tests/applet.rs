use crate::app::CosmicBWardenApp;
use crate::message::{Message, View, WindowState};
use cosmic::Application;
use cosmic::iced::window;
use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};

fn entry(id: &str, name: &str, entry_type: EntryType) -> SidebarEntry {
    SidebarEntry {
        id: id.to_string(),
        name: name.to_string(),
        username: Some("user".to_string()),
        public_key: Some("ssh-ed25519 AAAA...".to_string()),
        entry_type,
        is_pinned: false,
    }
}

#[tokio::test]
async fn test_applet_unlock_password_changed() {
    let mut app = CosmicBWardenApp::default();

    let _ = app.update(Message::AppletUnlockPasswordChanged("hunter2".to_string()));
    assert_eq!(app.applet_unlock_password, "hunter2");
}

#[tokio::test]
async fn test_applet_search_changed_updates_query_and_bumps_id() {
    let mut app = CosmicBWardenApp::default();
    assert_eq!(app.applet_search_id, 0);

    let _ = app.update(Message::AppletSearchChanged("foo".to_string()));
    assert_eq!(app.applet_search_query, "foo");
    assert_eq!(app.applet_search_id, 1);

    let _ = app.update(Message::AppletSearchChanged("bar".to_string()));
    assert_eq!(app.applet_search_query, "bar");
    assert_eq!(app.applet_search_id, 2);
}

#[tokio::test]
async fn test_applet_toggle_favourites_filter() {
    let mut app = CosmicBWardenApp::default();
    assert!(!app.applet_search_only_favourites);

    let _ = app.update(Message::AppletToggleFavouritesFilter);
    assert!(app.applet_search_only_favourites);

    let _ = app.update(Message::AppletToggleFavouritesFilter);
    assert!(!app.applet_search_only_favourites);
}

#[tokio::test]
async fn test_applet_reprompt_password_changed_and_cancelled() {
    let mut app = CosmicBWardenApp::default();
    app.applet_reprompt_id = Some("entry-1".to_string());

    let _ = app.update(Message::AppletRepromptPasswordChanged("hunter2".to_string()));
    assert_eq!(app.applet_reprompt_password, "hunter2");

    let _ = app.update(Message::AppletRepromptCancelled);
    assert!(app.applet_reprompt_id.is_none());
    assert!(app.applet_reprompt_password.is_empty());
}

#[tokio::test]
async fn test_applet_unlock_result_success_transitions_to_vault() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Unlock;
    app.applet_unlock_password = "hunter2".to_string();
    let prev_search_id = app.applet_search_id;

    let _ = app.update(Message::AppletUnlockResult(Ok(())));

    assert_eq!(app.view, View::Vault);
    assert!(app.applet_unlock_password.is_empty());
    assert!(app.applet_error.is_none());
    assert!(app.applet_search_id > prev_search_id);
}

#[tokio::test]
async fn test_applet_unlock_result_error_keeps_unlock_view() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Unlock;
    app.applet_unlock_password = "hunter2".to_string();

    let _ = app.update(Message::AppletUnlockResult(Err("invalid password".to_string())));

    assert_eq!(app.view, View::Unlock);
    assert!(app.applet_unlock_password.is_empty());
    assert_eq!(app.applet_error, Some("invalid password".to_string()));
}

#[tokio::test]
async fn test_applet_search_results_received_ignores_agent_locked_error() {
    let mut app = CosmicBWardenApp::default();

    let _ = app.update(Message::AppletSearchResultsReceived(app.applet_search_id, Err("agent is locked".to_string())));

    assert!(app.applet_error.is_none());
}

#[tokio::test]
async fn test_applet_toggle_unlock_and_reprompt_password_reveal() {
    let mut app = CosmicBWardenApp::default();
    assert!(!app.applet_unlock_password_revealed);
    assert!(!app.applet_reprompt_password_revealed);

    let _ = app.update(Message::AppletToggleUnlockPasswordReveal);
    assert!(app.applet_unlock_password_revealed);
    let _ = app.update(Message::AppletToggleUnlockPasswordReveal);
    assert!(!app.applet_unlock_password_revealed);

    let _ = app.update(Message::AppletToggleRepromptPasswordReveal);
    assert!(app.applet_reprompt_password_revealed);
    let _ = app.update(Message::AppletToggleRepromptPasswordReveal);
    assert!(!app.applet_reprompt_password_revealed);
}

#[tokio::test]
async fn test_applet_search_results_received_ignores_stale_id() {
    let mut app = CosmicBWardenApp::default();
    app.applet_search_id = 2;

    let stale = vec![entry("stale", "Stale Entry", EntryType::Login)];
    let _ = app.update(Message::AppletSearchResultsReceived(1, Ok(stale)));
    assert!(app.applet_search_results.is_empty());

    let current = vec![entry("current", "Current Entry", EntryType::Login)];
    let _ = app.update(Message::AppletSearchResultsReceived(2, Ok(current)));
    assert_eq!(app.applet_search_results.len(), 1);
    assert_eq!(app.applet_search_results[0].id, "current");
}

#[tokio::test]
async fn test_applet_secret_received_reprompt_required_vs_other_error() {
    let mut app = CosmicBWardenApp::default();

    let _ = app.update(Message::AppletSecretReceived(Err((
        "entry-1".to_string(),
        "reprompt_required".to_string(),
    ))));
    assert_eq!(app.applet_reprompt_id, Some("entry-1".to_string()));
    assert!(app.applet_error.is_none());

    app.applet_reprompt_id = None;
    let _ = app.update(Message::AppletSecretReceived(Err((
        "entry-1".to_string(),
        "some other error".to_string(),
    ))));
    assert!(app.applet_reprompt_id.is_none());
    assert_eq!(app.applet_error, Some("some other error".to_string()));
}

#[tokio::test]
async fn test_applet_popup_render_locked() {
    let mut app = CosmicBWardenApp::default();
    let id = window::Id::unique();
    app.windows.insert(id, WindowState::Popup);
    app.view = View::Unlock;

    let _ = app.view_window(id);
}

#[tokio::test]
async fn test_applet_popup_render_unlocked_empty_results() {
    let mut app = CosmicBWardenApp::default();
    let id = window::Id::unique();
    app.windows.insert(id, WindowState::Popup);
    app.view = View::Vault;
    app.applet_search_results = Vec::new();

    let _ = app.view_window(id);
}

#[tokio::test]
async fn test_applet_popup_render_unlocked_with_mixed_results() {
    let mut app = CosmicBWardenApp::default();
    let id = window::Id::unique();
    app.windows.insert(id, WindowState::Popup);
    app.view = View::Vault;
    app.applet_search_results = vec![
        entry("login-1", "Login Entry", EntryType::Login),
        entry("note-1", "Note Entry", EntryType::SecureNote),
        entry("ssh-1", "SSH Entry", EntryType::SshKey),
        entry("card-1", "Card Entry", EntryType::Card),
        entry("identity-1", "Identity Entry", EntryType::Identity),
    ];

    let _ = app.view_window(id);
}

#[tokio::test]
async fn test_applet_popup_render_with_reprompt_active() {
    let mut app = CosmicBWardenApp::default();
    let id = window::Id::unique();
    app.windows.insert(id, WindowState::Popup);
    app.view = View::Vault;
    app.applet_search_results = vec![entry("login-1", "Login Entry", EntryType::Login)];
    app.applet_reprompt_id = Some("login-1".to_string());

    let _ = app.view_window(id);
}

#[tokio::test]
async fn test_lock_and_quit_does_not_panic() {
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::LockAndQuit);
}

#[tokio::test]
async fn test_logout_and_quit_does_not_panic() {
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::LogoutAndQuit);
}
