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

// ── Quit footer (single Exit / Quit row) ─────────────────────────────────────

#[tokio::test]
async fn test_applet_quit_menu_unlocked_renders() {
    let (app, id) = popup_app(View::Vault);
    let _ = app.view_window(id);
}

#[tokio::test]
async fn test_applet_quit_menu_locked_renders() {
    let (app, id) = popup_app(View::Unlock);
    let _ = app.view_window(id);
}

#[tokio::test]
async fn test_applet_protocol_mismatch_renders_quit() {
    let (mut app, id) = popup_app(View::Unlock);
    app.protocol_mismatch = true;
    let _ = app.view_window(id);
}

// ── Search-row hover (action icons only while hovered) ───────────────────────

#[tokio::test]
async fn test_applet_search_row_hover_sets_and_clears_id() {
    let mut app = CosmicBWardenApp::default();
    assert_eq!(app.applet_hovered_row_id, None);

    let _ = app.update(Message::AppletSearchRowHoverChanged("e1".into(), true));
    assert_eq!(app.applet_hovered_row_id.as_deref(), Some("e1"));

    let _ = app.update(Message::AppletSearchRowHoverChanged("e1".into(), false));
    assert_eq!(app.applet_hovered_row_id, None);
}

#[tokio::test]
async fn test_applet_search_row_hover_exit_of_other_id_does_not_clear() {
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::AppletSearchRowHoverChanged("e1".into(), true));
    let _ = app.update(Message::AppletSearchRowHoverChanged("e2".into(), true));
    let _ = app.update(Message::AppletSearchRowHoverChanged("e1".into(), false));
    assert_eq!(app.applet_hovered_row_id.as_deref(), Some("e2"));
}

