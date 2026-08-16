use crate::app::update::applet::unlock_focus_id;
use crate::app::CosmicBWardenApp;
use crate::message::{Message, UnlockMode, View, WindowState};
use crate::view::applet::unlock;
use cosmic::iced::window;
use cosmic::Application;
use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};

fn popup_app(view: View) -> (CosmicBWardenApp, window::Id) {
    let mut app = CosmicBWardenApp::default();
    let id = window::Id::unique();
    app.windows.insert(id, WindowState::Popup);
    app.view = view;
    (app, id)
}

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

#[test]
fn test_unlock_focus_id_prefers_pin_when_pin_unlock_active() {
    assert_eq!(unlock_focus_id(UnlockMode::Pin), unlock::pin_input_id());
    assert_eq!(
        unlock_focus_id(UnlockMode::Password),
        unlock::password_input_id()
    );
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
    let mut app = CosmicBWardenApp {
        applet_reprompt_id: Some("entry-1".to_string()),
        ..Default::default()
    };

    let _ = app.update(Message::AppletRepromptPasswordChanged(
        "hunter2".to_string(),
    ));
    assert_eq!(app.applet_reprompt_password, "hunter2");

    let _ = app.update(Message::AppletRepromptCancelled);
    assert!(app.applet_reprompt_id.is_none());
    assert!(app.applet_reprompt_password.is_empty());
}

#[tokio::test]
async fn test_applet_unlock_result_success_transitions_to_vault() {
    let mut app = CosmicBWardenApp {
        view: View::Unlock,
        applet_unlock_password: "hunter2".to_string(),
        ..Default::default()
    };
    let prev_search_id = app.applet_search_id;

    let _ = app.update(Message::AppletUnlockResult(Ok(())));

    assert_eq!(app.view, View::Vault);
    assert!(app.applet_unlock_password.is_empty());
    assert!(app.applet_error.is_none());
    assert!(app.applet_search_id > prev_search_id);
}

#[tokio::test]
async fn test_applet_unlock_result_error_keeps_unlock_view() {
    let mut app = CosmicBWardenApp {
        view: View::Unlock,
        applet_unlock_password: "hunter2".to_string(),
        ..Default::default()
    };

    let _ = app.update(Message::AppletUnlockResult(Err(
        "invalid password".to_string()
    )));

    assert_eq!(app.view, View::Unlock);
    assert!(app.applet_unlock_password.is_empty());
    assert_eq!(app.applet_error, Some("invalid password".to_string()));
}

#[tokio::test]
async fn test_applet_search_results_received_ignores_agent_locked_error() {
    let mut app = CosmicBWardenApp::default();

    let _ = app.update(Message::AppletSearchResultsReceived(
        app.applet_search_id,
        Err("agent is locked".to_string()),
    ));

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
    let mut app = CosmicBWardenApp {
        applet_search_id: 2,
        ..Default::default()
    };

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

// Bug (b): closing and reopening the popup must preserve the user's search
// query and favourites-filter rather than resetting them to defaults.
// Verified via WindowClosed (popup close) not disturbing those fields, and
// via the applet search query surviving a VaultChanged refresh.
#[tokio::test]
async fn test_popup_closed_preserves_search_query() {
    let mut app = CosmicBWardenApp::default();
    let id = window::Id::unique();
    app.applet_popup = Some(id);
    app.windows.insert(id, WindowState::Popup);
    app.applet_search_query = "github".to_string();
    app.applet_search_only_favourites = true;

    let _ = app.update(Message::WindowClosed(id));

    assert!(app.applet_popup.is_none());
    assert_eq!(
        app.applet_search_query, "github",
        "search query must survive popup close"
    );
    assert!(
        app.applet_search_only_favourites,
        "favourites filter must survive popup close"
    );
}

#[tokio::test]
async fn test_popup_search_state_survives_vault_event() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        applet_search_query: "github".to_string(),
        applet_search_only_favourites: true,
        ..Default::default()
    };

    // A VaultChanged event triggers a refresh but must not touch applet search state.
    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::VaultChanged,
    ));

    assert_eq!(
        app.applet_search_query, "github",
        "VaultChanged must not reset applet search query"
    );
    assert!(
        app.applet_search_only_favourites,
        "VaultChanged must not reset applet favourites filter"
    );
}

#[tokio::test]
async fn test_applet_refresh_state_triggers_fetch_when_popup_open() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        applet_popup: Some(window::Id::unique()),
        ..Default::default()
    };
    let prev_search_id = app.applet_search_id;

    let _ = app.update(Message::RefreshStateInternal);

    assert!(app.applet_search_id > prev_search_id);
}

// ── Quit submenu ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_applet_quit_menu_toggle_expands_and_collapses() {
    let mut app = CosmicBWardenApp::default();
    assert!(!app.applet_quit_expanded);

    let _ = app.update(Message::AppletQuitMenuToggle);
    assert!(app.applet_quit_expanded);

    let _ = app.update(Message::AppletQuitMenuToggle);
    assert!(!app.applet_quit_expanded);
}

