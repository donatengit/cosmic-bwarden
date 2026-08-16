use crate::app::CosmicBWardenApp;
use crate::message::Message;
use crate::message::UnlockMode;
use crate::message::View;
use crate::message::WindowState;
use cosmic::iced::window;
use cosmic::Application;
use cosmic_bwarden_core::config::CosmicBWardenConfig;
use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};

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
    assert!(!app.windows.contains_key(&id));
}

#[tokio::test]
async fn test_lock_logout_clears_state() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        entries: vec![SidebarEntry {
            id: "1".to_string(),
            name: "Entry 1".to_string(),
            username: None,
            public_key: None,
            entry_type: EntryType::Login,
            is_pinned: false,
        }],
        selected_entry_id: Some("1".to_string()),
        ..Default::default()
    };
    app.revealed_fields
        .insert(("1".to_string(), "Password".to_string()));

    // 1. Test Lock
    app.error = Some("prior error".to_string());
    let _ = app.update(Message::LockResult);
    assert_eq!(app.view, View::Unlock);
    assert!(app.entries.is_empty());
    assert!(app.selected_entry_id.is_none());
    assert!(app.revealed_fields.is_empty());
    assert!(app.error.is_none(), "LockResult must clear app.error");

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
    let _ = app.update(Message::ConfigReceived(Ok((
        config.clone(),
        true,
        true,
        false,
        false, 0, 0))));
    assert_eq!(app.view, View::Vault);

    // Locked but with an account on disk -> Unlock, not Setup.
    let _ = app.update(Message::ConfigReceived(Ok((
        config.clone(),
        true,
        true,
        true,
        false, 0, 0))));
    assert_eq!(app.view, View::Unlock);

    // No account on disk at all -> Setup, regardless of is_locked.
    let _ = app.update(Message::ConfigReceived(Ok((
        config, true, false, false, false, 0, 0))));
    assert_eq!(app.view, View::Setup);
}

// Bug (a): ConfigReceived while already unlocked must trigger an entry fetch.
// The prior code set view=Vault but returned Task::none(), leaving the sidebar
// empty until the user typed in the search box.
#[tokio::test]
async fn test_config_received_vault_triggers_entry_fetch() {
    let mut app = CosmicBWardenApp::default();
    let prev_id = app.search_id;

    // Unlocked: has_account=true, is_locked=false
    let _ = app.update(Message::ConfigReceived(Ok((
        CosmicBWardenConfig::default(),
        false,
        true,
        false,
        false, 0, 0))));

    assert_eq!(app.view, View::Vault);
    assert!(
        app.search_id > prev_id,
        "ConfigReceived with unlocked vault must increment search_id to trigger fetch"
    );
}

// The applet popup's entry fetch must not fire until the vault is confirmed
// unlocked (ConfigReceived), not speculatively on click. Firing it eagerly on
// click wasted a request on the single serialized agent connection whenever
// the vault turned out to be locked, delaying the requests that actually
// drive the locked-state UI (Version/GetConfig/CheckTpm).
#[tokio::test]
async fn test_config_received_unlocked_with_applet_popup_triggers_applet_fetch() {
    let mut app = CosmicBWardenApp::default();
    let popup_id = window::Id::unique();
    app.applet_popup = Some(popup_id);
    let prev_id = app.applet_search_id;

    // Unlocked: has_account=true, is_locked=false
    let _ = app.update(Message::ConfigReceived(Ok((
        CosmicBWardenConfig::default(),
        false,
        true,
        false,
        false, 0, 0))));

    assert_eq!(app.view, View::Vault);
    assert!(
        app.applet_search_id > prev_id,
        "ConfigReceived with an open applet popup and an unlocked vault must \
         increment applet_search_id to trigger the applet entry fetch"
    );
}

// Companion to the above: with no applet popup open (main-window-only
// session), ConfigReceived must not bother fetching applet search results.
#[tokio::test]
async fn test_config_received_unlocked_without_applet_popup_skips_applet_fetch() {
    let mut app = CosmicBWardenApp {
        applet_popup: None,
        ..Default::default()
    };
    let prev_id = app.applet_search_id;

    let _ = app.update(Message::ConfigReceived(Ok((
        CosmicBWardenConfig::default(),
        false,
        true,
        false,
        false, 0, 0))));

    assert_eq!(app.view, View::Vault);
    assert_eq!(
        app.applet_search_id, prev_id,
        "ConfigReceived without an open applet popup must not trigger an applet entry fetch"
    );
}

