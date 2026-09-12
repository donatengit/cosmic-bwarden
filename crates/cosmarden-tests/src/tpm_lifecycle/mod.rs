//! Full TPM PIN-unlock lifecycle E2E tests.
//!
//! Requires:
//!   - `swtpm` + `swtpm_setup` in PATH
//!   - `target/debug/cosmarden-agent-tpm`
//!     (`cargo build -p cosmarden-agent --features tpm`)
//!   - Docker / Podman for the Vaultwarden container
//!
//! Run with:
//!   cargo test -p cosmarden-tests --features tpm-smoke \
//!     -- tpm_lifecycle --test-threads=1
//!
//! Split by scenario; shared fixtures/helpers live here and are pulled in by each
//! submodule via `use super::*`.

// Re-exported so the test submodules get everything from a single `use super::*`.
pub(super) use crate::common::register_user;
pub(super) use crate::common_tpm::TpmTestEnv;
pub(super) use anyhow::Result;
pub(super) use cosmarden_core::protocol::{Action, EntryType, Response};

mod cycles;
mod errors_and_setup;
mod full_lifecycle;
mod lockout;
mod restart;
mod session_restore;
mod state_changed;

pub(super) const EMAIL: &str = "tpm-lifecycle@example.com";
pub(super) const PASSWORD: &str = "CorrectHorseBatteryStaple99!";
pub(super) const PIN: &str = "999888";
pub(super) const NEW_PIN: &str = "111222";
pub(super) const WRONG_PIN: &str = "000000";
pub(super) const WRONG_PASSWORD: &str = "WrongMasterPassword";

// ─── helpers ──────────────────────────────────────────────────────────────

/// Assert TPM status matches expectations.
pub(super) async fn assert_tpm_status(
    env: &TpmTestEnv,
    expect_available: bool,
    expect_configured: bool,
) -> Result<()> {
    let res = env.client().send(Action::CheckTpm).await?;
    match res {
        Response::TpmStatus {
            available,
            configured,
            ..
        } => {
            assert_eq!(available, expect_available, "tpm_available mismatch");
            assert_eq!(configured, expect_configured, "tpm_configured mismatch");
        }
        other => anyhow::bail!("CheckTpm returned unexpected response: {:?}", other),
    }
    Ok(())
}

/// Assert vault is unlocked and accessible.
pub(super) async fn assert_vault_accessible(env: &TpmTestEnv) -> Result<()> {
    let res = env
        .client()
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    assert!(
        matches!(res, Response::Entries { .. }),
        "expected Entries (vault accessible), got {:?}",
        res
    );
    Ok(())
}

/// Assert vault is locked (GetEntries returns error).
pub(super) async fn assert_vault_locked(env: &TpmTestEnv) -> Result<()> {
    let res = env
        .client()
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    assert!(
        matches!(res, Response::Error { .. }),
        "expected Error (vault locked), got {:?}",
        res
    );
    Ok(())
}

pub(super) async fn lock(env: &TpmTestEnv) -> Result<()> {
    let res = env.client().send(Action::Lock).await?;
    assert!(matches!(res, Response::Ack), "Lock failed: {:?}", res);
    Ok(())
}

pub(super) async fn setup_pin(
    env: &TpmTestEnv,
    master_password: &str,
    pin: &str,
) -> Result<Response> {
    env.client()
        .send(Action::SetupTpmPin {
            master_password: master_password.to_string(),
            pin: pin.to_string(),
        })
        .await
        .map_err(Into::into)
}

pub(super) async fn unlock_with_pin(env: &TpmTestEnv, pin: &str) -> Result<Response> {
    env.client()
        .send(Action::UnlockWithPin {
            pin: pin.to_string(),
        })
        .await
        .map_err(Into::into)
}

pub(super) async fn disable_pin(env: &TpmTestEnv) -> Result<Response> {
    env.client()
        .send(Action::DisableTpmPin)
        .await
        .map_err(Into::into)
}

pub(super) async fn unlock_with_password(env: &TpmTestEnv, password: &str) -> Result<Response> {
    env.client()
        .send(Action::Unlock {
            password: password.to_string(),
        })
        .await
        .map_err(Into::into)
}

/// Path of the agent's encrypted session envelope for `email`. Resolved
/// against the *agent's* XDG_DATA_HOME and profile, not this test process's,
/// and via core's own `account_hash` so the two can never drift.
pub(super) fn session_envelope_path(env: &TpmTestEnv, email: &str) -> std::path::PathBuf {
    let server = env.vault_url();
    env.inner
        .data_home
        .join(format!("cosmarden-{}", env.inner.profile))
        .join(format!(
            "session_{}.enc",
            cosmarden_core::dirs::account_hash(server, email)
        ))
}

/// Delete the stored refresh-token envelope, forcing the next PIN unlock to
/// fall through to whatever weaker source remains. Lets a test isolate one
/// re-auth source instead of silently passing on the first one that works.
pub(super) fn remove_session_envelope(env: &TpmTestEnv, email: &str) -> Result<()> {
    let path = session_envelope_path(env, email);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("expected a session envelope at {}", path.display())
        }
        Err(e) => Err(e.into()),
    }
}
