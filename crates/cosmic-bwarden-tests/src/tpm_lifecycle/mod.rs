//! Full TPM PIN-unlock lifecycle E2E tests.
//!
//! Requires:
//!   - `swtpm` + `swtpm_setup` in PATH
//!   - `target/debug/cosmic-bwarden-agent-tpm`
//!     (`cargo build -p cosmic-bwarden-agent --features tpm`)
//!   - Docker / Podman for the Vaultwarden container
//!
//! Run with:
//!   cargo test -p cosmic-bwarden-tests --features tpm-smoke \
//!     -- tpm_lifecycle --test-threads=1
//!
//! Split by scenario; shared fixtures/helpers live here and are pulled in by each
//! submodule via `use super::*`.

// Re-exported so the test submodules get everything from a single `use super::*`.
pub(super) use crate::common::register_user;
pub(super) use crate::common_tpm::TpmTestEnv;
pub(super) use anyhow::Result;
pub(super) use cosmic_bwarden_core::protocol::{Action, EntryType, Response};

mod cycles;
mod errors_and_setup;
mod full_lifecycle;
mod lockout;
mod restart;
mod server_credentials;
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

pub(super) async fn enable_server_credentials(env: &TpmTestEnv) -> Result<Response> {
    env.client()
        .send(Action::EnableTpmServerCredentials)
        .await
        .map_err(Into::into)
}

pub(super) async fn disable_server_credentials(env: &TpmTestEnv) -> Result<Response> {
    env.client()
        .send(Action::DisableTpmServerCredentials)
        .await
        .map_err(Into::into)
}

/// Assert `CheckTpm.server_credentials` matches the expected value.
pub(super) async fn assert_server_credentials(env: &TpmTestEnv, expect: bool) -> Result<()> {
    let res = env.client().send(Action::CheckTpm).await?;
    match res {
        Response::TpmStatus {
            server_credentials, ..
        } => {
            assert_eq!(
                server_credentials, expect,
                "server_credentials mismatch (expected {})",
                expect
            );
        }
        other => anyhow::bail!("CheckTpm returned unexpected response: {:?}", other),
    }
    Ok(())
}
