//! How a PIN unlock recovers (or fails to recover) the server session.

use super::*;
use crate::tpm_skip_if_unavailable;

/// A PIN unlock restores server sync from the stored refresh-token envelope,
/// with no keyring involved.
///
/// This is the only re-auth path a PIN unlock has: the refresh grant is
/// revocable, expires on its own, and never has to satisfy 2FA. A PIN unlock
/// never holds the master password, so when this fails the user is asked for
/// it — nothing weaker is kept on disk to avoid that prompt.
///
/// Key conditions:
///   - `remember_me: false` → no keyring backup of the session token
///   - Lock clears the in-memory token from state
///   - PIN unlock exchanges the stored refresh token for a fresh session
#[tokio::test]
async fn test_pin_unlock_restores_sync_from_session_envelope() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);
    let client = env.client();

    let email = "tpm-envelope@example.com";
    register_user(env.vault_url(), email, PASSWORD).await?;

    // Login WITHOUT keyring persistence: the envelope must carry this alone.
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

    // Login wrote the envelope, and the refresh token is not in it in the clear.
    let envelope = session_envelope_path(&env, email);
    let raw = std::fs::read(&envelope)
        .map_err(|e| anyhow::anyhow!("login must persist {}: {e}", envelope.display()))?;
    assert!(
        !raw.windows(3).any(|w| w == b"eyJ"),
        "a JWT prefix appeared in the envelope — it is not encrypted"
    );

    // Lock vault — clears the in-memory session token.
    lock(&env).await?;

    let ok = unlock_with_pin(&env, PIN).await?;
    assert!(
        matches!(ok, Response::Ack),
        "PIN unlock should succeed: {:?}",
        ok
    );
    assert_vault_accessible(&env).await?;

    // The agent must report a healthy vault: a session was restored.
    match client.send(Action::GetConfig).await? {
        Response::Config {
            sync_failed,
            is_locked,
            ..
        } => {
            assert!(!is_locked, "vault must be unlocked after PIN unlock");
            assert!(
                !sync_failed,
                "PIN unlock restored a session, so sync_failed must be clear"
            );
        }
        other => anyhow::bail!("expected Config, got: {:?}", other),
    }

    let sync_res = client.send(Action::Sync).await?;
    assert!(
        matches!(sync_res, Response::Ack),
        "Sync should succeed from the stored refresh token: {:?}",
        sync_res
    );

    Ok(())
}

/// With no re-auth source — no keyring and no stored refresh token — the vault
/// still unlocks locally, but Sync fails honestly and the degraded state is
/// visible immediately.
///
/// This is the floor: a truthful failure that sends the user to their master
/// password, never an `Ack` that pretends the vault is in sync.
#[tokio::test]
async fn test_pin_unlock_without_any_credential_sync_fails() -> Result<()> {
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

    // Lock vault — clears the in-memory session token.
    lock(&env).await?;

    // Drop the last remaining source. Done after the lock so the envelope is
    // gone for the unlock but its creation was still asserted by login.
    remove_session_envelope(&env, email)?;

    // PIN unlock: no keyring (persist_session=false), no in-memory token
    // (cleared by lock), no envelope → locally accessible, server ops fail.
    let ok = unlock_with_pin(&env, PIN).await?;
    assert!(
        matches!(ok, Response::Ack),
        "PIN unlock should succeed: {:?}",
        ok
    );

    // Local vault access works (symmetric keys unsealed from TPM).
    assert_vault_accessible(&env).await?;

    // The degraded state must be visible in GetConfig IMMEDIATELY after the
    // unlock — the agent knows sync is impossible and must not report a
    // healthy unlocked vault.
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
                "degraded PIN unlock (no token, no envelope) must set sync_failed"
            );
        }
        other => anyhow::bail!("expected Config, got: {:?}", other),
    }

    // Sync must fail — no session token available for server API call.
    let sync_res = client.send(Action::Sync).await?;
    assert!(
        matches!(sync_res, Response::Error { .. }),
        "Sync should fail with no re-auth source after PIN unlock: {:?}",
        sync_res
    );

    Ok(())
}

/// A sync that finds no token in memory re-mints one from the stored envelope
/// instead of demanding the master password.
///
/// This is the transient-outage case: the unlock's own restore attempt failed
/// (server unreachable at that moment), so the agent holds no token — but the
/// envelope on disk is still valid. Before `with_refresh` learned to open it,
/// the only ways out were another lock/unlock cycle or a master-password
/// prompt, neither of which the situation actually warranted.
///
/// The outage is simulated by moving the envelope aside across the unlock and
/// back afterwards, which is deterministic where a real network failure is not.
#[tokio::test]
async fn test_sync_recovers_from_the_envelope_without_a_master_password() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);
    let client = env.client();

    let email = "tpm-sync-recovers@example.com";
    register_user(env.vault_url(), email, PASSWORD).await?;

    let res = client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url().to_string()),
            // No keyring fallback: the envelope must be the only way back.
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

    lock(&env).await?;

    // Hide the envelope so the unlock's restore fails, as it would with the
    // server unreachable.
    let envelope = session_envelope_path(&env, email);
    let stashed = envelope.with_extension("enc.stashed");
    std::fs::rename(&envelope, &stashed)?;

    let ok = unlock_with_pin(&env, PIN).await?;
    assert!(
        matches!(ok, Response::Ack),
        "PIN unlock should still succeed: {:?}",
        ok
    );
    assert_vault_accessible(&env).await?;

    // Degraded: unlocked locally, no server session.
    match client.send(Action::GetConfig).await? {
        Response::Config {
            sync_failed,
            is_locked,
            ..
        } => {
            assert!(!is_locked, "vault must be unlocked");
            assert!(sync_failed, "a failed restore must set sync_failed");
        }
        other => anyhow::bail!("expected Config, got: {:?}", other),
    }

    // The "outage" ends: the envelope is available again.
    std::fs::rename(&stashed, &envelope)?;

    // Sync alone must recover — no unlock, no master password.
    let sync_res = client.send(Action::Sync).await?;
    assert!(
        matches!(sync_res, Response::Ack),
        "Sync should re-mint a session from the envelope: {:?}",
        sync_res
    );

    match client.send(Action::GetConfig).await? {
        Response::Config { sync_failed, .. } => {
            assert!(!sync_failed, "a recovered sync must clear sync_failed");
        }
        other => anyhow::bail!("expected Config, got: {:?}", other),
    }

    Ok(())
}
