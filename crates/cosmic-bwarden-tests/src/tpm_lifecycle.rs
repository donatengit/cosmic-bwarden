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

use crate::common::register_user;
use crate::common_tpm::TpmTestEnv;
use crate::tpm_skip_if_unavailable;
use anyhow::Result;
use cosmic_bwarden_core::protocol::{Action, EntryType, Response};

const EMAIL: &str = "tpm-lifecycle@example.com";
const PASSWORD: &str = "CorrectHorseBatteryStaple99!";
const PIN: &str = "999888";
const NEW_PIN: &str = "111222";
const WRONG_PIN: &str = "000000";
const WRONG_PASSWORD: &str = "WrongMasterPassword";

// ─── helpers ──────────────────────────────────────────────────────────────

/// Assert TPM status matches expectations.
async fn assert_tpm_status(
    env: &TpmTestEnv,
    expect_available: bool,
    expect_configured: bool,
) -> Result<()> {
    let res = env.client().send(Action::CheckTpm).await?;
    match res {
        Response::TpmStatus { available, configured, .. } => {
            assert_eq!(available, expect_available, "tpm_available mismatch");
            assert_eq!(configured, expect_configured, "tpm_configured mismatch");
        }
        other => anyhow::bail!("CheckTpm returned unexpected response: {:?}", other),
    }
    Ok(())
}

