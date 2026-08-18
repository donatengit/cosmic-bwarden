//! Regression tests for the Settings save path.
//!
//! Incident (2026-08): saving Settings wrote the whole in-memory config to
//! disk. That struct is `Default` until the agent answers `GetConfig`, so one
//! save with a not-yet-loaded config replaced a live account's `email`,
//! `base_url`, `device_id` and TPM flags with `null`. The agent kept working
//! from its in-memory vault, so nothing looked wrong until the next write
//! failed with "email not set in config" — and a restart would have lost the
//! account entirely.
//!
//! The rules these tests pin down:
//!   1. Never write the config before it has been loaded.
//!   2. Only ever write the two fields this pane owns; everything else is the
//!      agent's and must survive untouched.
//!   3. A failed save keeps the user's edit buffer.

use super::config_env::{account_config, ConfigFile};
use crate::app::state::CosmicBWardenApp;
use crate::message::Message;

/// The incident itself: a save issued before `GetConfig` answered must not
/// touch the file. Against the pre-fix code this wrote an all-`null` config.
#[test]
fn save_before_config_loads_does_not_touch_the_file() {
    let on_disk = account_config();
    let file = ConfigFile::with(&on_disk);

    let mut app = CosmicBWardenApp::default();
    assert!(!app.config_loaded, "a fresh app has not loaded any config");
    // The user opens Settings and changes the lock timeout to 20 minutes.
    let _ = app.update_app(Message::SettingsEditClicked);
    let _ = app.update_app(Message::SettingsLockTimeoutChanged(20));
    let _ = app.update_app(Message::SettingsSaveClicked);

    assert_eq!(
        file.read(),
        on_disk,
        "settings save before load must leave the account config untouched"
    );
    assert!(
        app.error.is_some(),
        "the refusal must be surfaced, not silent"
    );
    assert!(
        app.editing_config.is_some(),
        "a refused save must keep the user's edit buffer"
    );
}

/// After a real load, only the two owned fields move; the agent's fields are
/// read back from disk rather than written from the UI's stale copy.
#[test]
fn save_after_load_writes_only_owned_fields() {
    let on_disk = account_config();
    let file = ConfigFile::with(&on_disk);

    let mut app = CosmicBWardenApp::default();
    // The agent answers GetConfig — but with TPM still off, mirroring a UI
    // whose copy went stale when the agent later enabled TPM on disk.
    let mut stale = on_disk.clone();
    stale.tpm_enabled = false;
    stale.tpm_store_server_credentials = false;
    let _ = app.update_app(Message::ConfigReceived(Ok((
        stale, false, true, false, false, 0, 0,
    ))));
    assert!(app.config_loaded);

    let _ = app.update_app(Message::SettingsEditClicked);
    let _ = app.update_app(Message::SettingsLockTimeoutChanged(20));
    let _ = app.update_app(Message::SettingsSaveClicked);

    let written = file.read();
    assert_eq!(written.lock_timeout, 1200, "the owned field is persisted");
    assert_eq!(written.email, on_disk.email, "email must survive");
    assert_eq!(written.base_url, on_disk.base_url, "base_url must survive");
    assert_eq!(
        written.device_id, on_disk.device_id,
        "device_id must survive"
    );
    assert!(
        written.tpm_enabled && written.tpm_store_server_credentials,
        "TPM flags are the agent's; a stale UI copy must not clear them"
    );
    assert!(written.persist_session, "persist_session must survive");
    assert!(app.error.is_none(), "a successful save reports no error");
    assert!(
        app.editing_config.is_none(),
        "a successful save consumes the edit buffer"
    );
}

/// The server field is genuinely owned by this pane, so it must still change.
#[test]
fn save_after_load_persists_the_server_url() {
    let on_disk = account_config();
    let file = ConfigFile::with(&on_disk);

    let mut app = CosmicBWardenApp::default();
    let _ = app.update_app(Message::ConfigReceived(Ok((
        on_disk.clone(),
        false,
        true,
        false,
        false,
        0,
        0,
    ))));
    let _ = app.update_app(Message::SettingsEditClicked);
    let _ = app.update_app(Message::SettingsServerChanged(
        "https://vault.example.com".to_string(),
    ));
    let _ = app.update_app(Message::SettingsSaveClicked);

    let written = file.read();
    assert_eq!(
        written.base_url.as_deref(),
        Some("https://vault.example.com")
    );
    assert_eq!(written.email, on_disk.email, "email still survives");
}
