/// Full CRUD tests for SshKey and SecureNote entry types.
use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::db::EntryData;
use cosmic_bwarden_core::protocol::{Action, EntryType, Response};

async fn login(client: &AgentClient, vault_url: &str, email: &str, password: &str) -> Result<()> {
    client
        .send(Action::Login {
            email: email.to_string(),
            password: password.to_string(),
            server_url: Some(vault_url.to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn test_ssh_key_full_crud() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "sshkey-crud@example.com";
    let password = "sshkeypassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    // 1. Create
    let res = client
        .send(Action::AddSshKey {
            name: "My SSH Key".to_string(),
            private_key: "-----BEGIN OPENSSH PRIVATE KEY-----\nFAKEKEYDATA\n-----END OPENSSH PRIVATE KEY-----".to_string().into(),
            public_key: Some("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI test@host".to_string()),
            notes: Some("SSH key for test".into()),
            fields: Vec::new(),
        })
        .await?;
    assert!(matches!(res, Response::Ack), "AddSshKey must return Ack");

    client.send(Action::Sync).await?;

    // 2. Read via sidebar (check type + public key)
    let res = client
        .send(Action::GetSidebarEntries { query: None, entry_type: None, only_pinned: false })
        .await?;
    let id = if let Response::SidebarEntries { entries } = res {
        let e = entries.iter().find(|e| e.name == "My SSH Key").expect("SSH key not found");
        assert_eq!(e.entry_type, EntryType::SshKey);
        assert_eq!(e.public_key.as_deref(), Some("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI test@host"));
        e.id.clone()
    } else {
        anyhow::bail!("Expected SidebarEntries");
    };

    // 3. Read full entry — verify private key and notes
    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    let mut entry = if let Response::Entry { entry } = res {
        if let EntryData::SshKey { private_key, public_key, .. } = &entry.data {
            assert!(private_key.as_ref().map(|k| k.expose().contains("FAKEKEYDATA")).unwrap_or(false));
            assert_eq!(public_key.as_deref(), Some("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI test@host"));
        } else {
            anyhow::bail!("Expected SshKey data");
        }
        assert_eq!(entry.notes.as_ref().map(|n| n.expose()), Some("SSH key for test"));
        entry
    } else {
        anyhow::bail!("Expected Entry response");
    };

    // 4. Update — rename and change public key
    entry.name = "Updated SSH Key".to_string();
    if let EntryData::SshKey { ref mut public_key, .. } = entry.data {
        *public_key = Some("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI updated@host".to_string());
    }
    let res = client.send(Action::UpdateEntry { entry }).await?;
    assert!(matches!(res, Response::Ack), "UpdateEntry must return Ack");

    client.send(Action::Sync).await?;

    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    if let Response::Entry { entry } = res {
        assert_eq!(entry.name, "Updated SSH Key");
        if let EntryData::SshKey { public_key, .. } = &entry.data {
            assert_eq!(public_key.as_deref(), Some("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI updated@host"));
        }
    } else {
        anyhow::bail!("Expected Entry after update");
    }

    // 5. Delete
    client.send(Action::DeleteEntry { id: id.clone() }).await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries { query: None, entry_type: None, only_pinned: false })
        .await?;
    if let Response::SidebarEntries { entries } = res {
        assert!(!entries.iter().any(|e| e.id == id), "SSH key must be gone after delete");
    }

    Ok(())
}

#[tokio::test]
async fn test_secure_note_full_crud() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "securenote-crud@example.com";
    let password = "notepassword456";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    // 1. Create via AddSecureNote (dedicated action, type=SecureNote)
    let res = client
        .send(Action::AddSecureNote {
            name: "My Secure Note".to_string(),
            notes: "Initial secret content".to_string().into(),
            fields: Vec::new(),
        })
        .await?;
    assert!(matches!(res, Response::Ack), "AddSecureNote must return Ack");

    client.send(Action::Sync).await?;

    // 2. Verify in sidebar
    let res = client
        .send(Action::GetSidebarEntries { query: None, entry_type: None, only_pinned: false })
        .await?;
    let id = if let Response::SidebarEntries { entries } = res {
        let e = entries.iter().find(|e| e.name == "My Secure Note").expect("note not found");
        assert_eq!(e.entry_type, EntryType::SecureNote);
        e.id.clone()
    } else {
        anyhow::bail!("Expected SidebarEntries");
    };

    // 3. Read content via GetPassword (returns notes field for SecureNote)
    let res = client.send(Action::GetPassword { id: id.clone(), password: None }).await?;
    if let Response::Password { password: content } = res {
        assert_eq!(content, "Initial secret content");
    } else {
        anyhow::bail!("Expected Password response for SecureNote");
    }

    // 4. Update content
    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    let mut entry = if let Response::Entry { entry } = res {
        assert!(matches!(entry.data, EntryData::SecureNote));
        assert_eq!(entry.notes.as_ref().map(|n| n.expose()), Some("Initial secret content"));
        entry
    } else {
        anyhow::bail!("Expected Entry");
    };

    entry.notes = Some("Updated secret content".into());
    let res = client.send(Action::UpdateEntry { entry }).await?;
    assert!(matches!(res, Response::Ack));

    client.send(Action::Sync).await?;

    let res = client.send(Action::GetPassword { id: id.clone(), password: None }).await?;
    if let Response::Password { password: content } = res {
        assert_eq!(content, "Updated secret content");
    } else {
        anyhow::bail!("Expected Password response after update");
    }

    // 5. Delete
    client.send(Action::DeleteEntry { id: id.clone() }).await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries { query: None, entry_type: None, only_pinned: false })
        .await?;
    if let Response::SidebarEntries { entries } = res {
        assert!(!entries.iter().any(|e| e.id == id));
    }

    Ok(())
}
