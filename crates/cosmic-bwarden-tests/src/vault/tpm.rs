//! TPM2 PIN-unlock smoke tests using swtpm (software TPM emulator).
//!
//! These tests are skipped automatically when:
//!   - `swtpm` binary is not found in PATH
//!   - The agent binary does not have TPM support (CheckTpm returns false)
//!
//! To run:
//!   cargo test -p cosmic-bwarden-tests --features tpm-smoke -- tpm --test-threads=1
//!
//! The `TSS2_TCTI` environment variable is set to point at the swtpm socket
//! so the agent uses the emulated TPM instead of real hardware.

use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};
use std::path::PathBuf;
use std::process::{Child, Command};
use tokio::time::{sleep, Duration};

const EMAIL: &str = "tpm-test@example.com";
const PASSWORD: &str = "tpm-master-password-123";
const PIN: &str = "1234";

struct SwtpmGuard {
    swtpm: Child,
    _state_dir: tempfile::TempDir,
}

impl Drop for SwtpmGuard {
    fn drop(&mut self) {
        let _ = self.swtpm.kill();
        let _ = self.swtpm.wait();
    }
}

/// Start a software TPM emulator on a unix socket.
/// Returns the guard (keeps the process alive) and the socket path.
/// Returns `None` if swtpm is not available.
fn start_swtpm() -> Option<(SwtpmGuard, PathBuf)> {
    // Check swtpm exists in PATH
    if Command::new("swtpm").arg("--version").output().is_err() {
        eprintln!("[tpm-smoke] swtpm not found — skipping TPM tests");
        return None;
    }

    let state_dir = tempfile::TempDir::new().ok()?;
    let socket_path = state_dir.path().join("swtpm.sock");

    // Initialize TPM state
    let init_status = Command::new("swtpm_setup")
        .args([
            "--tpmstate",
            state_dir.path().to_str()?,
            "--tpm2",
            "--createek",
        ])
        .status()
        .ok()?;

    if !init_status.success() {
        eprintln!("[tpm-smoke] swtpm_setup failed — skipping TPM tests");
        return None;
    }

    let swtpm = Command::new("swtpm")
        .args([
            "socket",
            "--tpmstate",
            &format!("dir={}", state_dir.path().display()),
            "--ctrl",
            &format!("type=unixio,path={}", socket_path.display()),
            "--tpm2",
            "--server",
            &format!(
                "type=unixio,path={}",
                socket_path.with_extension("server").display()
            ),
            "--flags",
            "not-need-init,startup-clear",
            "--log",
            "level=0",
        ])
        .spawn()
        .ok()?;

    Some((
        SwtpmGuard {
            swtpm,
            _state_dir: state_dir,
        },
        socket_path,
    ))
}

/// Returns the TCTI string for swtpm given the server socket path.
fn swtpm_tcti(socket_path: &PathBuf) -> String {
    format!(
        "swtpm:path={}",
        socket_path.with_extension("server").display()
    )
}

#[tokio::test]
async fn test_tpm_availability_check() -> Result<()> {
    let Some((guard, socket_path)) = start_swtpm() else {
        return Ok(()); // swtpm not available — skip
    };
    sleep(Duration::from_millis(300)).await; // let swtpm start

    let env = setup_env().await?;
    std::env::set_var("TSS2_TCTI", swtpm_tcti(&socket_path));

    let client = AgentClient::new_with_socket(env.socket_path.clone());

    let res = client.send(Action::CheckTpm).await?;
    drop(guard);

    match res {
        Response::TpmStatus {
            available,
            configured: _,
            server_credentials: _,
        } => {
            // With swtpm running, TPM should be available if agent has TPM support.
            // If agent lacks --features tpm, available == false is expected.
            eprintln!("[tpm-smoke] CheckTpm: available={}", available);
        }
        other => anyhow::bail!("expected TpmStatus, got {:?}", other),
    }

    Ok(())
}

