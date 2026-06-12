use crate::common::{lock_unlock_cycle, logout_login_cycle, register_user, setup_env, TestEnv};
use crate::ssh_test_utils::{assert_ssh_access, generate_ssh_keypair, start_sshd_container, wait_for_socket};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};
use std::path::PathBuf;
use testcontainers::{ContainerAsync, GenericImage};
use tokio::time::Duration;

/// Bundles everything a lifecycle test needs: the running Vaultwarden +
/// agent (`env`), a real `sshd` container authorized for the stored key
/// (`_sshd`), the agent IPC client, and the ssh-agent socket/port to drive
/// real `ssh`/`ssh-add` commands against.
struct LifecycleEnv {
    env: TestEnv,
    _sshd: ContainerAsync<GenericImage>,
    client: AgentClient,
    sock: PathBuf,
    ssh_port: u16,
}

async fn setup_with_ssh_key(email: &str, password: &str) -> Result<LifecycleEnv> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new();
    client
        .send(Action::Login {
            email: email.to_string(),
            password: password.to_string(),
            server_url: Some(env.vault_url.clone()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    let tmp_dir = tempfile::tempdir()?;
    let (private_key, public_key) = generate_ssh_keypair("ed25519", None, tmp_dir.path())?;

    let sshd = start_sshd_container(&public_key).await?;
    let ssh_port = sshd.get_host_port_ipv4(2222).await?;

    let res = client
        .send(Action::AddSshKey {
            name: "E2E Lifecycle Key".to_string(),
            private_key: private_key.into(),
            public_key: Some(public_key.clone()),
            notes: None,
            fields: Vec::new(),
        })
        .await?;
    if let Response::Error { message } = &res {
        anyhow::bail!("AddSshKey failed: {message}");
    }

    let res = client.send(Action::Sync).await?;
    if let Response::Error { message } = &res {
        anyhow::bail!("Sync failed: {message}");
    }

    let sock = cosmic_bwarden_core::dirs::ssh_agent_socket_file();
    wait_for_socket(&sock, Duration::from_secs(5)).await?;

    Ok(LifecycleEnv {
        env,
        _sshd: sshd,
        client,
        sock,
        ssh_port,
    })
}

/// While the agent is locked, the SSH agent must refuse to offer or sign
/// with the vault key: `ssh-add -l` shows no identities and a real `ssh`
/// connection is rejected outright (the test `sshd` has no other auth
/// method enabled).
#[tokio::test]
async fn test_ssh_agent_locked_refuses_signing() -> Result<()> {
    let lifecycle = setup_with_ssh_key("ssh-agent-locked@example.com", "sshagentpassword123").await?;

    assert_ssh_access(&lifecycle.sock, lifecycle.ssh_port, "testuser", true)?;

    lifecycle.client.send(Action::Lock).await?;

    assert_ssh_access(&lifecycle.sock, lifecycle.ssh_port, "testuser", false)?;

    Ok(())
}

/// SSH access must work again, with the same key, after a lock/unlock cycle.
#[tokio::test]
async fn test_ssh_agent_lock_unlock_cycle_preserves_access() -> Result<()> {
    let password = "sshagentpassword123";
    let lifecycle = setup_with_ssh_key("ssh-agent-lockcycle@example.com", password).await?;

    assert_ssh_access(&lifecycle.sock, lifecycle.ssh_port, "testuser", true)?;

    lock_unlock_cycle(&lifecycle.client, password).await?;

    assert_ssh_access(&lifecycle.sock, lifecycle.ssh_port, "testuser", true)?;

    Ok(())
}

/// SSH access must work again, with the same key, after a logout/login +
/// re-sync cycle (the key is re-fetched from the server, not just cached).
#[tokio::test]
async fn test_ssh_agent_logout_login_cycle_preserves_access() -> Result<()> {
    let email = "ssh-agent-logincycle@example.com";
    let password = "sshagentpassword123";
    let lifecycle = setup_with_ssh_key(email, password).await?;

    assert_ssh_access(&lifecycle.sock, lifecycle.ssh_port, "testuser", true)?;

    logout_login_cycle(&lifecycle.client, email, password, &lifecycle.env.vault_url).await?;
    let res = lifecycle.client.send(Action::Sync).await?;
    if let Response::Error { message } = &res {
        anyhow::bail!("Sync failed: {message}");
    }

    assert_ssh_access(&lifecycle.sock, lifecycle.ssh_port, "testuser", true)?;

    Ok(())
}
