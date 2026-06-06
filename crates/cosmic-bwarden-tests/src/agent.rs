use anyhow::Result;
use crate::common::{setup_env, register_user};
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};
use tokio::time::{sleep, Duration};
use std::path::PathBuf;
use std::process::Command;

#[tokio::test]
async fn test_agent_login_lifecycle() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "agent-test@example.com";
    let password = "testpassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new();

    // 1. Login
    let res = client.send(Action::Login {
        email: email.to_string(),
        password: password.to_string(),
        server_url: Some(env.vault_url.clone()),
        remember_me: true,
        two_factor_token: None,
        two_factor_provider: None,
        two_factor_code: None,
        device_verification_code: None,
    }).await?;
    assert!(matches!(res, Response::Ack));

    // 2. Add entry
    client.send(Action::AddEntry {
        name: "AgentSite".to_string(),
        entry_type: cosmic_bwarden_core::protocol::EntryType::Login,
        username: Some("user".to_string()),
        password: Some("secret".to_string().into()),
        notes: None,
        fields: Vec::new(),
    }).await?;

    client.send(Action::Sync).await?;

    // 3. Verify
    let res = client.send(Action::GetEntries { query: None, entry_type: None }).await?;
    if let Response::Entries { entries } = res {
        assert!(entries.iter().any(|e| e.name == "AgentSite"));
    } else {
        anyhow::bail!("Expected entries");
    }

    Ok(())
}

#[tokio::test]
async fn test_remember_email() -> Result<()> {
    let mut env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "remember-test@example.com";
    let password = "rempassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new();

    client.send(Action::Login {
        email: email.to_string(),
        password: password.to_string(),
        server_url: Some(env.vault_url.clone()),
        remember_me: true,
        two_factor_token: None,
        two_factor_provider: None,
        two_factor_code: None,
        device_verification_code: None,
    }).await?;

    // Kill the agent
    if let Some(mut proc) = env.agent_process.take() {
        proc.kill()?;
    }
    
    // Restart the agent
    let mut agent_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    agent_path.pop();
    agent_path.pop();
    agent_path.push("target/debug/cosmic-bwarden-agent");

    let agent_process = Command::new(&agent_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .env("RUST_LOG", "debug")
        .spawn()?;

    sleep(Duration::from_millis(1000)).await;
    env.agent_process = Some(agent_process);

    let client = AgentClient::new();
    let res = client.send(Action::GetConfig).await?;
    if let Response::Config { config, .. } = res {
        assert_eq!(config.email, Some(email.to_string()));
    } else {
        anyhow::bail!("Expected Config response, got {:?}", res);
    }

    Ok(())
}