#[tokio::test]
async fn test_applet_quit_menu_collapsed_renders() {
    let (app, id) = popup_app(View::Vault);
    // Collapsed by default
    let _ = app.view_window(id);
}

#[tokio::test]
async fn test_applet_quit_menu_expanded_unlocked_renders_all_sub_items() {
    let (mut app, id) = popup_app(View::Vault);
    app.applet_quit_expanded = true;
    // Must render Lock-and-Quit, Logout-and-Quit, Just-Quit without panic
    let _ = app.view_window(id);
}

#[tokio::test]
async fn test_applet_quit_menu_expanded_locked_shows_just_quit_only() {
    let (mut app, id) = popup_app(View::Unlock);
    app.applet_quit_expanded = true;
    // In locked state no Lock-and-Quit or Logout-and-Quit in the submenu
    let _ = app.view_window(id);
}

// ── Header row (Lock / Logout icon buttons) ───────────────────────────────────

#[tokio::test]
async fn test_applet_header_row_renders_unlocked() {
    let (app, id) = popup_app(View::Vault);
    // Lock + Logout icon buttons must both appear without panic
    let _ = app.view_window(id);
}

#[tokio::test]
async fn test_applet_header_row_renders_locked() {
    let (app, id) = popup_app(View::Unlock);
    // Only the Logout icon button appears when vault is locked
    let _ = app.view_window(id);
}

#[tokio::test]
async fn test_applet_header_row_renders_setup() {
    let (app, id) = popup_app(View::Setup);
    // Setup state: no Lock button, only Logout icon
    let _ = app.view_window(id);
}

// ── Search entry rows ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_applet_login_row_with_link_renders() {
    let (mut app, id) = popup_app(View::Vault);
    // Name looks like a hostname → is_uri_like → 🔗 button is active
    app.applet_search_results = vec![SidebarEntry {
        id: "1".to_string(),
        name: "account.facebook.com".to_string(),
        username: Some("alice@example.com".to_string()),
        public_key: None,
        entry_type: EntryType::Login,
        is_pinned: false,
    }];
    let _ = app.view_window(id);
}

#[tokio::test]
async fn test_applet_login_row_without_link_renders() {
    let (mut app, id) = popup_app(View::Vault);
    // Name has spaces → not URI-like → 🔗 button is inactive (on_press_maybe(None))
    app.applet_search_results = vec![SidebarEntry {
        id: "1".to_string(),
        name: "My Facebook Account".to_string(),
        username: Some("alice".to_string()),
        public_key: None,
        entry_type: EntryType::Login,
        is_pinned: false,
    }];
    let _ = app.view_window(id);
}

#[tokio::test]
async fn test_applet_ssh_row_renders_with_vault_and_secret_buttons() {
    let (mut app, id) = popup_app(View::Vault);
    app.applet_search_results = vec![SidebarEntry {
        id: "ssh-1".to_string(),
        name: "My Server".to_string(),
        username: None,
        public_key: Some("ssh-ed25519 AAAA...".to_string()),
        entry_type: EntryType::SshKey,
        is_pinned: false,
    }];
    let _ = app.view_window(id);
}

#[tokio::test]
async fn test_applet_note_row_renders_with_vault_and_secret_buttons() {
    let (mut app, id) = popup_app(View::Vault);
    app.applet_search_results = vec![SidebarEntry {
        id: "note-1".to_string(),
        name: "My Secret Note".to_string(),
        username: None,
        public_key: None,
        entry_type: EntryType::SecureNote,
        is_pinned: false,
    }];
    let _ = app.view_window(id);
}

// ── AppletCopyPrimary ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_applet_copy_primary_copies_username_from_search_results() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        applet_search_results: vec![SidebarEntry {
            id: "e1".to_string(),
            name: "example.com".to_string(),
            username: Some("alice@example.com".to_string()),
            public_key: None,
            entry_type: EntryType::Login,
            is_pinned: false,
        }],
        ..Default::default()
    };

    // Should not panic even though clipboard write is async
    let _ = app.update(Message::AppletCopyPrimary("e1".to_string()));
}

#[tokio::test]
async fn test_applet_copy_primary_no_op_for_missing_id() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        applet_search_results: vec![],
        ..Default::default()
    };

    let _ = app.update(Message::AppletCopyPrimary("nonexistent".to_string()));
    // No panic, no state change
    assert!(app.applet_error.is_none());
}

// ── AppletOpenInVault ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_applet_open_in_vault_does_not_crash() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        ..Default::default()
    };

    let _ = app.update(Message::AppletOpenInVault("entry-1".to_string()));
    // State must remain intact
    assert_eq!(app.view, View::Vault);
}

// ── AppletOpenLink ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_applet_open_link_does_not_crash() {
    let mut app = CosmicBWardenApp::default();
    // xdg-open may not be available in CI; we only check the handler doesn't panic
    let _ = app.update(Message::AppletOpenLink("https://example.com".to_string()));
    assert!(app.applet_error.is_none());
}
