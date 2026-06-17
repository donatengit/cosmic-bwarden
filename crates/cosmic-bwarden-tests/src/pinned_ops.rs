use anyhow::Result;
use crate::common::{setup_env, register_user};
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_pinning_lifecycle() -> Result<()> {
    let mut env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "pinning@example.com";
    let password = "pinpassword123";

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

    // 1. Create 3 entries
    for i in 1..=3 {
        client.send(Action::AddEntry {
            name: format!("Entry {}", i),
            entry_type: cosmic_bwarden_core::protocol::EntryType::Login,
            username: Some(format!("user{}", i)),
            password: Some(format!("pass{}", i).into()),
            notes: None,
            fields: Vec::new(),
        }).await?;
    }
    client.send(Action::Sync).await?;

    let res = client.send(Action::GetSidebarEntries { query: None, entry_type: None, only_pinned: false }).await?;
    let entries = if let Response::SidebarEntries { entries } = res {
        entries
    } else {
        anyhow::bail!("Expected sidebar entries");
    };
    assert_eq!(entries.len(), 3);
    for e in &entries {
        assert!(!e.is_pinned);
    }

    let id1 = entries.iter().find(|e| e.name == "Entry 1").unwrap().id.clone();
    let id2 = entries.iter().find(|e| e.name == "Entry 2").unwrap().id.clone();
    let id3 = entries.iter().find(|e| e.name == "Entry 3").unwrap().id.clone();

    // 2. Pin entries (this toggles native 'favorite' on server)
    client.send(Action::PinEntry { id: id1.clone() }).await?;
    client.send(Action::PinEntry { id: id2.clone() }).await?;
    client.send(Action::PinEntry { id: id3.clone() }).await?;
    
    // Sync to ensure we get the updated state from server
    client.send(Action::Sync).await?;

    let res = client.send(Action::GetSidebarEntries { query: None, entry_type: None, only_pinned: false }).await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 3);
        for e in &entries {
            assert!(e.is_pinned, "Entry {} should be pinned", e.name);
        }
    }

    // 3. Verify Top Frequent (now returns pinned entries)
    let res = client.send(Action::GetTopFrequent { limit: 5, days: None }).await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().any(|e| e.id == id1));
        assert!(entries.iter().any(|e| e.id == id2));
        assert!(entries.iter().any(|e| e.id == id3));
    } else {
        anyhow::bail!("Expected top frequent entries");
    }

    // 4. Persistence across restart
    if let Some(mut child) = env.agent_process.take() {
        child.kill()?;
    }
    
    env.agent_process = Some(env.start_agent()?);
    sleep(Duration::from_millis(1000)).await;

    // Must login again
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

    let res = client.send(Action::GetTopFrequent { limit: 5, days: None }).await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 3, "Pinned status should be persisted on server");
    } else {
        anyhow::bail!("Expected top frequent entries after restart");
    }

    // 5. Unpinning
    client.send(Action::UnpinEntry { id: id2.clone() }).await?;
    client.send(Action::Sync).await?;
    
    let res = client.send(Action::GetTopFrequent { limit: 5, days: None }).await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 2);
        assert!(!entries.iter().any(|e| e.id == id2));
    }

    // 6. Deletion cleanup
    client.send(Action::DeleteEntry { id: id3.clone() }).await?;
    client.send(Action::Sync).await?;
    
    let res = client.send(Action::GetTopFrequent { limit: 5, days: None }).await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, id1);
    }

    Ok(())
}