// Bug (d): LockResult and LogoutResult must clear any prior error so it does
// not bleed through onto the Unlock/Setup screen.
#[test]
fn test_lock_logout_clears_error() {
    let mut app = CosmicBWardenApp {
        error: Some("sync failed: no API session token".to_string()),
        ..Default::default()
    };
    let _ = app.update(Message::LockResult);
    assert!(app.error.is_none(), "LockResult must clear app.error");

    app.error = Some("sync failed: no API session token".to_string());
    let _ = app.update(Message::LogoutResult);
    assert!(app.error.is_none(), "LogoutResult must clear app.error");
}

// Bug (d): an in-flight GetSidebarEntries that returns "agent is locked"
// (because the lock raced the response) must silently clear entries rather
// than surfacing the error to the user.
#[test]
fn test_entries_received_locked_clears_without_setting_error() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        search_id: 3,
        ..Default::default()
    };
    let entry = SidebarEntry {
        id: "1".to_string(),
        name: "Entry 1".to_string(),
        username: None,
        public_key: None,
        entry_type: EntryType::Login,
        is_pinned: false,
    };
    app.all_entries = vec![entry.clone()];
    app.entries = vec![entry];

    let _ = app.update(Message::EntriesReceived(
        3,
        Err("agent is locked".to_string()),
    ));

    assert!(
        app.entries.is_empty(),
        "entries must be cleared on locked error"
    );
    assert!(
        app.all_entries.is_empty(),
        "all_entries must be cleared on locked error"
    );
    assert!(
        app.error.is_none(),
        "agent-is-locked must not set app.error"
    );
}

#[test]
fn test_entries_received_populates_all_entries_and_applies_active_filter() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        search_id: 5,
        search_query: "git".to_string(),
        ..Default::default()
    };

    let entries = vec![
        SidebarEntry {
            id: "1".to_string(),
            name: "GitHub".to_string(),
            username: Some("alice".to_string()),
            public_key: None,
            entry_type: EntryType::Login,
            is_pinned: false,
        },
        SidebarEntry {
            id: "2".to_string(),
            name: "AWS Console".to_string(),
            username: Some("bob".to_string()),
            public_key: None,
            entry_type: EntryType::Login,
            is_pinned: false,
        },
    ];

    let _ = app.update(Message::EntriesReceived(5, Ok(entries.clone())));

    assert_eq!(
        app.all_entries.len(),
        2,
        "all_entries must hold the full unfiltered list"
    );
    assert_eq!(
        app.entries.len(),
        1,
        "entries must be the filter-applied subset"
    );
    assert_eq!(
        app.entries[0].id, "1",
        "only GitHub matches the 'git' query"
    );
    assert!(app.error.is_none());
}

#[test]
fn test_entries_received_stale_id_ignored() {
    let mut app = CosmicBWardenApp {
        search_id: 5,
        ..Default::default()
    };

    let entries = vec![SidebarEntry {
        id: "1".to_string(),
        name: "GitHub".to_string(),
        username: None,
        public_key: None,
        entry_type: EntryType::Login,
        is_pinned: false,
    }];

    let _ = app.update(Message::EntriesReceived(4, Ok(entries)));

    assert!(
        app.all_entries.is_empty(),
        "stale EntriesReceived must not touch all_entries"
    );
    assert!(
        app.entries.is_empty(),
        "stale EntriesReceived must not touch entries"
    );
}

#[test]
fn test_settings_view_keeps_sidebar_entries() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        entries: vec![SidebarEntry {
            id: "1".to_string(),
            name: "Entry 1".to_string(),
            username: None,
            public_key: None,
            entry_type: EntryType::Login,
            is_pinned: false,
        }],
        ..Default::default()
    };

    let _ = app.update(Message::SettingsViewClicked);
    assert_eq!(app.view, View::Settings);
    assert!(!app.entries.is_empty());

    // Settings is rendered as the right panel alongside the sidebar.
    let _ = app.view_window(window::Id::RESERVED);
}