/// Assert vault is unlocked and accessible.
async fn assert_vault_accessible(env: &TpmTestEnv) -> Result<()> {
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
async fn assert_vault_locked(env: &TpmTestEnv) -> Result<()> {
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

async fn lock(env: &TpmTestEnv) -> Result<()> {
    let res = env.client().send(Action::Lock).await?;
    assert!(
        matches!(res, Response::Ack),
        "Lock failed: {:?}",
        res
    );
    Ok(())
}

async fn setup_pin(env: &TpmTestEnv, master_password: &str, pin: &str) -> Result<Response> {
    env.client()
        .send(Action::SetupTpmPin {
            master_password: master_password.to_string(),
            pin: pin.to_string(),
        })
        .await
        .map_err(Into::into)
}

async fn unlock_with_pin(env: &TpmTestEnv, pin: &str) -> Result<Response> {
    env.client()
        .send(Action::UnlockWithPin { pin: pin.to_string() })
        .await
        .map_err(Into::into)
}

async fn disable_pin(env: &TpmTestEnv) -> Result<Response> {
    env.client()
        .send(Action::DisableTpmPin)
        .await
        .map_err(Into::into)
}

async fn unlock_with_password(env: &TpmTestEnv, password: &str) -> Result<Response> {
    env.client()
        .send(Action::Unlock { password: password.to_string() })
        .await
        .map_err(Into::into)
}

async fn enable_server_credentials(env: &TpmTestEnv) -> Result<Response> {
    env.client()
        .send(Action::EnableTpmServerCredentials)
        .await
        .map_err(Into::into)
}

async fn disable_server_credentials(env: &TpmTestEnv) -> Result<Response> {
    env.client()
        .send(Action::DisableTpmServerCredentials)
        .await
        .map_err(Into::into)
}

/// Assert `CheckTpm.server_credentials` matches the expected value.
async fn assert_server_credentials(env: &TpmTestEnv, expect: bool) -> Result<()> {
    let res = env.client().send(Action::CheckTpm).await?;
    match res {
        Response::TpmStatus { server_credentials, .. } => {
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

// ─── tests ────────────────────────────────────────────────────────────────

/// Full end-to-end lifecycle:
/// login → setup PIN → lock → PIN unlock → lock → disable → password unlock
#[tokio::test]
async fn test_tpm_full_lifecycle() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);
    let client = env.client();

    // ── 1. Register and login ───────────────────────────────────────────
    register_user(env.vault_url(), EMAIL, PASSWORD).await?;
    let res = client
        .send(Action::Login {
            email: EMAIL.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url().to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;
    assert!(matches!(res, Response::Ack), "Login failed: {:?}", res);

    // ── 2. Seed a vault entry so we have something to verify later ──────
    client
        .send(Action::AddEntry {
            name: "TPM Lifecycle Test Entry".to_string(),
            entry_type: EntryType::Login,
            username: Some("alice@example.com".to_string()),
            password: Some("s3cret".to_string().into()),
            notes: None,
            fields: Vec::new(),
        })
        .await?;
    client.send(Action::Sync).await?;

    // ── 3. CheckTpm: available=true, configured=false ───────────────────
    assert_tpm_status(&env, true, false).await?;

    // ── 4. SetupTpmPin with wrong master password → rejected ────────────
    let bad_setup = setup_pin(&env, WRONG_PASSWORD, PIN).await?;
    assert!(
        matches!(bad_setup, Response::Error { .. }),
        "SetupTpmPin should reject wrong master password: {:?}",
        bad_setup
    );
    // TPM still unconfigured after rejection
    assert_tpm_status(&env, true, false).await?;

    // ── 5. SetupTpmPin with correct master password → succeeds ──────────
    let ok_setup = setup_pin(&env, PASSWORD, PIN).await?;
    assert!(
        matches!(ok_setup, Response::Ack),
        "SetupTpmPin should succeed: {:?}",
        ok_setup
    );
    assert_tpm_status(&env, true, true).await?;

    // ── 6. Lock vault ───────────────────────────────────────────────────
    lock(&env).await?;
    assert_vault_locked(&env).await?;

    // ── 7. Wrong PIN → rejected, vault remains locked ───────────────────
    let bad_pin = unlock_with_pin(&env, WRONG_PIN).await?;
    assert!(
        matches!(bad_pin, Response::Error { .. }),
        "UnlockWithPin should reject wrong PIN: {:?}",
        bad_pin
    );
    assert_vault_locked(&env).await?;

    // ── 8. Correct PIN → vault unlocked ─────────────────────────────────
    let ok_pin = unlock_with_pin(&env, PIN).await?;
    assert!(
        matches!(ok_pin, Response::Ack),
        "UnlockWithPin should succeed with correct PIN: {:?}",
        ok_pin
    );
    assert_vault_accessible(&env).await?;

    // Verify the seeded entry is present and decrypted correctly
    let entries_res = client
        .send(Action::GetEntries {
            query: Some("TPM Lifecycle".to_string()),
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    if let Response::Entries { entries } = entries_res {
        let found = entries.iter().any(|e| e.name.contains("TPM Lifecycle"));
        assert!(found, "seeded entry not found after PIN unlock");
    }

    // ── 9. Lock and unlock again — idempotent ───────────────────────────
    lock(&env).await?;
    let ok_pin2 = unlock_with_pin(&env, PIN).await?;
    assert!(matches!(ok_pin2, Response::Ack), "second PIN unlock failed: {:?}", ok_pin2);
    assert_vault_accessible(&env).await?;

    // ── 10. DisableTpmPin (vault is unlocked — no password needed) ──────────
    let ok_disable = disable_pin(&env).await?;
    assert!(
        matches!(ok_disable, Response::Ack),
        "DisableTpmPin should succeed while vault is unlocked: {:?}",
        ok_disable
    );
    assert_tpm_status(&env, true, false).await?;

    // ── 11. PIN unlock after disable → rejected ──────────────────────────
    lock(&env).await?;
    let pin_after_disable = unlock_with_pin(&env, PIN).await?;
    assert!(
        matches!(pin_after_disable, Response::Error { .. }),
        "UnlockWithPin should fail after disable: {:?}",
        pin_after_disable
    );
    assert_vault_locked(&env).await?;

    // ── 12. Password unlock still works after disable ────────────────────
    let pw_unlock = unlock_with_password(&env, PASSWORD).await?;
    assert!(
        matches!(pw_unlock, Response::Ack),
        "password unlock should succeed after TPM disable: {:?}",
        pw_unlock
    );
    assert_vault_accessible(&env).await?;

    // ── 13. Re-setup TPM PIN → idempotent second enrollment ─────────────
    let re_setup = setup_pin(&env, PASSWORD, NEW_PIN).await?;
    assert!(
        matches!(re_setup, Response::Ack),
        "re-SetupTpmPin should succeed: {:?}",
        re_setup
    );
    assert_tpm_status(&env, true, true).await?;

    lock(&env).await?;

    // Old PIN from first enrollment no longer works (blob was replaced)
    let old_pin_res = unlock_with_pin(&env, PIN).await?;
    assert!(
        matches!(old_pin_res, Response::Error { .. }),
        "old PIN should be rejected after re-enrollment: {:?}",
        old_pin_res
    );

    // New PIN works
    let new_pin_res = unlock_with_pin(&env, NEW_PIN).await?;
    assert!(
        matches!(new_pin_res, Response::Ack),
        "new PIN should unlock after re-enrollment: {:?}",
        new_pin_res
    );
    assert_vault_accessible(&env).await?;

    Ok(())
}

/// PIN unlock is blocked when the TPM blob file is absent (e.g., deleted by
/// the user or after logout-and-back-in on a different machine).
#[tokio::test]
async fn test_tpm_pin_without_blob_fails() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);

    let email = "tpm-no-blob@example.com";
    register_user(env.vault_url(), email, PASSWORD).await?;

    let client = env.client();
    client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url().to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    // No SetupTpmPin called — blob file doesn't exist.
    lock(&env).await?;

    let res = unlock_with_pin(&env, PIN).await?;
    assert!(
        matches!(res, Response::Error { .. }),
        "UnlockWithPin without setup should fail: {:?}",
        res
    );

    Ok(())
}

/// CheckTpm before any login / account setup returns available=true but
/// configured=false.
#[tokio::test]
async fn test_tpm_status_unauthenticated() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);
    // No login — agent starts fresh.
    assert_tpm_status(&env, true, false).await?;
    Ok(())
}

/// Master password unlock works correctly while TPM PIN is also configured
/// (the two unlock paths are independent).
#[tokio::test]
async fn test_password_unlock_coexists_with_tpm() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);

    let email = "tpm-coexist@example.com";
    register_user(env.vault_url(), email, PASSWORD).await?;

    let client = env.client();
    client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url().to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    // Setup PIN
    let s = setup_pin(&env, PASSWORD, PIN).await?;
    assert!(matches!(s, Response::Ack), "setup failed: {:?}", s);

    // Lock and unlock with master password (not PIN)
    lock(&env).await?;
    let pw = unlock_with_password(&env, PASSWORD).await?;
    assert!(matches!(pw, Response::Ack), "password unlock failed while TPM configured: {:?}", pw);
    assert_vault_accessible(&env).await?;

    // Lock and unlock with PIN
    lock(&env).await?;
    let pin = unlock_with_pin(&env, PIN).await?;
    assert!(matches!(pin, Response::Ack), "PIN unlock failed: {:?}", pin);
    assert_vault_accessible(&env).await?;

    Ok(())
}

/// SetupTpmPin is idempotent: calling it a second time replaces the old blob.
#[tokio::test]
async fn test_tpm_setup_is_idempotent() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);

    let email = "tpm-idempotent@example.com";
    register_user(env.vault_url(), email, PASSWORD).await?;

    let client = env.client();
    client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url().to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    // First enrollment
    let s1 = setup_pin(&env, PASSWORD, PIN).await?;
    assert!(matches!(s1, Response::Ack), "first setup failed: {:?}", s1);

    // Second enrollment (different PIN)
    let s2 = setup_pin(&env, PASSWORD, NEW_PIN).await?;
    assert!(matches!(s2, Response::Ack), "second setup failed: {:?}", s2);

    // Old PIN no longer works
    lock(&env).await?;
    let old = unlock_with_pin(&env, PIN).await?;
    assert!(
        matches!(old, Response::Error { .. }),
        "old PIN should be invalid after re-enrollment: {:?}",
        old
    );

    // New PIN works
    let new = unlock_with_pin(&env, NEW_PIN).await?;
    assert!(matches!(new, Response::Ack), "new PIN should work: {:?}", new);

    Ok(())
}

