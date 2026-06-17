use anyhow::Result;
use crate::common::{setup_env, register_user};
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_agent_login_lifecycle() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "agent-test@example.com";
    let password = "testpassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());

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
    let res = client.send(Action::GetEntries { query: None, entry_type: None, only_pinned: false }).await?;
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

    let client = AgentClient::new_with_socket(env.socket_path.clone());

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
    let agent_process = env.start_agent()?;

    sleep(Duration::from_millis(1000)).await;
    env.agent_process = Some(agent_process);

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    let res = client.send(Action::GetConfig).await?;
    if let Response::Config { config, .. } = res {
        assert_eq!(config.email, Some(email.to_string()));
    } else {
        anyhow::bail!("Expected Config response, got {:?}", res);
    }

    Ok(())
}

#[tokio::test]
async fn test_unlock_after_restart_is_not_routed_back_to_login() -> Result<()> {
    let mut env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "restart-unlock@example.com";
    let password = "restartpassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
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

    // Kill and restart the agent, simulating a fresh process with no in-memory
    // access/refresh tokens (the default build has no keyring backend, so tokens
    // are never persisted across restarts).
    if let Some(mut proc) = env.agent_process.take() {
        proc.kill()?;
    }

    let agent_process = env.start_agent()?;

    sleep(Duration::from_millis(1000)).await;
    env.agent_process = Some(agent_process);

    let client = AgentClient::new_with_socket(env.socket_path.clone());

    // Fresh process: locked, but the account/vault data is on disk.
    let res = client.send(Action::GetConfig).await?;
    if let Response::Config { has_account, is_locked, .. } = res {
        assert!(has_account, "expected has_account to be true after restart");
        assert!(is_locked, "expected is_locked to be true after restart");
    } else {
        anyhow::bail!("Expected Config response, got {:?}", res);
    }

    // Unlocking with the correct password must succeed...
    let res = client.send(Action::Unlock { password: password.to_string() }).await?;
    assert!(matches!(res, Response::Ack), "Expected Ack, got {:?}", res);

    // ...and the resulting config must report unlocked + has_account, even though
    // needs_login may still be true (no access/refresh tokens in this build).
    let res = client.send(Action::GetConfig).await?;
    if let Response::Config { has_account, is_locked, .. } = res {
        assert!(has_account, "expected has_account to remain true after unlock");
        assert!(!is_locked, "expected is_locked to be false after a successful unlock");
    } else {
        anyhow::bail!("Expected Config response, got {:?}", res);
    }

    Ok(())
}