#[tokio::test]
async fn test_applet_hovered_search_row_renders() {
    let (mut app, id) = popup_app(View::Vault);
    app.applet_search_results = vec![SidebarEntry {
        id: "1".to_string(),
        name: "account.facebook.com".to_string(),
        username: Some("alice@example.com".to_string()),
        public_key: None,
        entry_type: EntryType::Login,
        is_pinned: false,
    }];
    app.applet_hovered_row_id = Some("1".to_string());
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
    // Name looks like a hostname → is_uri_like → open-URI button is active
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
    // Name has spaces → not URI-like → open-URI button is inactive (on_press_maybe(None))
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

// ── Popup reopen debounce ─────────────────────────────────────────────────────

#[test]
fn test_should_suppress_popup_reopen_within_debounce_window() {
    use crate::app::update::applet::{
        should_suppress_popup_reopen, APPLET_POPUP_REOPEN_DEBOUNCE_MS,
    };

    let now = std::time::Instant::now();

    // No prior close → never suppress.
    assert!(!should_suppress_popup_reopen(None, now));

    // Closed 150 ms ago (within the 200 ms window) → suppress.
    let within = now - std::time::Duration::from_millis(150);
    assert!(should_suppress_popup_reopen(Some(within), now));

    // Closed 250 ms ago (past the window) → reopen allowed.
    let past = now - std::time::Duration::from_millis(250);
    assert!(!should_suppress_popup_reopen(Some(past), now));

    // Exactly at the boundary (200 ms) → not strictly less than 200 → reopen.
    assert_eq!(APPLET_POPUP_REOPEN_DEBOUNCE_MS, 200);
    let at_boundary =
        now - std::time::Duration::from_millis(APPLET_POPUP_REOPEN_DEBOUNCE_MS as u64);
    assert!(!should_suppress_popup_reopen(Some(at_boundary), now));
}

#[tokio::test]
async fn test_icon_click_close_then_same_press_reopen_is_suppressed() {
    let mut app = CosmicBWardenApp::default();
    let id = window::Id::unique();
    app.applet_popup = Some(id);
    app.windows.insert(id, WindowState::Popup);
    let offset = cosmic::iced::Vector::default();
    let bounds = cosmic::iced::Rectangle::default();

    // First click: popup is open → close path. Records the close timestamp.
    let _ = app.update(Message::AppletIconClicked(offset, bounds));
    assert!(
        app.applet_popup.is_none(),
        "first click must close the popup"
    );
    assert!(app.applet_last_popup_closed_at.is_some());

    // Second click in the same press: must NOT reopen (no popup task, state stays closed).
    let _ = app.update(Message::AppletIconClicked(offset, bounds));
    assert!(
        app.applet_popup.is_none(),
        "same-press reopen must be suppressed"
    );
    assert!(
        app.applet_last_popup_closed_at.is_none(),
        "the suppression timestamp is consumed so the next deliberate click opens"
    );
}

#[tokio::test]
async fn test_icon_click_opens_after_debounce_window_elapses() {
    use std::time::Duration;

    let mut app = CosmicBWardenApp::default();
    let offset = cosmic::iced::Vector::default();
    let bounds = cosmic::iced::Rectangle::default();

    // Simulate a close that happened longer than the debounce window ago.
    app.applet_last_popup_closed_at = Some(std::time::Instant::now() - Duration::from_millis(500));

    // A click after the window elapses opens normally (produces an open task;
    // no panic, popup id is assigned by the popup-creation closure we don't run here).
    let _ = app.update(Message::AppletIconClicked(offset, bounds));
    // The reopen is not suppressed, so the close-timestamp is cleared.
    assert!(app.applet_last_popup_closed_at.is_none());
}

#[test]
fn test_clamped_anchor_rect_never_emits_zero_or_negative_dimensions() {
    use crate::app::update::applet::clamped_anchor_rect;

    // Normal case: offset subtracted, no clamping applies.
    let normal = clamped_anchor_rect(
        cosmic::iced::Vector { x: 5.0, y: 5.0 },
        cosmic::iced::Rectangle {
            x: 100.0,
            y: 100.0,
            width: 32.0,
            height: 32.0,
        },
    );
    assert_eq!(normal.x, 95);
    assert_eq!(normal.y, 95);
    assert_eq!(normal.width, 32);
    assert_eq!(normal.height, 32);

    // Degenerate / over-large offset: every side clamps to >= 1px.
    let degenerate = clamped_anchor_rect(
        cosmic::iced::Vector { x: 500.0, y: 500.0 },
        cosmic::iced::Rectangle {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        },
    );
    assert!(degenerate.x >= 1, "anchor x must be >= 1");
    assert!(degenerate.y >= 1, "anchor y must be >= 1");
    assert!(degenerate.width >= 1, "anchor width must be >= 1");
    assert!(degenerate.height >= 1, "anchor height must be >= 1");
}

// ── Popup size limits ─────────────────────────────────────────────────────────

#[test]
fn applet_popup_limits_keep_the_360px_floor_and_cap_at_372() {
    let limits = crate::view::applet::applet_popup_limits();
    let min = limits.min();
    let max = limits.max();
    // The 360 floor matches the libcosmic default so Fill-based popup content
    // keeps its current width; the 372 ceiling gives wide content headroom.
    assert_eq!(min.width, 360.0);
    assert_eq!(max.width, 372.0);
    assert_eq!(min.height, 200.0);
    assert_eq!(max.height, 1080.0);
}

// ── Activation-token routing ──────────────────────────────────────────────────

#[test]
fn test_token_activation_routes_activate_lock_without_panicking() {
    let mut app = CosmicBWardenApp::default();
    let task = app.update(Message::Token(
        cosmic::applet::token::subscription::TokenUpdate::ActivationToken {
            token: None,
            exec: "activate:lock".to_string(),
        },
    ));
    let _ = task;
    // The lock round-trip resolves asynchronously via LockResult (needs a
    // live agent), so only the routing itself is asserted: no panic, and the
    // applet session state is untouched.
    assert!(app.token_tx.is_none());
    assert_eq!(app.view, View::Loading);
}

#[test]
fn test_token_activation_ignores_unknown_execs() {
    let mut app = CosmicBWardenApp::default();
    let task = app.update(Message::Token(
        cosmic::applet::token::subscription::TokenUpdate::ActivationToken {
            token: None,
            exec: "activate:bogus".to_string(),
        },
    ));
    let _ = task;
    assert_eq!(app.view, View::Loading);
}