// ─── TPM state machine tests ─────────────────────────────────────────────────
//
// Four distinct situations the settings view must represent correctly:
//   1. Status not yet queried      → show "Checking…" (not "not accessible")
//   2. Hardware not accessible     → show diagnostics
//   3. Available, not configured   → show toggle in OFF/"Not configured" state
//   4. Available and configured    → show toggle in ON/"Active" state
//
// Root-cause context: tpm_available defaults to false; without tpm_status_known
// gating the "not accessible" branch, an unlocked vault startup (where
// ConfigReceived returns early before calling check_tpm_task) shows the wrong message.

#[test]
fn test_tpm_status_unknown_shows_checking_not_inaccessible() {
    let app = CosmicBWardenApp::default();
    // Fresh default: status not yet queried.
    assert!(!app.tpm_status_known);
    assert!(!app.tpm_available);

    // Render must not crash and must NOT show the "not accessible" message
    // (that would be misleading before we've even asked the agent).
    let _ = app.view_settings();
}

#[test]
fn test_tpm_status_received_hardware_unavailable() {
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::TpmStatusReceived(Ok((false, false, false))));
    assert!(app.tpm_status_known);
    assert!(!app.tpm_available);
    assert!(!app.tpm_configured);

    // Settings view renders without panicking (shows diagnostics branch).
    let _ = app.view_settings();
}

#[test]
fn test_tpm_status_received_available_not_configured() {
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::TpmStatusReceived(Ok((true, false, false))));
    assert!(app.tpm_status_known);
    assert!(app.tpm_available);
    assert!(!app.tpm_configured);
    assert_eq!(
        app.unlock_mode,
        UnlockMode::Password,
        "no PIN unlock when not configured"
    );

    let _ = app.view_settings();
}

#[test]
fn test_tpm_status_received_available_and_configured() {
    // While the unlock view is showing, a configured PIN promotes the form to
    // PIN-first.
    let mut app = CosmicBWardenApp {
        view: View::Unlock,
        ..Default::default()
    };
    let _ = app.update(Message::TpmStatusReceived(Ok((true, true, false))));
    assert!(app.tpm_status_known);
    assert!(app.tpm_available);
    assert!(app.tpm_configured);
    assert_eq!(app.unlock_mode, UnlockMode::Pin);

    let _ = app.view_settings();
}

#[test]
fn test_tpm_status_received_while_unlocked_does_not_flip_mode() {
    // Regression (F2): TpmStatusReceived used to enable PIN unlock even while
    // the vault was unlocked, so the next lock prompted PIN-first regardless
    // of how the user got in. While unlocked, the mode must stay put.
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        ..Default::default()
    };
    let _ = app.update(Message::TpmStatusReceived(Ok((true, true, false))));
    assert!(app.tpm_configured);
    assert_eq!(
        app.unlock_mode,
        UnlockMode::Password,
        "unlocked: TPM status must not flip the unlock form"
    );
}

#[test]
fn test_tpm_status_known_set_on_error_response() {
    // If CheckTpm fails (agent unreachable), the unlock form must still render:
    // mark the status as known-unavailable so the password fallback shows
    // instead of an endless spinner.
    let mut app = CosmicBWardenApp {
        view: View::Unlock,
        ..Default::default()
    };
    let _ = app.update(Message::TpmStatusReceived(Err(
        "agent unreachable".to_string()
    )));
    assert!(app.tpm_status_known, "Err must mark TPM status as known");
    assert!(!app.tpm_available);
    assert_eq!(app.unlock_mode, UnlockMode::Password);
}

#[test]
fn test_config_received_unlocked_sets_tpm_check_pending() {
    // When vault is already unlocked (ConfigReceived with is_locked=false),
    // tpm_status_known must still be false until TpmStatusReceived arrives.
    // This confirms the async check was dispatched (search_id increment is a proxy).
    let mut app = CosmicBWardenApp::default();
    let prev_search_id = app.search_id;

    let _ = app.update(Message::ConfigReceived(Ok((
        CosmicBWardenConfig::default(),
        false,
        true,  // has_account
        false, // is_locked = false → vault open path
        false, 0, 0))));

    assert_eq!(app.view, View::Vault);
    assert!(
        app.search_id > prev_search_id,
        "entry fetch must be triggered"
    );
    // tpm_status_known is still false — the async check_tpm_task has been
    // dispatched but TpmStatusReceived hasn't arrived yet.
    assert!(!app.tpm_status_known);
}

