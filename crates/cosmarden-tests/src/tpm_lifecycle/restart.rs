//! Agent restart with a sealed PIN blob: `tpm_configured` is detected from
//! blob existence at startup, so a PIN unlock works without any re-setup.

use super::*;
use crate::tpm_skip_if_unavailable;

#[tokio::test]
async fn test_pin_survives_agent_restart() -> Result<()> {
    let mut env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);

    let email = "tpm-restart@example.com";
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
    assert!(matches!(s, Response::Ack), "setup_pin failed: {:?}", s);
    lock(&env).await?;

    // Simulate an agent crash/restart while locked.
    env.restart_agent()?;
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // Startup must detect the blob and report the PIN as configured, with
    // the vault locked (keys are memory-only).
    assert_tpm_status(&env, true, true).await?;
    assert_vault_locked(&env).await?;

    // The sealed blob is on disk and the TPM state matches — PIN unlock
    // works without re-login or re-setup.
    let ok = unlock_with_pin(&env, PIN).await?;
    assert!(
        matches!(ok, Response::Ack),
        "PIN unlock after agent restart failed: {:?}",
        ok
    );
    assert_vault_accessible(&env).await?;

    Ok(())
}
