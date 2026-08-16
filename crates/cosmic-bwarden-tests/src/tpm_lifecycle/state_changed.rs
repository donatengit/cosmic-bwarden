//! PCR-state change (BIOS/firmware update simulation): the PIN itself stays
//! valid, the TPM refuses to unseal because the policy check fails, and the
//! agent must report the stable `ERR_TPM_STATE_CHANGED` — never "wrong PIN".
//! Recovery is master-password unlock + re-seal.

use super::*;
use crate::tpm_skip_if_unavailable;
use cosmic_bwarden_core::protocol::ERR_TPM_STATE_CHANGED;

#[tokio::test]
async fn test_pcr_state_change_blocks_pin_with_state_changed_error_and_recovers() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);

    let email = "tpm-pcr-change@example.com";
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

    // A correct PIN still works before the state change.
    let ok = unlock_with_pin(&env, PIN).await?;
    assert!(matches!(ok, Response::Ack), "pre-change unlock failed: {:?}", ok);
    lock(&env).await?;

    // Simulate a BIOS/firmware update: extend PCR 7, breaking the policy
    // digest the PIN blob was sealed against.
    env.extend_pcr7()?;

    // Record the DA counter: a policy failure must NOT consume an attempt.
    let da_before = da_remaining(&env).await?;

    // The PIN is correct — but the machine state moved. The agent must say
    // so with the stable state-changed message, not "wrong PIN".
    let res = unlock_with_pin(&env, PIN).await?;
    match res {
        Response::Error { message } => {
            assert_eq!(
                message, ERR_TPM_STATE_CHANGED,
                "PCR change must map to ERR_TPM_STATE_CHANGED"
            );
        }
        other => anyhow::bail!("expected ERR_TPM_STATE_CHANGED, got: {:?}", other),
    }

    // No dictionary-attack attempt was consumed (policy failure is not an
    // auth failure).
    let da_after = da_remaining(&env).await?;
    assert_eq!(
        da_after, da_before,
        "PCR-change failure must not consume a DA attempt"
    );

    // Recovery: master-password unlock is untouched by the PCR state, and
    // re-sealing the PIN against the new PCRs restores PIN unlock.
    let res = unlock_with_password(&env, PASSWORD).await?;
    assert!(
        matches!(res, Response::Ack),
        "master-password unlock must work after PCR change: {:?}",
        res
    );
    let s = setup_pin(&env, PASSWORD, PIN).await?;
    assert!(matches!(s, Response::Ack), "re-seal failed: {:?}", s);

    lock(&env).await?;
    let ok = unlock_with_pin(&env, PIN).await?;
    assert!(
        matches!(ok, Response::Ack),
        "PIN unlock must work after re-seal: {:?}",
        ok
    );
    assert_vault_accessible(&env).await?;

    Ok(())
}

/// Read the dictionary-attack attempts remaining from the agent.
async fn da_remaining(env: &TpmTestEnv) -> Result<Option<u32>> {
    let res = env.client().send(Action::GetTpmDaStatus).await?;
    match res {
        Response::TpmDaStatus { status } => Ok(status.remaining),
        other => anyhow::bail!("expected TpmDaStatus, got: {:?}", other),
    }
}