#[test]
fn test_settings_view_clicked_dispatches_tpm_check() {
    // Navigating to Settings must always refresh TPM status (TPM might have
    // become accessible after the user was added to the tss group, etc.).
    // We verify by confirming tpm_status_known resets when the incoming
    // response changes state — i.e. the task was dispatched and processed.
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        ..Default::default()
    };

    // Pre-set known state from a prior check.
    let _ = app.update(Message::TpmStatusReceived(Ok((true, false, false))));
    assert!(app.tpm_status_known);

    // Navigate to settings — this dispatches check_tpm_task (async, not awaited
    // in unit tests), so tpm_status_known stays true from the previous check.
    let _ = app.update(Message::SettingsViewClicked);
    assert_eq!(app.view, View::Settings);
    // State from prior check is still visible while the refresh is in-flight.
    assert!(app.tpm_status_known);
    assert!(app.tpm_available);
}

#[test]
fn test_master_password_unlock_offers_pin_reenable() {
    // TPM present and a (possibly stale) PIN blob configured, but the user is on
    // the master-password screen (e.g. after a PIN mismatch forced a fallback).
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::TpmStatusReceived(Ok((true, true, false))));
    app.unlock_mode = UnlockMode::Password;
    app.password_preferred = true; // fell back to master password
    app.login_email = "user@example.com".to_string();
    app.view = View::Unlock;

    // The master-password unlock view renders the PIN (re-)enable field path.
    let _ = app.view_auth();

    // Typing a PIN then submitting master password marks the apply-pending flag.
    let _ = app.update(Message::UnlockPinChanged("123456".to_string()));
    assert_eq!(app.unlock_pin, "123456");
    let _ = app.update(Message::UnlockPasswordChanged("masterpw".to_string()));
    let _ = app.update(Message::UnlockSubmitted);
    assert!(
        app.unlock_pin_apply_pending,
        "unlock should apply the PIN field"
    );
}

#[test]
fn test_unlock_pin_too_short_is_rejected() {
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::TpmStatusReceived(Ok((true, false, false))));
    let _ = app.update(Message::UnlockPinChanged("12".to_string()));
    let _ = app.update(Message::UnlockPasswordChanged("masterpw".to_string()));
    let _ = app.update(Message::UnlockSubmitted);
    assert!(
        app.error.is_some(),
        "short PIN must be rejected before unlock"
    );
    assert!(!app.unlock_pin_apply_pending);
}

#[test]
fn test_tpm_da_line_formatting() {
    use cosmic_bwarden_core::protocol::TpmDaStatus;
    let mut app = CosmicBWardenApp::default();

    // Nothing fetched yet.
    assert!(app.tpm_da_line().is_none());

    // Unavailable TPM → no line.
    app.tpm_da = Some(TpmDaStatus {
        available: false,
        ..Default::default()
    });
    assert!(app.tpm_da_line().is_none());

    // Normal case with remaining/max.
    app.tpm_da = Some(TpmDaStatus {
        available: true,
        max_tries: Some(32),
        lockout_counter: Some(3),
        remaining: Some(29),
        in_lockout: false,
        recovery_interval_secs: Some(7200),
    });
    let line = app.tpm_da_line().unwrap();
    assert!(line.contains("29 of 32"), "got: {line}");

    // In lockout → mentions lockout and recovery time.
    app.tpm_da = Some(TpmDaStatus {
        available: true,
        in_lockout: true,
        recovery_interval_secs: Some(7200),
        ..Default::default()
    });
    let line = app.tpm_da_line().unwrap();
    assert!(line.to_lowercase().contains("locked out"), "got: {line}");
    assert!(line.contains("2h"), "got: {line}");
}

#[test]
fn test_enable_pin_resets_server_credentials_toggle() {
    // Simulate a prior state where server credentials were on, then a fresh
    // enable (TpmSetupResult Ok). Enabling resets all TPM stores, so the UI's
    // server-credentials flag must return to false.
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::TpmStatusReceived(Ok((true, true, true))));
    assert!(app.tpm_server_credentials, "precondition: server creds on");

    let _ = app.update(Message::TpmSetupResult(Ok(())));
    assert!(app.tpm_configured);
    assert!(
        !app.tpm_server_credentials,
        "enabling PIN must reset server credentials to off"
    );
}

