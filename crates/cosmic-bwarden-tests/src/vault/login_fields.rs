/// Tests for Login-specific fields: URIs, all custom field types, and TOTP.
use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::db::{EntryData, Field};
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
async fn test_login_with_uris() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "uri-test@example.com";
    let password = "uripassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    // 1. Create entry
    client
        .send(Action::AddEntry {
            name: "URI Site".to_string(),
            entry_type: EntryType::Login,
            username: Some("uriuser".to_string()),
            password: Some("uripass".to_string().into()),
            notes: None,
            fields: Vec::new(),
        })
        .await?;

    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries { query: None, entry_type: None, only_pinned: false })
        .await?;
    let id = if let Response::SidebarEntries { entries } = res {
        entries.iter().find(|e| e.name == "URI Site").expect("entry not found").id.clone()
    } else {
        anyhow::bail!("Expected SidebarEntries");
    };

    // 2. Add URIs via update
    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    let mut entry = if let Response::Entry { entry } = res { entry } else {
        anyhow::bail!("Expected Entry");
    };

    if let EntryData::Login { ref mut uris, .. } = entry.data {
        uris.push(cosmic_bwarden_core::db::Uri {
            uri: "https://example.com".to_string(),
            match_type: None,
        });
        uris.push(cosmic_bwarden_core::db::Uri {
            uri: "https://www.example.com".to_string(),
            match_type: None,
        });
    }

    let res = client.send(Action::UpdateEntry { entry }).await?;
    assert!(matches!(res, Response::Ack));

    client.send(Action::Sync).await?;

    // 3. Verify URIs persisted
    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    if let Response::Entry { entry } = res {
        if let EntryData::Login { uris, .. } = &entry.data {
            assert_eq!(uris.len(), 2, "Must have 2 URIs after update");
            assert!(uris.iter().any(|u| u.uri == "https://example.com"));
            assert!(uris.iter().any(|u| u.uri == "https://www.example.com"));
        } else {
            anyhow::bail!("Expected Login data");
        }
    } else {
        anyhow::bail!("Expected Entry after update");
    }

    client.send(Action::DeleteEntry { id }).await?;
    client.send(Action::Sync).await?;

    Ok(())
}

#[tokio::test]
async fn test_custom_fields_all_types() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "fields-all@example.com";
    let password = "fieldspassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    // Create entry with text, hidden, and boolean custom fields
    client
        .send(Action::AddEntry {
            name: "Fields Entry".to_string(),
            entry_type: EntryType::Login,
            username: Some("fieldsuser".to_string()),
            password: Some("fieldspass".to_string().into()),
            notes: None,
            fields: vec![
                Field {
                    name: Some("TextField".to_string()),
                    value: Some("plain text".into()),
                    ty: Some(cosmic_bwarden_core::api::FieldType::Text),
                    linked_id: None,
                },
                Field {
                    name: Some("HiddenField".to_string()),
                    value: Some("secret value".into()),
                    ty: Some(cosmic_bwarden_core::api::FieldType::Hidden),
                    linked_id: None,
                },
                Field {
                    name: Some("BoolField".to_string()),
                    value: Some("true".into()),
                    ty: Some(cosmic_bwarden_core::api::FieldType::Boolean),
                    linked_id: None,
                },
            ],
        })
        .await?;

    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries { query: None, entry_type: None, only_pinned: false })
        .await?;
    let id = if let Response::SidebarEntries { entries } = res {
        entries.iter().find(|e| e.name == "Fields Entry").expect("not found").id.clone()
    } else {
        anyhow::bail!("Expected SidebarEntries");
    };

    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    if let Response::Entry { entry } = res {
        let tf = entry.get_field("TextField").expect("TextField missing");
        assert_eq!(tf.ty, Some(cosmic_bwarden_core::api::FieldType::Text));
        assert_eq!(tf.value.as_ref().map(|v| v.expose()), Some("plain text"));

        let hf = entry.get_field("HiddenField").expect("HiddenField missing");
        assert_eq!(hf.ty, Some(cosmic_bwarden_core::api::FieldType::Hidden));
        assert_eq!(hf.value.as_ref().map(|v| v.expose()), Some("secret value"));

        let bf = entry.get_field("BoolField").expect("BoolField missing");
        assert_eq!(bf.ty, Some(cosmic_bwarden_core::api::FieldType::Boolean));
        assert_eq!(bf.value.as_ref().map(|v| v.expose()), Some("true"));
    } else {
        anyhow::bail!("Expected Entry");
    }

    client.send(Action::DeleteEntry { id }).await?;
    client.send(Action::Sync).await?;

    Ok(())
}

#[tokio::test]
async fn test_get_totp_from_login_entry() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "totp-test@example.com";
    let password = "totppassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    // Create Login entry without TOTP first
    client
        .send(Action::AddEntry {
            name: "TOTP Site".to_string(),
            entry_type: EntryType::Login,
            username: Some("totpuser".to_string()),
            password: Some("totppass".to_string().into()),
            notes: None,
            fields: Vec::new(),
        })
        .await?;

    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries { query: None, entry_type: None, only_pinned: false })
        .await?;
    let id = if let Response::SidebarEntries { entries } = res {
        entries.iter().find(|e| e.name == "TOTP Site").expect("not found").id.clone()
    } else {
        anyhow::bail!("Expected SidebarEntries");
    };

    // GetTotp on entry without TOTP must return Error, not panic
    let res = client.send(Action::GetTotp { id: id.clone() }).await?;
    assert!(
        matches!(res, Response::Error { .. }),
        "GetTotp on entry without TOTP must return Error"
    );

    // Add a valid base32 TOTP secret via update (JBSWY3DPEHPK3PXP = "Hello World!")
    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    let mut entry = if let Response::Entry { entry } = res { entry } else {
        anyhow::bail!("Expected Entry");
    };

    if let EntryData::Login { ref mut totp, .. } = entry.data {
        *totp = Some("JBSWY3DPEHPK3PXP".to_string().into());
    }

    let res = client.send(Action::UpdateEntry { entry }).await?;
    assert!(matches!(res, Response::Ack));
    client.send(Action::Sync).await?;

    // GetTotp must return a 6-digit code
    let res = client.send(Action::GetTotp { id: id.clone() }).await?;
    if let Response::Totp { code } = res {
        assert_eq!(code.len(), 6, "TOTP code must be 6 digits, got: {}", code);
        assert!(
            code.chars().all(|c| c.is_ascii_digit()),
            "TOTP code must be all digits, got: {}",
            code
        );
    } else {
        anyhow::bail!("Expected Totp response, got {:?}", res);
    }

    client.send(Action::DeleteEntry { id }).await?;
    client.send(Action::Sync).await?;

    Ok(())
}
