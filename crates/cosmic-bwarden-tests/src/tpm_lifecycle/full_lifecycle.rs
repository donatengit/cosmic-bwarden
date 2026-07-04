//! End-to-end PIN lifecycle: login → setup → lock → PIN unlock → disable → password unlock.

use super::*;
use crate::tpm_skip_if_unavailable;

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
    assert!(
        matches!(ok_pin2, Response::Ack),
        "second PIN unlock failed: {:?}",
        ok_pin2
    );
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
