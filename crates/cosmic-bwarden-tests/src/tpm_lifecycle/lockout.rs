//! Dictionary-attack lockout: enough wrong PINs push the TPM into lockout;
//! the correct PIN is then refused (still `ERR_TPM_UNSEAL_FAILED` — the
//! lockout is surfaced via the DA status), and the master-password path
//! still works because it never touches the TPM.

use super::*;
use crate::tpm_skip_if_unavailable;
use cosmic_bwarden_core::protocol::ERR_TPM_UNSEAL_FAILED;

#[tokio::test]
async fn test_da_lockout_refuses_correct_pin_and_password_fallback_works() -> Result<()> {
    let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);

    let email = "tpm-da-lockout@example.com";
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

    // How many wrong attempts the TPM tolerates before lockout.
    let max_tries = {
        let res = client.send(Action::GetTpmDaStatus).await?;
        match res {
            Response::TpmDaStatus { status } => status.max_tries.ok_or_else(|| {
                anyhow::anyhow!("TPM reports no max_tries — cannot drive lockout")
            })?,
            other => anyhow::bail!("expected TpmDaStatus, got: {:?}", other),
        }
    };

    // Exhaust the dictionary-attack counter. One extra attempt in case the
    // counter semantics differ; the loop stops early once lockout is reached.
    let mut in_lockout = false;
    for _ in 0..(max_tries + 2) {
        let res = unlock_with_pin(&env, WRONG_PIN).await?;
        assert!(
            matches!(res, Response::Error { .. }),
            "wrong PIN must be rejected: {:?}",
            res
        );
        let status = {
            let res = client.send(Action::GetTpmDaStatus).await?;
            match res {
                Response::TpmDaStatus { status } => status,
                other => anyhow::bail!("expected TpmDaStatus, got: {:?}", other),
            }
        };
        if status.in_lockout {
            in_lockout = true;
            break;
        }
    }
    assert!(in_lockout, "TPM did not enter DA lockout after wrong PINs");

    // The CORRECT PIN is now refused — still the unseal error (the lockout
    // details come from the DA status, which the UI fetches and displays).
    let res = unlock_with_pin(&env, PIN).await?;
    match res {
        Response::Error { message } => {
            assert_eq!(
                message, ERR_TPM_UNSEAL_FAILED,
                "lockout must map to ERR_TPM_UNSEAL_FAILED"
            );
        }
        other => anyhow::bail!(
            "expected ERR_TPM_UNSEAL_FAILED during lockout, got: {:?}",
            other
        ),
    }

    // The vault is still locked.
    assert_vault_locked(&env).await?;

    // Master-password unlock is TPM-independent and must keep working.
    let res = unlock_with_password(&env, PASSWORD).await?;
    assert!(
        matches!(res, Response::Ack),
        "master-password unlock must work during DA lockout: {:?}",
        res
    );
    assert_vault_accessible(&env).await?;

    Ok(())
}
