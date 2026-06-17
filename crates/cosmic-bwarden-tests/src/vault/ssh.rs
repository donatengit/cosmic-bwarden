use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};

#[tokio::test]
async fn test_ssh_key_crud_lifecycle() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "ssh-crud@example.com";
    let password = "sshpassword123";

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

    // 1. Create SSH Key
    let res = client
        .send(Action::AddSshKey {
            name: "My Work Key".to_string(),
            private_key: "PRIVATE KEY CONTENT".to_string().into(),
            public_key: Some("ssh-rsa PUBLIC KEY".to_string()),
            notes: Some("Some notes".into()),
            fields: Vec::new(),
        })
        .await?;

    if let Response::Error { message } = &res {
        anyhow::bail!("AddSshKey failed: {}", message);
    }
    assert!(matches!(res, Response::Ack));

    let res = client.send(Action::Sync).await?;
    if let Response::Error { message } = &res {
        anyhow::bail!("Sync failed: {}", message);
    }

    // 2. Verify
    let res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let id = if let Response::Entries { entries } = res {
        if entries.is_empty() {
            anyhow::bail!("No entries found after Sync");
        }
        let entry = entries
            .iter()
            .find(|e| e.name == "My Work Key")
            .ok_or_else(|| {
                let names: Vec<_> = entries.iter().map(|e| e.name.clone()).collect();
                anyhow::anyhow!("SSH Key not found. Available entries: {:?}", names)
            })?;
        entry.id.clone()
    } else {
        anyhow::bail!("Expected entries, got {:?}", res);
    };

    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    let mut entry = if let Response::Entry { entry } = res {
        if let cosmic_bwarden_core::db::EntryData::SshKey {
            private_key,
            public_key,
            ..
        } = &entry.data
        {
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
    let new_pub_key = "ssh-ed25519 NEW PUBLIC KEY".to_string();
    if let cosmic_bwarden_core::db::EntryData::SshKey {
        ref mut public_key, ..
    } = entry.data
    {
        *public_key = Some(new_pub_key.clone());
    }
    // Also update the fallback field to keep them in sync
    for field in &mut entry.fields {
        if field.name.as_deref() == Some("public_key") {
            field.value = Some(new_pub_key.clone().into());
        }
    }

    let res = client.send(Action::UpdateEntry { entry }).await?;
    assert!(matches!(res, Response::Ack));

    client.send(Action::Sync).await?;

    // 4. Verify Update
    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    if let Response::Entry { entry } = res {
        assert_eq!(entry.name, "My Updated Key");
        if let cosmic_bwarden_core::db::EntryData::SshKey { public_key, .. } = &entry.data {
            assert_eq!(public_key.as_deref(), Some("ssh-ed25519 NEW PUBLIC KEY"));
        }
    }

    // 5. Delete
    client.send(Action::DeleteEntry { id }).await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    if let Response::Entries { entries } = res {
        assert!(entries.is_empty());
    }

    Ok(())
}