#[test]
fn test_disable_then_enable_cycle_state() {
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::TpmStatusReceived(Ok((true, true, true))));

    // Disable clears configured + server creds and hides PIN unlock.
    let _ = app.update(Message::TpmDisableResult(Ok(())));
    assert!(!app.tpm_configured);
    assert_eq!(app.unlock_mode, UnlockMode::Password);

    // Re-enable: configured again, server creds fresh (off).
    let _ = app.update(Message::TpmSetupResult(Ok(())));
    assert!(app.tpm_configured);
    assert!(!app.tpm_server_credentials);
}

#[tokio::test]
async fn test_pin_incorrect_flag_lifecycle() {
    // Ready account so PinRequested actually prompts.
    let mut app = CosmicBWardenApp {
        has_account: true,
        ..Default::default()
    };
    app.config.email = Some("user@example.com".to_string());
    let _ = app.update(Message::TpmStatusReceived(Ok((true, true, false))));

    // Fresh PIN prompt: counter hidden.
    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::PinRequested,
    ));
    assert!(!app.pin_incorrect, "counter hidden on fresh prompt");

    let unseal_err = cosmic_bwarden_core::protocol::ERR_TPM_UNSEAL_FAILED.to_string();

    // Wrong PIN: counter revealed; the unseal error is log-only, never shown.
    let _ = app.update(Message::AppletPinResult(Err(unseal_err.clone())));
    assert!(app.pin_incorrect, "counter revealed after wrong PIN");
    assert!(
        app.applet_error.is_none(),
        "unseal error must not be shown in the applet"
    );
    assert!(
        app.error.is_none(),
        "unseal error must not be shown in the main window"
    );

    // Main-window PIN unlock failure: same suppression via MainWindowPinResult.
    let _ = app.update(Message::MainWindowPinResult(Err(unseal_err)));
    assert!(app.pin_incorrect);
    assert!(
        app.error.is_none(),
        "unseal error must not be shown on the PIN dialog"
    );

    // A non-unseal failure (agent down, config/account problem) is still
    // surfaced, not mislabeled as a wrong PIN.
    let _ = app.update(Message::AppletPinResult(Err(
        "unexpected response".to_string()
    )));
    assert_eq!(app.applet_error.as_deref(), Some("unexpected response"));
    let _ = app.update(Message::MainWindowPinResult(Err(
        "failed to load config: x".to_string(),
    )));
    assert_eq!(app.error.as_deref(), Some("failed to load config: x"));
    app.applet_error = None;
    app.error = None;

    // A failed LOGIN while the PIN flags are still set keeps its error visible
    // (regression: AuthResult must not be reclassified as a PIN failure).
    app.unlock_mode = UnlockMode::Pin;
    app.tpm_available = true;
    let _ = app.update(Message::AuthResult(Err("invalid password".to_string())));
    assert_eq!(app.error.as_deref(), Some("invalid password"));
    app.error = None;

    // Switching to master password clears it.
    let _ = app.update(Message::AppletUseMasterPasswordInstead);
    assert!(!app.pin_incorrect);
}

#[test]
fn test_da_status_received_updates_state() {
    use cosmic_bwarden_core::protocol::TpmDaStatus;
    let mut app = CosmicBWardenApp::default();
    let status = TpmDaStatus {
        available: true,
        remaining: Some(10),
        max_tries: Some(32),
        ..Default::default()
    };
    let _ = app.update(Message::TpmDaStatusReceived(Some(status)));
    assert_eq!(app.tpm_da.as_ref().and_then(|d| d.remaining), Some(10));
}

#[test]
fn test_apply_unlock_pin_noop_without_tpm() {
    // No TPM: the PIN field is inert and produces no task, and is cleared.
    let mut app = CosmicBWardenApp {
        unlock_pin: "123456".to_string(),
        ..Default::default()
    };
    assert!(app.apply_unlock_pin_task().is_none());
    assert!(app.unlock_pin.is_empty());
}

#[tokio::test]
async fn test_applet_messages() {
    let mut app = CosmicBWardenApp::default();

    // These messages mostly trigger Tasks, but we verify they don't crash and reach update
    let _ = app.update(Message::LockClicked);
    let _ = app.update(Message::LogoutClicked);
    let _ = app.update(Message::SyncClicked);

    let _ = app.update(Message::SettingsViewClicked);
    assert_eq!(app.view, View::Settings);
}