#[tokio::test]
async fn test_tpm_pin_setup_and_unlock() -> Result<()> {
    let Some((guard, socket_path)) = start_swtpm() else {
        return Ok(());
    };
    sleep(Duration::from_millis(300)).await;

    let env = setup_env().await?;
    let tcti = swtpm_tcti(&socket_path);
    std::env::set_var("TSS2_TCTI", &tcti);

    register_user(&env.vault_url, EMAIL, PASSWORD).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    let res = client
        .send(Action::Login {
            email: EMAIL.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url.clone()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;
    anyhow::ensure!(matches!(res, Response::Ack), "login failed: {:?}", res);

    // Check TPM available; skip if not compiled in
    let tpm_res = client.send(Action::CheckTpm).await?;
    let available = match tpm_res {
        Response::TpmStatus { available, .. } => available,
        other => anyhow::bail!("unexpected CheckTpm response: {:?}", other),
    };
    if !available {
        eprintln!("[tpm-smoke] TPM not available (agent may lack --features tpm) — skipping");
        drop(guard);
        return Ok(());
    }

    // Set up PIN
    let setup_res = client
        .send(Action::SetupTpmPin {
            master_password: PASSWORD.to_string(),
            pin: PIN.to_string(),
        })
        .await?;
    anyhow::ensure!(
        matches!(setup_res, Response::Ack),
        "SetupTpmPin failed: {:?}",
        setup_res
    );

    // Lock vault
    client.send(Action::Lock).await?;

    // Unlock with PIN
    let unlock_res = client
        .send(Action::UnlockWithPin {
            pin: PIN.to_string(),
        })
        .await?;
    anyhow::ensure!(
        matches!(unlock_res, Response::Ack),
        "UnlockWithPin failed: {:?}",
        unlock_res
    );

    // Vault should now be accessible
    let entries_res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    anyhow::ensure!(
        matches!(entries_res, Response::Entries { .. }),
        "GetEntries failed after PIN unlock: {:?}",
        entries_res
    );

    drop(guard);
    Ok(())
}

#[tokio::test]
async fn test_tpm_wrong_pin_rejected() -> Result<()> {
    let Some((guard, socket_path)) = start_swtpm() else {
        return Ok(());
    };
    sleep(Duration::from_millis(300)).await;

    let env = setup_env().await?;
    std::env::set_var("TSS2_TCTI", swtpm_tcti(&socket_path));

    let email = "tpm-wrong-pin@example.com";
    register_user(&env.vault_url, email, PASSWORD).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url.clone()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    let tpm_res = client.send(Action::CheckTpm).await?;
    let available = match tpm_res {
        Response::TpmStatus { available, .. } => available,
        _ => false,
    };
    if !available {
        drop(guard);
        return Ok(());
    }

    client
        .send(Action::SetupTpmPin {
            master_password: PASSWORD.to_string(),
            pin: PIN.to_string(),
        })
        .await?;

    client.send(Action::Lock).await?;

    // Wrong PIN
    let res = client
        .send(Action::UnlockWithPin {
            pin: "9999".to_string(),
        })
        .await?;

    assert!(
        matches!(res, Response::Error { .. }),
        "Expected error for wrong PIN, got {:?}",
        res
    );

    // Vault should remain locked
    let entries_res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    assert!(
        matches!(entries_res, Response::Error { .. }),
        "Vault should still be locked after wrong PIN"
    );

    drop(guard);
    Ok(())
}

#[tokio::test]
async fn test_tpm_disable_requires_master_password() -> Result<()> {
    let Some((guard, socket_path)) = start_swtpm() else {
        return Ok(());
    };
    sleep(Duration::from_millis(300)).await;

    let env = setup_env().await?;
    std::env::set_var("TSS2_TCTI", swtpm_tcti(&socket_path));

    let email = "tpm-disable@example.com";
    register_user(&env.vault_url, email, PASSWORD).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url.clone()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    let tpm_res = client.send(Action::CheckTpm).await?;
    let available = match tpm_res {
        Response::TpmStatus { available, .. } => available,
        _ => false,
    };
    if !available {
        drop(guard);
        return Ok(());
    }

    client
        .send(Action::SetupTpmPin {
            master_password: PASSWORD.to_string(),
            pin: PIN.to_string(),
        })
        .await?;

    // Vault is unlocked — disable requires no master password.
    let ok_res = client.send(Action::DisableTpmPin).await?;
    assert!(
        matches!(ok_res, Response::Ack),
        "DisableTpmPin should succeed while vault is unlocked: {:?}",
        ok_res
    );

    // TPM should now be unconfigured
    let status = client.send(Action::CheckTpm).await?;
    match status {
        Response::TpmStatus { configured, .. } => {
            assert!(!configured, "TPM should be unconfigured after disable");
        }
        other => anyhow::bail!("unexpected CheckTpm response: {:?}", other),
    }

    drop(guard);
    Ok(())
}

#[tokio::test]
async fn test_tpm_fallback_on_unavailable() -> Result<()> {
    // Test that normal password unlock still works even when TPM is not available.
    let env = setup_env().await?;

    // Deliberately do NOT set TSS2_TCTI — no TPM available.
    // Unset it in case a previous test set it.
    std::env::remove_var("TSS2_TCTI");

    let email = "tpm-fallback@example.com";
    register_user(&env.vault_url, email, PASSWORD).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    client
        .send(Action::Login {
            email: email.to_string(),
            password: PASSWORD.to_string(),
            server_url: Some(env.vault_url.clone()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    // Lock and unlock with password — should always work regardless of TPM.
    client.send(Action::Lock).await?;

    let unlock_res = client
        .send(Action::Unlock {
            password: PASSWORD.to_string(),
        })
        .await?;
    assert!(
        matches!(unlock_res, Response::Ack),
        "Password unlock should succeed even without TPM: {:?}",
        unlock_res
    );

    let entries_res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    assert!(
        matches!(entries_res, Response::Entries { .. }),
        "Vault should be accessible after password unlock: {:?}",
        entries_res
    );

    Ok(())
}
