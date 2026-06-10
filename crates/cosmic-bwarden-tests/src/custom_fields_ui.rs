use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};

#[tokio::test]
async fn test_custom_fields_ui_simulation() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "ui-sim@example.com";
    let password = "uipassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new();
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

    // 1. Add entry with custom field via CLI or agent
    client
        .send(Action::AddEntry {
            name: "UIEntry".to_string(),
            entry_type: cosmic_bwarden_core::protocol::EntryType::Login,
            username: Some("user1".to_string()),
            password: Some("pass1".to_string().into()),
            notes: None,
            fields: vec![cosmic_bwarden_core::db::Field {
                name: Some("UIField".to_string()),
                value: Some("Initial".into()),
                ty: Some(cosmic_bwarden_core::api::FieldType::Text),
                linked_id: None,
            }],
        })
        .await?;
    client.send(Action::Sync).await?;

    // 2. Fetch entry (as UI would do when selecting it)
    let res = client
        .send(Action::GetEntries {
            query: Some("UIEntry".to_string()),
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let id = if let Response::Entries { entries } = res {
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
        anyhow::bail!("Expected full entry");
    };

    // 3. Simulate UI editing field
    // In UI, this updates self.editing_entry
    if let Some(f) = entry
        .fields
        .iter_mut()
        .find(|f| f.name.as_deref() == Some("UIField"))
    {
        f.value = Some("UpdatedByUI".into());
    }

    // 4. Save changes (as UI would do when clicking 'Save')
    client
        .send(Action::UpdateEntry {
            entry: entry.clone(),
        })
        .await?;
    client.send(Action::Sync).await?;

    // 5. Verify persistence and decryption
    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    if let Response::Entry { entry } = res {
        let f = entry.get_field("UIField").unwrap();
        assert_eq!(f.value.as_deref(), Some("UpdatedByUI"));
    }

    Ok(())
}