/// Vault data integrity: entries added before TPM setup are fully accessible
/// after PIN unlock (same keys, no re-encryption).
#[tokio::test]
async fn test_vault_data_survives_tpm_roundtrip() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);

    let email = "tpm-data@example.com";
    register_user(env.vault_url(), email, PASSWORD).await?;

    let client = env.client();
    client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url().to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    // Create entries with various types before TPM setup
    let entries_to_create = vec![
        ("Login Entry", EntryType::Login),
        ("Secure Note", EntryType::SecureNote),
    ];
    for (name, entry_type) in &entries_to_create {
        client
            .send(Action::AddEntry {
                name: name.to_string(),
                entry_type: *entry_type,
                username: Some("user@test.com".to_string()),
                password: Some("pass123".to_string().into()),
                notes: Some(format!("Note for {}", name).into()),
                fields: Vec::new(),
            })
            .await?;
    }
    client.send(Action::Sync).await?;

    // Read entries before PIN setup (baseline)
    let before = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let count_before = match before {
        Response::Entries { entries } => entries.len(),
        other => anyhow::bail!("GetEntries failed before setup: {:?}", other),
    };

    // Setup PIN
    let s = setup_pin(&env, PASSWORD, PIN).await?;
    assert!(matches!(s, Response::Ack), "setup failed: {:?}", s);

    // Lock and re-unlock via PIN
    lock(&env).await?;
    let p = unlock_with_pin(&env, PIN).await?;
    assert!(matches!(p, Response::Ack), "PIN unlock failed: {:?}", p);

    // Entry count must match
    let after = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let count_after = match after {
        Response::Entries { entries } => entries.len(),
        other => anyhow::bail!("GetEntries failed after PIN unlock: {:?}", other),
    };

    assert_eq!(
        count_before, count_after,
        "entry count changed after TPM round-trip ({} -> {})",
        count_before, count_after
    );

    Ok(())
}

