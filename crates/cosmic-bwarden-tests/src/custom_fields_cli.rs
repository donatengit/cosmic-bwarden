use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};

#[tokio::test]
async fn test_custom_fields_cli() -> Result<()> {
    let env = setup_env().await?;

    let email = "fields@example.com";
    let password = "fieldpassword123";

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

    // 1. Add entry with custom fields via CLI
    let output = env.cli_cmd()
        .arg("add")
        .arg("FieldEntry")
        .arg("username=user1")
        .arg("password=pass1")
        .arg("--field")
        .arg("MyText=Value1")
        .arg("--secret-field")
        .arg("MySecret=SecretValue")
        .output()?;

    assert!(
        output.status.success(),
        "CLI add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 2. Verify via agent
    client.send(Action::Sync).await?;
    let res = client
        .send(Action::GetEntries {
            query: Some("FieldEntry".to_string()),
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let entry = if let Response::Entries { entries } = res {
        entries[0].clone()
    } else {
        anyhow::bail!("Expected entries");
    };

    let res = client
        .send(Action::GetEntry {
            id: entry.id.clone(),
            password: None,
        })
        .await?;
    let entry = if let Response::Entry { entry } = res {
        entry
    } else {
        anyhow::bail!("Expected full entry");
    };

    let f1 = entry.get_field("MyText").expect("MyText field missing");
    assert_eq!(f1.value.as_deref(), Some("Value1"));
    assert_eq!(f1.ty, Some(cosmic_bwarden_core::api::FieldType::Text));

    let f2 = entry.get_field("MySecret").expect("MySecret field missing");
    assert_eq!(f2.value.as_deref(), Some("SecretValue"));
    assert_eq!(f2.ty, Some(cosmic_bwarden_core::api::FieldType::Hidden));

    // 3. Test CLI output masking
    let output = env.cli_cmd()
        .arg("get")
        .arg("FieldEntry")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MyText: Value1"));
    assert!(stdout.contains("MySecret: ********"));
    assert!(!stdout.contains("SecretValue"));

    // 4. Edit field via CLI
    let output = env.cli_cmd()
        .arg("edit")
        .arg("FieldEntry")
        .arg("--field")
        .arg("MyText=UpdatedValue")
        .output()?;

    assert!(
        output.status.success(),
        "CLI edit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    client.send(Action::Sync).await?;
    let res = client
        .send(Action::GetEntry {
            id: entry.id.clone(),
            password: None,
        })
        .await?;
    if let Response::Entry { entry } = res {
        let f = entry.get_field("MyText").unwrap();
        assert_eq!(f.value.as_deref(), Some("UpdatedValue"));
    }

    Ok(())
}
