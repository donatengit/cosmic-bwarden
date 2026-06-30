/// Backend-unavailability lifecycle tests.
///
/// Strategy: after a successful login the agent stores the server URL in the
/// config file on disk and re-reads it on every network call (`load_legacy()`).
/// Tests corrupt the config's `base_url` to point at a refused port, trigger an
/// operation, then verify the error is surfaced and `sync_failed` is set in the
/// agent.  Restoring the URL + calling Sync verifies that `sync_failed` clears.
use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, EntryType, Response};
use std::path::Path;
use tokio::time::{sleep, Duration};

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

/// Read the config JSON on disk, swap `base_url` to the given value, write back.
fn set_config_base_url(config_path: &Path, url: &str) -> Result<()> {
    let raw = std::fs::read_to_string(config_path)?;
    let mut val: serde_json::Value = serde_json::from_str(&raw)?;
    if let Some(obj) = val.as_object_mut() {
        obj.insert("base_url".to_string(), serde_json::Value::String(url.to_string()));
        obj.insert("identity_url".to_string(), serde_json::Value::String(format!("{}/identity", url)));
    }
    std::fs::write(config_path, serde_json::to_string_pretty(&val)?)?;
    Ok(())
}

fn assert_sync_failed(res: &Response) {
    match res {
        Response::Config { sync_failed: true, .. } => {}
        other => panic!("Expected Config {{ sync_failed: true }}, got: {:?}", other),
    }
}

fn assert_sync_ok(res: &Response) {
    match res {
        Response::Config { sync_failed: false, .. } => {}
        other => panic!("Expected Config {{ sync_failed: false }}, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_delete_fails_gracefully_when_backend_unavailable() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "delete-err@example.com";
    let password = "deletepassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    // Create an entry while backend is healthy
    client
        .send(Action::AddEntry {
            name: "To Delete".to_string(),
            entry_type: EntryType::Login,
            username: Some("deluser".to_string()),
            password: Some("delpass".to_string().into()),
            notes: None,
            fields: Vec::new(),
        })
        .await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries { query: None, entry_type: None, only_pinned: false })
        .await?;
    let id = if let Response::SidebarEntries { entries } = res {
        entries.iter().find(|e| e.name == "To Delete").expect("entry not found").id.clone()
    } else {
        anyhow::bail!("Expected SidebarEntries");
    };

    // Confirm sync_failed is initially false
    let res = client.send(Action::GetConfig).await?;
    assert_sync_ok(&res);

    // Corrupt the config to point at a dead server
    set_config_base_url(&env.config_path, "http://localhost:1")?;
    // Give the agent time to pick up the changed config on next call
    sleep(Duration::from_millis(100)).await;

    // DeleteEntry must return Error (not panic, not hang forever)
    let res = client.send(Action::DeleteEntry { id: id.clone() }).await?;
    assert!(
        matches!(res, Response::Error { .. }),
        "DeleteEntry against unavailable backend must return Error, got: {:?}",
        res
    );

    // sync_failed must now be visible via GetConfig
    let res = client.send(Action::GetConfig).await?;
    assert_sync_failed(&res);

    // Restore the valid URL and sync — must clear sync_failed
    set_config_base_url(&env.config_path, &env.vault_url)?;
    sleep(Duration::from_millis(100)).await;

    // The entry still exists on the server (delete failed), so sync should succeed
    let res = client.send(Action::Sync).await?;
    assert!(matches!(res, Response::Ack), "Sync after restore must succeed");

    let res = client.send(Action::GetConfig).await?;
    assert_sync_ok(&res);

    // Clean up
    client.send(Action::DeleteEntry { id }).await?;
    client.send(Action::Sync).await?;

    Ok(())
}

#[tokio::test]
async fn test_add_fails_gracefully_when_backend_unavailable() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "add-err@example.com";
    let password = "addpassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    // Confirm healthy state
    let res = client.send(Action::GetConfig).await?;
    assert_sync_ok(&res);

    // Kill the server URL
    set_config_base_url(&env.config_path, "http://localhost:1")?;
    sleep(Duration::from_millis(100)).await;

    let res = client
        .send(Action::AddEntry {
            name: "Fails to Add".to_string(),
            entry_type: EntryType::Login,
            username: Some("failuser".to_string()),
            password: Some("failpass".to_string().into()),
            notes: None,
            fields: Vec::new(),
        })
        .await?;
    assert!(
        matches!(res, Response::Error { .. }),
        "AddEntry against unavailable backend must return Error, got: {:?}",
        res
    );

    let res = client.send(Action::GetConfig).await?;
    assert_sync_failed(&res);

    // Restore and verify recovery
    set_config_base_url(&env.config_path, &env.vault_url)?;
    sleep(Duration::from_millis(100)).await;

    let res = client.send(Action::Sync).await?;
    assert!(matches!(res, Response::Ack));

    let res = client.send(Action::GetConfig).await?;
    assert_sync_ok(&res);

    Ok(())
}

