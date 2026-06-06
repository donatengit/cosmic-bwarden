use anyhow::Result;
use crate::common::{setup_env, register_user};
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};
use tokio::time::{sleep, Duration};
use std::process::Command;
use std::path::PathBuf;

#[tokio::test]
async fn test_pinning_lifecycle() -> Result<()> {
    let mut env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "pinning@example.com";
    let password = "pinpassword123";

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

    let res = client.send(Action::GetSidebarEntries { query: None, entry_type: None }).await?;
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

    // 2. Pin entries
    client.send(Action::PinEntry { id: id1.clone() }).await?;
    client.send(Action::PinEntry { id: id2.clone() }).await?;
    client.send(Action::PinEntry { id: id3.clone() }).await?;
    client.send(Action::Sync).await?;

    let res = client.send(Action::GetSidebarEntries { query: None, entry_type: None }).await?;
    if let Response::SidebarEntries { entries } = res {
        for e in &entries {
            assert!(e.is_pinned);
        }
    }

    // 3. Usage tracking and sorting
    // Entry 2: 3 copies
    // Entry 3: 1 copy
    // Entry 1: 0 copies
    for _ in 0..3 {
        client.send(Action::RecordCopy { id: id2.clone() }).await?;
    }
    client.send(Action::RecordCopy { id: id3.clone() }).await?;

    let res = client.send(Action::GetTopFrequent { limit: 5, days: None }).await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].id, id2);
        assert_eq!(entries[1].id, id3);
        assert_eq!(entries[2].id, id1);
    } else {
        anyhow::bail!("Expected top frequent entries");
    }

    // 4. Persistence across restart
    if let Some(mut child) = env.agent_process.take() {
        child.kill()?;
    }
    
    let mut agent_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    agent_path.pop();
    agent_path.pop();
    agent_path.push("target/debug/cosmic-bwarden-agent");

    env.agent_process = Some(Command::new(&agent_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .env("RUST_LOG", "debug")
        .spawn()?);
    sleep(Duration::from_millis(1000)).await;

    // Must login again to have a session for unpinning
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
        assert_eq!(entries.len(), 3, "State should be persisted");
        assert_eq!(entries[0].id, id2);
    } else {
        anyhow::bail!("Expected top frequent entries after restart");
    }

    // 5. Unpinning
    client.send(Action::UnpinEntry { id: id2.clone() }).await?;
    let res = client.send(Action::GetTopFrequent { limit: 5, days: None }).await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 2);
        assert!(!entries.iter().any(|e| e.id == id2));
        assert_eq!(entries[0].id, id3); // id3 has 1 copy, id1 has 0
    }

    // 6. Deletion cleanup
    client.send(Action::DeleteEntry { id: id3.clone() }).await?;
    let res = client.send(Action::GetTopFrequent { limit: 5, days: None }).await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, id1);
    }

    // 7. Search tests
    // Create some more entries for search
    client.send(Action::AddEntry {
        name: "Searchable Login".to_string(),
        entry_type: cosmic_bwarden_core::protocol::EntryType::Login,
        username: Some("searchuser".to_string()),
        password: Some("searchpass".to_string().into()),
        notes: None,
        fields: Vec::new(),
    }).await?;
    client.send(Action::Sync).await?;

    // Search by name
    let res = client.send(Action::GetSidebarEntries { query: Some("Searchable".to_string()), entry_type: None }).await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Searchable Login");
    } else {
        anyhow::bail!("Expected sidebar entries for search");
    }

    // Search by username
    let res = client.send(Action::GetSidebarEntries { query: Some("searchuser".to_string()), entry_type: None }).await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 1);
    } else {
        anyhow::bail!("Expected sidebar entries for username search");
    }

    // Search with entry type filter
    let res = client.send(Action::GetSidebarEntries { query: None, entry_type: Some(cosmic_bwarden_core::protocol::EntryType::Login) }).await?;
    if let Response::SidebarEntries { entries } = res {
        // Entry 1 and Searchable Login are Logins
        assert!(entries.len() >= 2);
    }

    Ok(())
}