/// DisableTpmPin is rejected when the vault is locked (no authorization).
#[tokio::test]
async fn test_disable_pin_rejected_when_locked() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);

    let email = "tpm-disable-locked@example.com";
    register_user(env.vault_url(), email, PASSWORD).await?;

    let client = env.client();
    client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url().to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    let s = setup_pin(&env, PASSWORD, PIN).await?;
    assert!(matches!(s, Response::Ack), "setup failed: {:?}", s);

    lock(&env).await?;
    assert_vault_locked(&env).await?;

    // Vault is locked — disable should be rejected.
    let res = disable_pin(&env).await?;
    assert!(
        matches!(res, Response::Error { .. }),
        "DisableTpmPin should be rejected while vault is locked: {:?}",
        res
    );

    // TPM still configured.
    assert_tpm_status(&env, true, true).await?;

    Ok(())
}

/// Logout and re-login: TPM PIN blob must survive the session; a fresh login
/// with the correct PIN works after logout → re-login.
#[tokio::test]
async fn test_tpm_pin_survives_logout_login() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);

    let email = "tpm-logout@example.com";
    register_user(env.vault_url(), email, PASSWORD).await?;

    let client = env.client();
    client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url().to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    let s = setup_pin(&env, PASSWORD, PIN).await?;
    assert!(matches!(s, Response::Ack), "setup failed: {:?}", s);

    // Logout clears in-memory keys but must NOT delete the blob.
    let logout_res = client.send(Action::Logout).await?;
    assert!(matches!(logout_res, Response::Ack), "Logout failed: {:?}", logout_res);

    // Re-login with master password.
    let re_login = client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url().to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;
    assert!(matches!(re_login, Response::Ack), "re-login failed: {:?}", re_login);

    // Blob should still be present → configured=true.
    assert_tpm_status(&env, true, true).await?;

    // Lock and unlock via PIN — should still work.
    lock(&env).await?;
    let pin_res = unlock_with_pin(&env, PIN).await?;
    assert!(matches!(pin_res, Response::Ack), "PIN unlock after re-login failed: {:?}", pin_res);
    assert_vault_accessible(&env).await?;

    Ok(())
}

