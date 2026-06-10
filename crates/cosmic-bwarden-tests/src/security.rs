use anyhow::Result;
use crate::common::{setup_env, register_user};
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};

#[tokio::test]
async fn test_lock_unlock() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "lock-test@example.com";
    let password = "lockpassword123";

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

    client.send(Action::Lock).await?;

    let res = client.send(Action::GetEntries { query: None, entry_type: None, only_pinned: false }).await?;
    if let Response::Error { message } = res {
        assert!(message.contains("locked"));
    } else {
        anyhow::bail!("Expected locked error");
    }

    client.send(Action::Unlock { password: password.to_string() }).await?;
    let res = client.send(Action::GetEntries { query: None, entry_type: None, only_pinned: false }).await?;
    assert!(matches!(res, Response::Entries { .. }));

    Ok(())
}

#[tokio::test]
async fn test_reprompt() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "reprompt@example.com";
    let password = "reppassword123";

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

    client.send(Action::AddEntry {
        name: "Sensitive".to_string(),
        entry_type: cosmic_bwarden_core::protocol::EntryType::Login,
        username: Some("user".to_string()),
        password: Some("secret".to_string().into()),
        notes: None,
        fields: Vec::new(),
    }).await?;
    client.send(Action::Sync).await?;

    let res = client.send(Action::GetEntries { query: None, entry_type: None, only_pinned: false }).await?;
    let id = if let Response::Entries { entries } = res {
        entries[0].id.clone()
    } else {
        anyhow::bail!("Expected entries");
    };

    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    let mut entry = if let Response::Entry { entry } = res {
        entry
    } else {
        anyhow::bail!("Expected entry");
    };
    
    entry.master_password_reprompt = cosmic_bwarden_core::api::CipherRepromptType::Password;
    client.send(Action::UpdateEntry { entry: entry.clone() }).await?;
    client.send(Action::Sync).await?;

    let res = client.send(Action::GetPassword { id: entry.id.clone(), password: None }).await?;
    assert!(matches!(res, Response::Error { message } if message == "reprompt_required"));

    let res = client.send(Action::GetPassword { id: entry.id.clone(), password: Some(password.to_string()) }).await?;
    if let Response::Password { password: p } = res {
        assert_eq!(p, "secret");
    } else {
        anyhow::bail!("Expected password");
    }

    Ok(())
}

#[tokio::test]
async fn test_agent_events() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "events@example.com";
    let password = "eventpassword123";

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

    // Connect to events
    use tokio::net::UnixStream;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use cosmic_bwarden_core::protocol::{Event};

    let mut stream = UnixStream::connect(cosmic_bwarden_core::dirs::socket_file()).await?;
    let subscribe_req = Action::Subscribe;
    stream.write_all(&serde_json::to_vec(&subscribe_req)?).await?;
    
    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await?;
    let resp: Response = serde_json::from_slice(&buf[..n])?;
    assert!(matches!(resp, Response::Ack));

    // Trigger an event (VaultChanged)
    client.send(Action::Sync).await?;

    let n = stream.read(&mut buf).await?;
    let resp: Response = serde_json::from_slice(&buf[..n])?;
    if let Response::Event { event } = resp {
        assert!(matches!(event, Event::VaultChanged));
    } else {
        anyhow::bail!("Expected VaultChanged event, got {:?}", resp);
    }

    // Trigger Locked event
    client.send(Action::Lock).await?;
    let n = stream.read(&mut buf).await?;
    let resp: Response = serde_json::from_slice(&buf[..n])?;
    if let Response::Event { event } = resp {
        assert!(matches!(event, Event::Locked));
    } else {
        anyhow::bail!("Expected Locked event, got {:?}", resp);
    }

    Ok(())
}

#[tokio::test]
async fn test_token_leakage() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "leak@example.com";
    let password = "leakpassword123";

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

    // Check the DB file on disk
    let db_path = cosmic_bwarden_core::dirs::db_file(&env.vault_url, email);
    assert!(db_path.exists());

    let content = std::fs::read_to_string(db_path)?;
    assert!(!content.contains("access_token"));
    assert!(!content.contains("refresh_token"));
    
    // Ensure it's valid JSON but without tokens
    let json: serde_json::Value = serde_json::from_str(&content)?;
    assert!(json.get("access_token").is_none());
    assert!(json.get("refresh_token").is_none());

    Ok(())
}
