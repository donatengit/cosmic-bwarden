//! PIN error paths, status when unauthenticated, idempotent setup, and vault-data survival.

use super::*;
use crate::tpm_skip_if_unavailable;

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
    assert!(
        matches!(pw, Response::Ack),
        "password unlock failed while TPM configured: {:?}",
        pw
    );
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
    assert!(
        matches!(new, Response::Ack),
        "new PIN should work: {:?}",
        new
    );

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
                totp: None,
                uris: Vec::new(),
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