/// Multiple consecutive lock/unlock cycles are stable (no state corruption
/// across cycles, no accumulated errors).
#[tokio::test]
async fn test_tpm_multiple_lock_unlock_cycles() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);

    let email = "tpm-cycles@example.com";
    register_user(env.vault_url(), email, PASSWORD).await?;

    let client = env.client();
    client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url().to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    let s = setup_pin(&env, PASSWORD, PIN).await?;
    assert!(matches!(s, Response::Ack), "setup failed: {:?}", s);

    for i in 0..5 {
        lock(&env).await?;
        assert_vault_locked(&env).await?;

        let res = unlock_with_pin(&env, PIN).await?;
        assert!(
            matches!(res, Response::Ack),
            "PIN unlock failed on cycle {}: {:?}",
            i,
            res
        );
        assert_vault_accessible(&env).await?;
    }

    Ok(())
}

// ─── server-credentials (TPM-sealed hash) tests ───────────────────────────

/// Without the server-credentials blob, Sync fails after PIN unlock when no
/// session token is available.
///
/// Key conditions:
///   - `remember_me: false` → no keyring backup of the session token
///   - Lock clears the in-memory token from state
///   - PIN unlock can't restore the token (no blob, no keyring, no in-memory)
///   - GetEntries (local) still works — vault keys are unsealed
///   - Sync (server) fails with "no API session token"
#[tokio::test]
async fn test_pin_unlock_without_server_credentials_sync_fails() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);
    let client = env.client();

    let email = "tpm-nocreds@example.com";
    register_user(env.vault_url(), email, PASSWORD).await?;

    // Login WITHOUT keyring persistence so there is no token backup.
    let res = client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url().to_string()),
            remember_me: false,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;
    assert!(matches!(res, Response::Ack), "Login failed: {:?}", res);

    let s = setup_pin(&env, PASSWORD, PIN).await?;
    assert!(matches!(s, Response::Ack), "setup_pin failed: {:?}", s);

    // Confirm server_credentials blob does NOT exist before we start.
    assert_server_credentials(&env, false).await?;

    // Lock vault — clears the in-memory session token.
    lock(&env).await?;

    // PIN unlock: no keyring (persist_session=false), no in-memory token (cleared
    // by lock), no hash blob → vault is locally accessible but server ops fail.
    let ok = unlock_with_pin(&env, PIN).await?;
    assert!(matches!(ok, Response::Ack), "PIN unlock should succeed: {:?}", ok);

    // Local vault access works (symmetric keys unsealed from TPM).
    assert_vault_accessible(&env).await?;

    // Sync must fail — no session token available for server API call.
    let sync_res = client.send(Action::Sync).await?;
    assert!(
        matches!(sync_res, Response::Error { .. }),
        "Sync should fail without server credentials after PIN unlock: {:?}",
        sync_res
    );

    Ok(())
}

/// With the server-credentials blob sealed, Sync succeeds after PIN unlock
/// because the master_password_hash is unsealed and used for silent re-auth.
///
/// Key conditions:
///   - `remember_me: false` → no keyring backup
///   - Lock clears the in-memory token
///   - PIN unlock unseals the hash blob → silent re-auth → new token obtained
///   - Both GetEntries and Sync succeed
#[tokio::test]
async fn test_pin_unlock_with_server_credentials_sync_succeeds() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);
    let client = env.client();

    let email = "tpm-withcreds@example.com";
    register_user(env.vault_url(), email, PASSWORD).await?;

    // Login WITHOUT keyring persistence.
    let res = client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url().to_string()),
            remember_me: false,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;
    assert!(matches!(res, Response::Ack), "Login failed: {:?}", res);

    let s = setup_pin(&env, PASSWORD, PIN).await?;
    assert!(matches!(s, Response::Ack), "setup_pin failed: {:?}", s);

    // Vault is unlocked with master password → master_password_hash is in memory.
    // Seal it into the TPM so PIN unlock can recover it later.
    let enable_res = enable_server_credentials(&env).await?;
    assert!(
        matches!(enable_res, Response::Ack),
        "EnableTpmServerCredentials should succeed while unlocked via master password: {:?}",
        enable_res
    );

    // Confirm both blobs are now present.
    assert_tpm_status(&env, true, true).await?;
    assert_server_credentials(&env, true).await?;

    // Lock vault — clears the in-memory session token.
    lock(&env).await?;

    // PIN unlock: hash blob unsealed → master_password_hash restored →
    // silent re-auth → new session token obtained inside handle_unlock_with_pin.
    let ok = unlock_with_pin(&env, PIN).await?;
    assert!(matches!(ok, Response::Ack), "PIN unlock should succeed: {:?}", ok);

    // Local vault access works.
    assert_vault_accessible(&env).await?;

    // Sync MUST succeed — token was silently refreshed via the sealed hash.
    let sync_res = client.send(Action::Sync).await?;
    assert!(
        matches!(sync_res, Response::Ack),
        "Sync should succeed after PIN unlock with server credentials: {:?}",
        sync_res
    );

    Ok(())
}

