//! Lock/unlock cycles, disable-while-locked rejection, and PIN persistence across logout/login.

use super::*;
use crate::tpm_skip_if_unavailable;

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
