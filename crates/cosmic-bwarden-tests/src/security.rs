use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};

#[tokio::test]
async fn test_lock_unlock() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "lock-test@example.com";
    let password = "lockpassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
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

    client.send(Action::Lock).await?;

    let res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    if let Response::Error { message } = res {
        assert!(message.contains("locked"));
    } else {
        anyhow::bail!("Expected locked error");
    }

    client
        .send(Action::Unlock {
            password: password.to_string(),
        })
        .await?;
    let res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
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

    let client = AgentClient::new_with_socket(env.socket_path.clone());
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

    client
        .send(Action::AddEntry {
            name: "Sensitive".to_string(),
            entry_type: cosmic_bwarden_core::protocol::EntryType::Login,
            username: Some("user".to_string()),
            password: Some("secret".to_string().into()),
            notes: None,
            fields: Vec::new(),
        })
        .await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let id = if let Response::Entries { entries } = res {
        // Bulk read must never carry secrets: the login password is redacted here
        // and only retrievable via GetPassword (which enforces reprompt).
        if let cosmic_bwarden_core::db::EntryData::Login { password, .. } = &entries[0].data {
            assert!(password.is_none(), "GetEntries leaked a login password");
        }
        entries[0].id.clone()
    } else {
        anyhow::bail!("Expected entries");
    };

    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    let mut entry = if let Response::Entry { entry } = res {
        entry
    } else {
        anyhow::bail!("Expected entry");
    };

    entry.master_password_reprompt = cosmic_bwarden_core::api::CipherRepromptType::Password;
    client
        .send(Action::UpdateEntry {
            entry: entry.clone(),
        })
        .await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetPassword {
            id: entry.id.clone(),
            password: None,
        })
        .await?;
    assert!(matches!(res, Response::Error { message } if message == "reprompt_required"));

    let res = client
        .send(Action::GetPassword {
            id: entry.id.clone(),
            password: Some(password.to_string()),
        })
        .await?;
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

    let client = AgentClient::new_with_socket(env.socket_path.clone());
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

    // Connect to events
    use cosmic_bwarden_core::protocol::Event;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    let mut stream = UnixStream::connect(&env.socket_path).await?;
    let subscribe_req = Action::Subscribe;
    let req_bytes = postcard::to_allocvec(&subscribe_req)?;
    let len = req_bytes.len() as u32;
    stream.write_all(&len.to_le_bytes()).await?;
    stream.write_all(&req_bytes).await?;

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    let resp: Response = postcard::from_bytes(&buf)?;
    assert!(matches!(resp, Response::Ack));

    // Trigger an event (VaultChanged)
    client.send(Action::Sync).await?;

    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    let resp: Response = postcard::from_bytes(&buf)?;
    if let Response::Event { event } = resp {
        assert!(matches!(event, Event::VaultChanged));
    } else {
        anyhow::bail!("Expected VaultChanged event, got {:?}", resp);
    }

    // Trigger Locked event
    client.send(Action::Lock).await?;
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    let resp: Response = postcard::from_bytes(&buf)?;
    if let Response::Event { event } = resp {
        assert!(matches!(event, Event::Locked));
    } else {
        anyhow::bail!("Expected Locked event, got {:?}", resp);
    }

    // An ssh-agent request while locked should broadcast UnlockRequested.
    use crate::ssh_test_utils::{run_ssh_add_list, wait_for_socket};
    use std::time::Duration;

    let ssh_sock = &env.ssh_socket_path;
    wait_for_socket(&ssh_sock, Duration::from_secs(5)).await?;

    let _ = run_ssh_add_list(&ssh_sock)?;

    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    let resp: Response = postcard::from_bytes(&buf)?;
    if let Response::Event { event } = resp {
        assert!(matches!(event, Event::UnlockRequested));
    } else {
        anyhow::bail!("Expected UnlockRequested event, got {:?}", resp);
    }

    // A second ssh-agent request while still locked should be debounced:
    // no second UnlockRequested for this lock period.
    let _ = run_ssh_add_list(&ssh_sock)?;

    let mut len_buf2 = [0u8; 4];
    let res = tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut len_buf2)).await;
    assert!(
        res.is_err(),
        "expected no second UnlockRequested event (debounce)"
    );

    Ok(())
}

#[tokio::test]
async fn test_token_leakage() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "leak@example.com";
    let password = "leakpassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
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

    // Force sync to ensure DB is written to disk
    client.send(Action::Sync).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Find the DB file in the isolated cache directory (search recursively)
    let mut db_path = None;
    for _ in 0..10 {
        for entry in walkdir::WalkDir::new(&env.cache_home) {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                    db_path = Some(path.to_path_buf());
                    break;
                }
            }
        }
        if db_path.is_some() {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    let db_path = db_path.expect("No DB file found in cache after login and sync");
    let content = std::fs::read_to_string(db_path)?;
    assert!(!content.contains("access_token"));
    assert!(!content.contains("refresh_token"));

    // Ensure it's valid JSON but without tokens
    let json: serde_json::Value = serde_json::from_str(&content)?;
    assert!(json.get("access_token").is_none());
    assert!(json.get("refresh_token").is_none());

    Ok(())
}
