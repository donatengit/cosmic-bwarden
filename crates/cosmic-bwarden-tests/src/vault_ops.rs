use anyhow::Result;
use crate::common::{setup_env, register_user};
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};

#[tokio::test]
async fn test_note_crud_lifecycle() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "note-crud@example.com";
    let password = "notepassword123";

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

    // 1. Create Note
    client.send(Action::AddEntry {
        name: "My Secret Note".to_string(),
        entry_type: cosmic_bwarden_core::protocol::EntryType::Login,
        username: None,
        password: None,
        notes: Some("Initial content".into()),
        fields: Vec::new(),
    }).await?;

    client.send(Action::Sync).await?;

    // 2. Read & Verify
    let res = client.send(Action::GetEntries { query: None, entry_type: None }).await?;
    let id = if let Response::Entries { entries } = res {
        let entry = entries.iter().find(|e| e.name == "My Secret Note").expect("Note not found");
        entry.id.clone()
    } else {
        anyhow::bail!("Expected entries");
    };

    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    let mut entry = if let Response::Entry { entry } = res {
        assert_eq!(entry.notes, Some(cosmic_bwarden_core::db::Secret::from("Initial content".to_string())));
        entry
    } else {
        anyhow::bail!("Expected full entry");
    };

    // 3. Update Note (This is the critical fix verification)
    entry.notes = Some("Updated content".into());
    let res = client.send(Action::UpdateEntry { entry }).await?;
    if let Response::Error { message } = &res {
        anyhow::bail!("UpdateEntry failed: {}", message);
    }
    assert!(matches!(res, Response::Ack));

    client.send(Action::Sync).await?;

    // 4. Verify Update
    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    if let Response::Entry { entry } = res {
        assert_eq!(entry.notes, Some(cosmic_bwarden_core::db::Secret::from("Updated content".to_string())));
    } else {
        anyhow::bail!("Expected full entry after update");
    }

    // 5. Delete Note
    client.send(Action::DeleteEntry { id: id.clone() }).await?;
    client.send(Action::Sync).await?;

    // 6. Verify Deletion
    let res = client.send(Action::GetEntries { query: None, entry_type: None }).await?;
    if let Response::Entries { entries } = res {
        assert!(!entries.iter().any(|e| e.id == id));
    }

    Ok(())
}

#[tokio::test]
async fn test_login_crud_lifecycle() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "login-crud@example.com";
    let password = "loginpassword123";

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

    // 1. Create Login
    client.send(Action::AddEntry {
        name: "LoginSite".to_string(),
        entry_type: cosmic_bwarden_core::protocol::EntryType::Login,
        username: Some("user123".to_string()),
        password: Some("pass123".to_string().into()),
        notes: None,
        fields: Vec::new(),
    }).await?;

    client.send(Action::Sync).await?;

    // 2. Verify
    let res = client.send(Action::GetEntries { query: None, entry_type: None }).await?;
    let id = if let Response::Entries { entries } = res {
        entries[0].id.clone()
    } else {
        anyhow::bail!("Expected entries");
    };

    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    let mut entry = if let Response::Entry { entry } = res {
        if let cosmic_bwarden_core::db::EntryData::Login { username, password, .. } = &entry.data {
            assert_eq!(username.as_deref(), Some("user123"));
            assert_eq!(password.as_deref(), Some("pass123"));
        } else {
            anyhow::bail!("Expected Login data");
        }
        entry
    } else {
        anyhow::bail!("Expected entry");
    };

    // 3. Update Login
    entry.name = "UpdatedSite".to_string();
    if let cosmic_bwarden_core::db::EntryData::Login { ref mut username, .. } = entry.data {
        *username = Some("newuser".into());
    }
    let res = client.send(Action::UpdateEntry { entry }).await?;
    assert!(matches!(res, Response::Ack));

    client.send(Action::Sync).await?;

    // 4. Verify Update
    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    if let Response::Entry { entry } = res {
        assert_eq!(entry.name, "UpdatedSite");
        if let cosmic_bwarden_core::db::EntryData::Login { username, .. } = &entry.data {
            assert_eq!(username.as_deref(), Some("newuser"));
        }
    }

    // 5. Delete
    client.send(Action::DeleteEntry { id }).await?;
    client.send(Action::Sync).await?;

    let res = client.send(Action::GetEntries { query: None, entry_type: None }).await?;
    if let Response::Entries { entries } = res {
        assert!(entries.is_empty());
    }

    Ok(())
}

