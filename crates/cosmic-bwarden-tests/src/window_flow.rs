use anyhow::Result;
use crate::common::{setup_env, register_user};
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};

#[tokio::test]
async fn test_detached_auth_flow_simulation() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "window-flow@example.com";
    let password = "windowpassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new();
    
    // 1. Simulate applet start (Agent is locked)
    let res = client.send(Action::GetConfig).await?;
    if let Response::Config { needs_login, .. } = res {
        assert!(needs_login);
    } else {
        anyhow::bail!("Expected config");
    }

    // 2. Simulate Login (as would happen in the detached Auth window)
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

    // 3. Verify unlocked state
    let res = client.send(Action::GetConfig).await?;
    if let Response::Config { needs_login, .. } = res {
        assert!(!needs_login);
    } else {
        anyhow::bail!("Expected config");
    }

    // 4. Simulate Lock (as from applet menu)
    client.send(Action::Lock).await?;
    
    // 5. Verify needs unlock but NOT login
    let res = client.send(Action::GetConfig).await?;
    if let Response::Config { needs_login, is_locked, .. } = res {
        assert!(is_locked);
        assert!(!needs_login);
    } else {
        anyhow::bail!("Expected config");
    }

    Ok(())
}