#[tokio::test]
async fn test_update_fails_gracefully_when_backend_unavailable() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "update-err@example.com";
    let password = "updatepassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    client
        .send(Action::AddEntry {
            name: "Fails Update".to_string(),
            entry_type: EntryType::Login,
            username: Some("upuser".to_string()),
            password: Some("uppass".to_string().into()),
            notes: None,
            fields: Vec::new(),
        })
        .await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries { query: None, entry_type: None, only_pinned: false })
        .await?;
    let id = if let Response::SidebarEntries { entries } = res {
        entries.iter().find(|e| e.name == "Fails Update").expect("not found").id.clone()
    } else {
        anyhow::bail!("Expected SidebarEntries");
    };

    let res = client.send(Action::GetEntry { id: id.clone(), password: None }).await?;
    let mut entry = if let Response::Entry { entry } = res { entry } else {
        anyhow::bail!("Expected Entry");
    };
    entry.name = "Should Not Reach Server".to_string();

    // Break the backend
    set_config_base_url(&env.config_path, "http://localhost:1")?;
    sleep(Duration::from_millis(100)).await;

    let res = client.send(Action::UpdateEntry { entry }).await?;
    assert!(
        matches!(res, Response::Error { .. }),
        "UpdateEntry against unavailable backend must return Error, got: {:?}",
        res
    );

    let res = client.send(Action::GetConfig).await?;
    assert_sync_failed(&res);

    // Restore and recover
    set_config_base_url(&env.config_path, &env.vault_url)?;
    sleep(Duration::from_millis(100)).await;

    let res = client.send(Action::Sync).await?;
    assert!(matches!(res, Response::Ack));

    let res = client.send(Action::GetConfig).await?;
    assert_sync_ok(&res);

    client.send(Action::DeleteEntry { id }).await?;
    client.send(Action::Sync).await?;

    Ok(())
}

#[tokio::test]
async fn test_sync_fails_sets_flag_recovery_clears_it() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "sync-err@example.com";
    let password = "syncpassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    // Healthy sync
    let res = client.send(Action::Sync).await?;
    assert!(matches!(res, Response::Ack));

    let res = client.send(Action::GetConfig).await?;
    assert_sync_ok(&res);

    // Break the backend
    set_config_base_url(&env.config_path, "http://localhost:1")?;
    sleep(Duration::from_millis(100)).await;

    let res = client.send(Action::Sync).await?;
    assert!(
        matches!(res, Response::Error { .. }),
        "Sync against unavailable backend must return Error, got: {:?}",
        res
    );

    let res = client.send(Action::GetConfig).await?;
    assert_sync_failed(&res);

    // Restore and verify sync_failed clears
    set_config_base_url(&env.config_path, &env.vault_url)?;
    sleep(Duration::from_millis(100)).await;

    let res = client.send(Action::Sync).await?;
    assert!(matches!(res, Response::Ack), "Sync after restore must succeed");

    let res = client.send(Action::GetConfig).await?;
    assert_sync_ok(&res);

    Ok(())
}

#[tokio::test]
async fn test_sync_failed_clears_on_lock() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "syncfail-lock@example.com";
    let password = "syncfaillock123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    // Trigger sync failure
    set_config_base_url(&env.config_path, "http://localhost:1")?;
    sleep(Duration::from_millis(100)).await;

    client.send(Action::Sync).await?;

    let res = client.send(Action::GetConfig).await?;
    assert_sync_failed(&res);

    // Lock must clear sync_failed
    client.send(Action::Lock).await?;

    let res = client.send(Action::GetConfig).await?;
    match res {
        Response::Config { sync_failed: false, is_locked: true, .. } => {}
        other => panic!("Expected Config {{ sync_failed: false, is_locked: true }}, got: {:?}", other),
    }

    Ok(())
}
