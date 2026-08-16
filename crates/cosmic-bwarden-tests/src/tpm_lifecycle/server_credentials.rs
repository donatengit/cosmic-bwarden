//! Server-credentials (sealed master-password-hash) sync behavior.

use super::*;
use crate::tpm_skip_if_unavailable;

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
    assert!(
        matches!(ok, Response::Ack),
        "PIN unlock should succeed: {:?}",
        ok
    );

    // Local vault access works (symmetric keys unsealed from TPM).
    assert_vault_accessible(&env).await?;

    // The degraded state must be visible in GetConfig IMMEDIATELY after the
    // unlock — the agent knows sync is impossible (no token, no hash blob)
    // and must not report a healthy unlocked vault.
    let cfg_res = client.send(Action::GetConfig).await?;
    match cfg_res {
        Response::Config {
            sync_failed,
            is_locked,
            ..
        } => {
            assert!(!is_locked, "vault must be unlocked after PIN unlock");
            assert!(
                sync_failed,
                "degraded PIN unlock (no token, no hash blob) must set sync_failed"
            );
        }
        other => anyhow::bail!("expected Config, got: {:?}", other),
    }

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
    assert!(
        matches!(ok, Response::Ack),
        "PIN unlock should succeed: {:?}",
        ok
    );

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
    assert!(
        matches!(ok, Response::Ack),
        "PIN unlock should succeed: {:?}",
        ok
    );
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
    assert!(
        matches!(e, Response::Ack),
        "enable_server_credentials failed: {:?}",
        e
    );

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
    assert!(
        matches!(uw, Response::Ack),
        "password unlock failed: {:?}",
        uw
    );

    let e2 = enable_server_credentials(&env).await?;
    assert!(
        matches!(e2, Response::Ack),
        "second enable failed: {:?}",
        e2
    );
    assert_server_credentials(&env, true).await?;

    // Disable only server credentials; PIN should remain.
    let dc = disable_server_credentials(&env).await?;
    assert!(
        matches!(dc, Response::Ack),
        "disable_server_credentials failed: {:?}",
        dc
    );

    assert_tpm_status(&env, true, true).await?; // PIN still configured
    assert_server_credentials(&env, false).await?; // hash blob gone

    Ok(())
}
