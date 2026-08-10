use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};

async fn login(env: &crate::common::TestEnv, email: &str, password: &str) -> Result<AgentClient> {
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
    Ok(client)
}

async fn count_by_name(client: &AgentClient, name: &str) -> Result<usize> {
    let res = client
        .send(Action::GetEntries {
            query: Some(name.to_string()),
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let Response::Entries { entries } = res else {
        anyhow::bail!("Expected entries, got: {res:?}");
    };
    Ok(entries.into_iter().filter(|e| e.name == name).count())
}

#[tokio::test]
async fn test_add_warns_on_duplicate_but_still_creates() -> Result<()> {
    let env = setup_env().await?;
    let email = "dupwarn@example.com";
    let password = "dupwarnpassword123";
    register_user(&env.vault_url, email, password).await?;
    let client = login(&env, email, password).await?;

    let output = env
        .cli_cmd()
        .args(["note", "add", "Dup Note", "notes=first"])
        .output()?;
    assert!(output.status.success());

    // Second add with the same name+type should still succeed, but warn.
    let output = env
        .cli_cmd()
        .args(["note", "add", "Dup Note", "notes=second"])
        .output()?;
    assert!(
        output.status.success(),
        "second add should still succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("already exists")
            && stderr.contains("--replace")
            && stderr.contains("--delete"),
        "expected duplicate warning mentioning --replace/--delete, got: {stderr}"
    );

    client.send(Action::Sync).await?;
    assert_eq!(count_by_name(&client, "Dup Note").await?, 2);

    Ok(())
}

#[tokio::test]
async fn test_add_replace_deletes_existing_duplicates() -> Result<()> {
    let env = setup_env().await?;
    let email = "dupreplace@example.com";
    let password = "dupreplacepassword123";
    register_user(&env.vault_url, email, password).await?;
    let client = login(&env, email, password).await?;

    for _ in 0..2 {
        let output = env
            .cli_cmd()
            .args(["note", "add", "Replace Me", "notes=stale"])
            .output()?;
        assert!(output.status.success());
    }
    client.send(Action::Sync).await?;
    assert_eq!(count_by_name(&client, "Replace Me").await?, 2);

    let output = env
        .cli_cmd()
        .args(["note", "add", "Replace Me", "--replace", "notes=fresh"])
        .output()?;
    assert!(
        output.status.success(),
        "add --replace failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    client.send(Action::Sync).await?;
    assert_eq!(
        count_by_name(&client, "Replace Me").await?,
        1,
        "--replace should leave exactly one entry behind"
    );

    let res = client
        .send(Action::GetEntries {
            query: Some("Replace Me".to_string()),
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let Response::Entries { entries } = res else {
        anyhow::bail!("Expected entries, got: {res:?}");
    };
    let entry = client
        .send(Action::GetEntry {
            id: entries[0].id.clone(),
            password: None,
        })
        .await?;
    let Response::Entry { entry } = entry else {
        anyhow::bail!("Expected entry, got: {entry:?}");
    };
    assert_eq!(entry.notes.expect("notes should be set").expose(), "fresh");

    Ok(())
}

#[tokio::test]
async fn test_edit_delete_removes_entry() -> Result<()> {
    let env = setup_env().await?;
    let email = "editdelete@example.com";
    let password = "editdeletepassword123";
    register_user(&env.vault_url, email, password).await?;
    let client = login(&env, email, password).await?;

    let output = env
        .cli_cmd()
        .args(["note", "add", "Delete Me", "notes=throwaway"])
        .output()?;
    assert!(output.status.success());
    client.send(Action::Sync).await?;
    assert_eq!(count_by_name(&client, "Delete Me").await?, 1);

    let output = env
        .cli_cmd()
        .args(["edit", "Delete Me", "--delete"])
        .output()?;
    assert!(
        output.status.success(),
        "edit --delete failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    client.send(Action::Sync).await?;
    assert_eq!(count_by_name(&client, "Delete Me").await?, 0);

    Ok(())
}

#[tokio::test]
async fn test_edit_delete_rejects_combined_args() -> Result<()> {
    let env = setup_env().await?;
    let email = "editdeletecombined@example.com";
    let password = "editdeletecombinedpassword123";
    register_user(&env.vault_url, email, password).await?;
    let client = login(&env, email, password).await?;

    let output = env
        .cli_cmd()
        .args(["note", "add", "Keep Me", "notes=stays"])
        .output()?;
    assert!(output.status.success());
    client.send(Action::Sync).await?;

    let output = env
        .cli_cmd()
        .args(["edit", "Keep Me", "--delete", "notes=changed"])
        .output()?;
    assert!(
        !output.status.success(),
        "expected --delete combined with other args to be rejected"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("--delete"),
        "expected error to mention --delete conflict, got: {stderr}"
    );

    client.send(Action::Sync).await?;
    assert_eq!(
        count_by_name(&client, "Keep Me").await?,
        1,
        "rejected edit --delete must not have deleted the entry"
    );

    Ok(())
}