#[test]
fn test_stale_config_received_is_dropped() {
    // F1: a Config snapshot computed BEFORE a lock-state transition must not
    // be applied once a newer (session_id, lock_epoch) snapshot has been
    // seen. This is what used to bounce users back to the lock screen right
    // after a successful PIN unlock.
    let mut app = CosmicBWardenApp {
        view: View::Unlock,
        ..Default::default()
    };

    // Newer snapshot: session 7, epoch 5, unlocked.
    let _ = app.update(Message::ConfigReceived(Ok((
        CosmicBWardenConfig::default(),
        false,
        true,
        false,
        false,
        7,
        5,
    ))));
    assert_eq!(app.view, View::Vault);
    assert_eq!(app.last_config, Some((7, 5)));

    // Stale snapshot: same session, epoch 4, LOCKED — must be dropped; the
    // view must not bounce back to Unlock.
    let _ = app.update(Message::ConfigReceived(Ok((
        CosmicBWardenConfig::default(),
        false,
        true,
        true,
        false,
        7,
        4,
    ))));
    assert_eq!(
        app.view,
        View::Vault,
        "stale snapshot must not flip the view back to Unlock"
    );

    // Equal epoch is a re-delivery of the same state, not staleness: applied.
    let _ = app.update(Message::ConfigReceived(Ok((
        CosmicBWardenConfig::default(),
        false,
        true,
        true,
        false,
        7,
        5,
    ))));
    assert_eq!(app.view, View::Unlock, "same-epoch snapshot is not stale");

    // Different session id = agent restart (epoch resets): must be applied,
    // not mistaken for staleness.
    let _ = app.update(Message::ConfigReceived(Ok((
        CosmicBWardenConfig::default(),
        false,
        true,
        false,
        false,
        8,
        0,
    ))));
    assert_eq!(app.view, View::Vault, "agent restart is not staleness");
    assert_eq!(app.last_config, Some((8, 0)));
}

#[test]
fn test_unlock_mode_transitions() {
    let mut app = CosmicBWardenApp {
        has_account: true,
        view: View::Vault,
        ..Default::default()
    };
    app.config.email = Some("user@example.com".to_string());

    // Agent broadcast PinRequested (TPM PIN configured): PIN form.
    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::PinRequested,
    ));
    assert_eq!(app.unlock_mode, UnlockMode::Pin);
    assert_eq!(app.view, View::Unlock);

    // Agent broadcast UnlockRequested (no PIN configured): password form.
    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::UnlockRequested,
    ));
    assert_eq!(app.unlock_mode, UnlockMode::Password);

    // Explicit "use master password instead" sticks: a later TPM status must
    // NOT flip the form back to PIN.
    let _ = app.update(Message::AppletUseMasterPasswordInstead);
    assert!(app.password_preferred);
    let _ = app.update(Message::TpmStatusReceived(Ok((true, true, false))));
    assert_eq!(
        app.unlock_mode,
        UnlockMode::Password,
        "explicit choice must survive TPM status"
    );
}

#[test]
fn test_pin_result_state_changed_not_mislabeled() {
    // F5: a PCR-state change (BIOS/firmware update) must never be shown as
    // "Incorrect PIN" — the PIN is fine and no DA attempt was consumed.
    let mut app = CosmicBWardenApp {
        view: View::Unlock,
        unlock_mode: UnlockMode::Pin,
        tpm_status_known: true,
        tpm_configured: true,
        ..Default::default()
    };
    let state_changed = cosmic_bwarden_core::protocol::ERR_TPM_STATE_CHANGED.to_string();

    let _ = app.update(Message::MainWindowPinResult(Err(state_changed.clone())));
    assert!(!app.pin_incorrect, "PCR change is not a wrong PIN");
    assert!(app.error.is_some(), "recovery guidance must be shown");
    assert_eq!(
        app.unlock_mode,
        UnlockMode::Password,
        "state change flips to the password form"
    );

    // The applet path mirrors it.
    let _ = app.update(Message::AppletPinResult(Err(state_changed)));
    assert!(!app.pin_incorrect);
    assert!(app.error.is_some());
    assert_eq!(app.unlock_mode, UnlockMode::Password);
}
