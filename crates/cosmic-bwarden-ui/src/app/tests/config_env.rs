//! Test-only helper for anything that persists the app config.
//!
//! **Rule: no test may call a path that reaches `save_legacy()` without going
//! through `ConfigFile`.** `dirs::config_file()` falls back to the real user
//! config (`~/.config/cosmic-bwarden/config.json`) when `COSMIC_BWARDEN_CONFIG`
//! is unset, so a unit test that saves settings overwrites the developer's own
//! account. That is not hypothetical: on 2026-08-10 a plain
//! `cargo test -p cosmic-bwarden-ui` replaced a live config with an all-`null`
//! default, and the running agent kept serving the vault from memory until the
//! next write failed with "email not set in config".

use cosmic_bwarden_core::config::CosmicBWardenConfig;
use std::sync::{Mutex, MutexGuard};

// COSMIC_BWARDEN_CONFIG is process-global and tests run in parallel threads;
// the lock keeps overlapping tests from seeing each other's config path.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// A populated on-disk config standing in for a real, logged-in account.
pub fn account_config() -> CosmicBWardenConfig {
    CosmicBWardenConfig {
        email: Some("user@example.com".to_string()),
        // Empty string on purpose: that is what leaving the server field blank
        // produces, and `server_name()` keys the vault cache and the TPM blob
        // on it, so it must round-trip exactly.
        base_url: Some(String::new()),
        device_id: Some("device-uuid".to_string()),
        lock_timeout: 5400,
        persist_session: true,
        tpm_enabled: true,
        tpm_store_server_credentials: true,
        ..CosmicBWardenConfig::default()
    }
}

/// Redirects `COSMIC_BWARDEN_CONFIG` at a temporary file for the duration of a
/// test, seeded with `config`. Restores the environment on drop.
pub struct ConfigFile {
    _guard: MutexGuard<'static, ()>,
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
}

impl ConfigFile {
    pub fn with(config: &CosmicBWardenConfig) -> Self {
        // A poisoned lock only means some other test panicked; the environment
        // is still ours to set, and failing here would mask that test's error.
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        std::env::set_var("COSMIC_BWARDEN_CONFIG", &path);
        config.save_legacy().expect("seed config");
        Self {
            _guard: guard,
            _dir: dir,
            path,
        }
    }

    /// The config as it currently exists on disk.
    pub fn read(&self) -> CosmicBWardenConfig {
        let json = std::fs::read_to_string(&self.path).expect("read config");
        serde_json::from_str(&json).expect("parse config")
    }
}

impl Drop for ConfigFile {
    fn drop(&mut self) {
        std::env::remove_var("COSMIC_BWARDEN_CONFIG");
    }
}
