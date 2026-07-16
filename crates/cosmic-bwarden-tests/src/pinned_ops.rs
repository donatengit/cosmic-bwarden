use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};
use tokio::time::{sleep, Duration};

/// Verify that PinEntry followed by an immediate GetSidebarEntries
/// (without Sync in between) returns the entry as pinned. This tests
/// that the in-memory favorite update happens before the lock is released,
/// preventing stale reads in concurrent GetSidebarEntries calls.
#[tokio::test]
async fn test_pin_visible_without_sync() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "pin-visibility@example.com";
    let password = "pinvisible123";

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

    // Create one entry
    client
        .send(Action::AddEntry {
            name: "Fresh Entry".to_string(),
            entry_type: cosmic_bwarden_core::protocol::EntryType::Login,
            username: Some("freshuser".to_string()),
            password: Some("freshpass".to_string().into()),
            notes: None,
            fields: Vec::new(),
            totp: None,
            uris: Vec::new(),
        })
        .await?;
    client.send(Action::Sync).await?;

    // Get entry ID
    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
            domain: None,
        })
        .await?;
    let entries = if let Response::SidebarEntries { entries } = res {
        entries
    } else {
        anyhow::bail!("Expected sidebar entries");
    };
    assert_eq!(entries.len(), 1);
    assert!(!entries[0].is_pinned);
    let id = entries[0].id.clone();

    // Pin the entry
    client.send(Action::PinEntry { id: id.clone() }).await?;

    // Immediately check pinned-only — must see it, no Sync between
    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: true,
            domain: None,
        })
        .await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(
            entries.len(),
            1,
            "Pinned entry must be visible in only_pinned without Sync"
        );
        assert!(entries[0].is_pinned);
        assert_eq!(entries[0].id, id);
    } else {
        anyhow::bail!("Expected sidebar entries, got {:?}", res);
    }

    Ok(())
}

#[tokio::test]
async fn test_pinning_lifecycle() -> Result<()> {
    let mut env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "pinning@example.com";
    let password = "pinpassword123";

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

    // 1. Create 3 entries
    for i in 1..=3 {
        client
            .send(Action::AddEntry {
                name: format!("Entry {}", i),
                entry_type: cosmic_bwarden_core::protocol::EntryType::Login,
                username: Some(format!("user{}", i)),
                password: Some(format!("pass{}", i).into()),
                notes: None,
                fields: Vec::new(),
                totp: None,
                uris: Vec::new(),
            })
            .await?;
    }
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
            domain: None,
        })
        .await?;
    let entries = if let Response::SidebarEntries { entries } = res {
        entries
    } else {
        anyhow::bail!("Expected sidebar entries");
    };
    assert_eq!(entries.len(), 3);
    for e in &entries {
        assert!(!e.is_pinned);
    }

    let id1 = entries
        .iter()
        .find(|e| e.name == "Entry 1")
        .unwrap()
        .id
        .clone();
    let id2 = entries
        .iter()
        .find(|e| e.name == "Entry 2")
        .unwrap()
        .id
        .clone();
    let id3 = entries
        .iter()
        .find(|e| e.name == "Entry 3")
        .unwrap()
        .id
        .clone();

    // 2. Pin entries (this toggles native 'favorite' on server)
    client.send(Action::PinEntry { id: id1.clone() }).await?;
    client.send(Action::PinEntry { id: id2.clone() }).await?;
    client.send(Action::PinEntry { id: id3.clone() }).await?;

    // Sync to ensure we get the updated state from server
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
            domain: None,
        })
        .await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 3);
        for e in &entries {
            assert!(e.is_pinned, "Entry {} should be pinned", e.name);
        }
    }

    // 3. Verify pinned sidebar entries
    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: true,
            domain: None,
        })
        .await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().any(|e| e.id == id1));
        assert!(entries.iter().any(|e| e.id == id2));
        assert!(entries.iter().any(|e| e.id == id3));
    } else {
        anyhow::bail!("Expected pinned sidebar entries");
    }

    // 4. Persistence across restart
    if let Some(mut child) = env.agent_process.take() {
        child.kill()?;
    }

    env.agent_process = Some(env.start_agent()?);
    sleep(Duration::from_millis(1000)).await;

    // Must login again
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

    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: true,
            domain: None,
        })
        .await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(
            entries.len(),
            3,
            "Pinned status should be persisted on server"
        );
    } else {
        anyhow::bail!("Expected pinned sidebar entries after restart");
    }

    // 5. Unpinning
    client.send(Action::UnpinEntry { id: id2.clone() }).await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: true,
            domain: None,
        })
        .await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 2);
        assert!(!entries.iter().any(|e| e.id == id2));
    }

    // 6. Deletion cleanup
    client.send(Action::DeleteEntry { id: id3.clone() }).await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: true,
            domain: None,
        })
        .await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, id1);
    }

    Ok(())
}

/// After agent restart, `access_token`/`refresh_token` are lost
/// (`#[serde(skip)]`). Unlock performs a silent re-auth with the freshly
/// derived master-password hash, so server operations (PinEntry) must work
/// again immediately after restart + unlock, and the change must survive a
/// forced Sync.
#[tokio::test]
async fn test_pin_after_restart_unlock_silent_reauth() -> Result<()> {
    let mut env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "restart-pin@example.com";
    let password = "restartpinpass123";

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

    // Create one entry to pin
    client
        .send(Action::AddEntry {
            name: "Target Entry".to_string(),
            entry_type: cosmic_bwarden_core::protocol::EntryType::Login,
            username: Some("targetuser".to_string()),
            password: Some("targetpass".to_string().into()),
            notes: None,
            fields: Vec::new(),
            totp: None,
            uris: Vec::new(),
        })
        .await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
            domain: None,
        })
        .await?;
    let entries = if let Response::SidebarEntries { entries } = res {
        entries
    } else {
        anyhow::bail!("Expected sidebar entries");
    };
    assert_eq!(entries.len(), 1);
    let target_id = entries[0].id.clone();

    // Kill and restart the agent -- tokens are #[serde(skip)] and lost.
    // Without the keyring feature, unlock cannot recover them.
    if let Some(mut child) = env.agent_process.take() {
        child.kill()?;
    }
    env.agent_process = Some(env.start_agent()?);
    sleep(Duration::from_millis(1000)).await;

    // Unlock (NOT re-login) -- vault content is on disk and decryptable
    let res = client
        .send(Action::Unlock {
            password: password.to_string(),
        })
        .await?;
    assert!(
        matches!(res, Response::Ack),
        "Expected Ack after unlock, got {:?}",
        res
    );

    // Unlock silently re-authenticated, so a server operation (PinEntry)
    // must succeed without an explicit re-login.
    let res = client
        .send(Action::PinEntry {
            id: target_id.clone(),
        })
        .await?;
    assert!(
        matches!(res, Response::Ack),
        "Expected Ack for pin after restart+unlock (silent re-auth), got {:?}",
        res
    );

    // The favorite must have reached the server: a forced Sync replaces the
    // local DB with server state, so the pin only survives if the server
    // accepted it.
    let res = client.send(Action::Sync).await?;
    assert!(
        matches!(res, Response::Ack),
        "Expected Ack for sync, got {:?}",
        res
    );

    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: true,
            domain: None,
        })
        .await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 1, "Pin must survive forced Sync");
        assert_eq!(entries[0].id, target_id);
    } else {
        anyhow::bail!("Expected sidebar entries after sync, got {:?}", res);
    }

    Ok(())
}
