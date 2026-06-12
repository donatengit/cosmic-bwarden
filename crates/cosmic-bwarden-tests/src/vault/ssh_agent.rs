use crate::common::{register_user, setup_env};
use crate::ssh_test_utils::{assert_ssh_access, generate_ssh_keypair, start_sshd_container, wait_for_socket};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};
use tokio::time::Duration;

/// Full real-protocol round trip: stores a freshly generated SSH keypair in
/// the vault, then proves the agent's `ssh-agent-socket` serves it
/// (`request_identities`) and can sign a real OpenSSH handshake with it
/// (`sign`) against a real `sshd` container.
async fn run_signing_test(key_type: &str, bits: Option<u32>, email: &str, password: &str) -> Result<()> {
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
    let (private_key, public_key) = generate_ssh_keypair(key_type, bits, tmp_dir.path())?;

    let sshd = start_sshd_container(&public_key).await?;
    let ssh_port = sshd.get_host_port_ipv4(2222).await?;

    let res = client
        .send(Action::AddSshKey {
            name: format!("E2E {key_type} Key"),
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

    assert_ssh_access(&sock, ssh_port, "testuser", true)?;

    Ok(())
}

#[tokio::test]
async fn test_ssh_agent_signs_real_ssh_connection_ed25519() -> Result<()> {
    run_signing_test("ed25519", None, "ssh-agent-ed25519@example.com", "sshagentpassword123").await
}

#[tokio::test]
async fn test_ssh_agent_signs_real_ssh_connection_rsa() -> Result<()> {
    run_signing_test("rsa", Some(2048), "ssh-agent-rsa@example.com", "sshagentpassword123").await
}