/// EnableTpmServerCredentials is rejected when the vault was unlocked with PIN
/// only (master_password_hash is None because no hash blob existed at that point).
///
/// The user must first unlock with their master password to enable this feature.
#[tokio::test]
async fn test_enable_server_credentials_rejected_after_pin_only_unlock() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);
    let client = env.client();

    let email = "tpm-pinonly@example.com";
    register_user(env.vault_url(), email, PASSWORD).await?;

    client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url().to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    let s = setup_pin(&env, PASSWORD, PIN).await?;
    assert!(matches!(s, Response::Ack), "setup_pin failed: {:?}", s);

    // Lock and re-unlock with PIN — no hash blob → master_password_hash stays None.
    lock(&env).await?;
    let ok = unlock_with_pin(&env, PIN).await?;
    assert!(matches!(ok, Response::Ack), "PIN unlock should succeed: {:?}", ok);
    assert_vault_accessible(&env).await?;

    // Try to enable server credentials — should fail: master_password_hash not in memory.
    let enable_res = enable_server_credentials(&env).await?;
    assert!(
        matches!(enable_res, Response::Error { .. }),
        "EnableTpmServerCredentials should fail when master_password_hash is absent: {:?}",
        enable_res
    );

    // Hash blob must NOT have been created.
    assert_server_credentials(&env, false).await?;

    Ok(())
}

/// DisableTpmPin removes BOTH the vault-keys blob and the server-credentials blob.
/// After disable, CheckTpm reports both configured=false and server_credentials=false.
#[tokio::test]
async fn test_disable_pin_removes_server_credentials_blob() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);
    let client = env.client();

    let email = "tpm-cleanall@example.com";
    register_user(env.vault_url(), email, PASSWORD).await?;

    client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url().to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    let s = setup_pin(&env, PASSWORD, PIN).await?;
    assert!(matches!(s, Response::Ack), "setup_pin failed: {:?}", s);

    let e = enable_server_credentials(&env).await?;
    assert!(matches!(e, Response::Ack), "enable_server_credentials failed: {:?}", e);

    // Both blobs must exist.
    assert_tpm_status(&env, true, true).await?;
    assert_server_credentials(&env, true).await?;

    // Disable PIN (vault is unlocked — no password needed).
    let ok = disable_pin(&env).await?;
    assert!(matches!(ok, Response::Ack), "disable_pin failed: {:?}", ok);

    // Both blobs must be gone — the hash blob is orphaned without the vault-keys blob.
    assert_tpm_status(&env, true, false).await?;
    assert_server_credentials(&env, false).await?;

    // DisableServerCredentials alone also works when server_creds are standalone.
    // Re-enable PIN, enable server creds, then disable only the creds.
    let s2 = setup_pin(&env, PASSWORD, PIN).await?;
    assert!(matches!(s2, Response::Ack), "re-setup_pin failed: {:?}", s2);

    // Vault was just re-locked by the previous enable flow... unlock with password.
    let uw = client
        .send(Action::Unlock {
            password: PASSWORD.to_string(),
        })
        .await?;
    assert!(matches!(uw, Response::Ack), "password unlock failed: {:?}", uw);

    let e2 = enable_server_credentials(&env).await?;
    assert!(matches!(e2, Response::Ack), "second enable failed: {:?}", e2);
    assert_server_credentials(&env, true).await?;

    // Disable only server credentials; PIN should remain.
    let dc = disable_server_credentials(&env).await?;
    assert!(matches!(dc, Response::Ack), "disable_server_credentials failed: {:?}", dc);

    assert_tpm_status(&env, true, true).await?;   // PIN still configured
    assert_server_credentials(&env, false).await?; // hash blob gone

    Ok(())
}