#[tokio::test]
async fn test_ssh_key_crud_lifecycle() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "ssh-crud@example.com";
    let password = "sshpassword123";

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

    // 1. Create SSH Key
    let res = client.send(Action::AddSshKey {
        name: "My Work Key".to_string(),
        private_key: "PRIVATE KEY CONTENT".to_string().into(),
        public_key: Some("ssh-rsa PUBLIC KEY".to_string()),
        notes: Some("Some notes".into()),
        fields: Vec::new(),
    }).await?;
    
    if let Response::Error { message } = &res {
        anyhow::bail!("AddSshKey failed: {}", message);
    }
    assert!(matches!(res, Response::Ack));

    let res = client.send(Action::Sync).await?;
    if let Response::Error { message } = &res {
        anyhow::bail!("Sync failed: {}", message);
    }

    // 2. Verify
    let res = client.send(Action::GetEntries { query: None, entry_type: None }).await?;
    let id = if let Response::Entries { entries } = res {
        if entries.is_empty() {
             anyhow::bail!("No entries found after Sync");
        }
        let entry = entries.iter().find(|e| e.name == "My Work Key").ok_or_else(|| {
            let names: Vec<_> = entries.iter().map(|e| e.name.clone()).collect();
            anyhow::anyhow!("SSH Key not found. Available entries: {:?}", names)
        })?;
        entry.id.clone()
    } else {
        anyhow::bail!("Expected entries, got {:?}", res);
    };

    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    let mut entry = if let Response::Entry { entry } = res {
        if let cosmic_bwarden_core::db::EntryData::SshKey { private_key, public_key, .. } = &entry.data {
            assert_eq!(private_key.as_deref(), Some("PRIVATE KEY CONTENT"));
            assert_eq!(public_key.as_deref(), Some("ssh-rsa PUBLIC KEY"));
        } else {
            anyhow::bail!("Expected SshKey data, got {:?}", entry.data);
        }
        entry
    } else {
        anyhow::bail!("Expected entry");
    };

    // 3. Update SSH Key
    entry.name = "My Updated Key".to_string();
    if let cosmic_bwarden_core::db::EntryData::SshKey { ref mut public_key, .. } = entry.data {
        *public_key = Some("ssh-ed25519 NEW PUBLIC KEY".to_string());
    }
    let res = client.send(Action::UpdateEntry { entry }).await?;
    assert!(matches!(res, Response::Ack));

    client.send(Action::Sync).await?;

    // 4. Verify Update
    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    if let Response::Entry { entry } = res {
        assert_eq!(entry.name, "My Updated Key");
        if let cosmic_bwarden_core::db::EntryData::SshKey { public_key, .. } = &entry.data {
            assert_eq!(public_key.as_deref(), Some("ssh-ed25519 NEW PUBLIC KEY"));
        }
    }

    // 5. Delete
    client.send(Action::DeleteEntry { id }).await?;
    client.send(Action::Sync).await?;

    let res = client.send(Action::GetEntries { query: None, entry_type: None }).await?;
    if let Response::Entries { entries } = res {
        assert!(entries.is_empty());
    }

    Ok(())
}

#[tokio::test]
async fn test_card_crud_lifecycle() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "card-crud@example.com";
    let password = "cardpassword123";

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

    // 1. Create Card
    let res = client.send(Action::AddCard {
        name: "My Travel Card".to_string(),
        cardholder_name: Some("John Doe".to_string()),
        number: Some("1234567812345678".to_string().into()),
        brand: Some("Visa".to_string()),
        exp_month: Some("12".to_string()),
        exp_year: Some("2030".to_string()),
        code: Some("123".to_string().into()),
        notes: Some("Travel card".into()),
        fields: Vec::new(),
    }).await?;
    assert!(matches!(res, Response::Ack));

    client.send(Action::Sync).await?;

    // 2. Verify
    let res = client.send(Action::GetEntries { query: None, entry_type: None }).await?;
    let id = if let Response::Entries { entries } = res {
        let entry = entries.iter().find(|e| e.name == "My Travel Card").expect("Card not found");
        entry.id.clone()
    } else {
        anyhow::bail!("Expected entries");
    };

    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    if let Response::Entry { entry } = res {
        if let cosmic_bwarden_core::db::EntryData::Card { cardholder_name, number, .. } = &entry.data {
            assert_eq!(cardholder_name.as_deref(), Some("John Doe"));
            assert_eq!(number.as_deref(), Some("1234567812345678"));
        } else {
            anyhow::bail!("Expected Card data");
        }
    } else {
        anyhow::bail!("Expected entry");
    }

    // 3. Delete
    client.send(Action::DeleteEntry { id }).await?;
    client.send(Action::Sync).await?;
    Ok(())
}

#[tokio::test]
async fn test_identity_crud_lifecycle() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    
    let email = "identity-crud@example.com";
    let password = "identitypassword123";

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

    // 1. Create Identity
    let res = client.send(Action::AddIdentity {
        name: "My Identity".to_string(),
        first_name: Some("Jane".to_string()),
        last_name: Some("Smith".to_string()),
        address1: Some("123 Main St".to_string()),
        city: Some("Anytown".to_string()),
        state: Some("CA".to_string()),
        postal_code: Some("12345".to_string()),
        country: Some("US".to_string()),
        email: Some("jane@example.com".to_string()),
        phone: Some("555-1234".to_string()),
        notes: Some("Personal identity".into()),
        fields: Vec::new(),
    }).await?;
    assert!(matches!(res, Response::Ack));

    client.send(Action::Sync).await?;

    // 2. Verify
    let res = client.send(Action::GetEntries { query: None, entry_type: None }).await?;
    let id = if let Response::Entries { entries } = res {
        let entry = entries.iter().find(|e| e.name == "My Identity").expect("Identity not found");
        entry.id.clone()
    } else {
        anyhow::bail!("Expected entries");
    };

    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    if let Response::Entry { entry } = res {
        if let cosmic_bwarden_core::db::EntryData::Identity { first_name, last_name, email, .. } = &entry.data {
            assert_eq!(first_name.as_deref(), Some("Jane"));
            assert_eq!(last_name.as_deref(), Some("Smith"));
            assert_eq!(email.as_deref(), Some("jane@example.com"));
        } else {
            anyhow::bail!("Expected Identity data");
        }
    } else {
        anyhow::bail!("Expected entry");
    }

    // 3. Delete
    client.send(Action::DeleteEntry { id }).await?;
    client.send(Action::Sync).await?;
    Ok(())
}
