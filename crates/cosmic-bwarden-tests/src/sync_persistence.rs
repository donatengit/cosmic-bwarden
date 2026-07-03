//! Every vault modification must survive a forced Sync.
//!
//! `Action::Sync` replaces the local DB with server state, so these tests
//! prove each modification actually reached the server — a modification that
//! only lands in local state silently disappears on the next sync (the bug
//! that hit favorites when the server rejected `PUT /ciphers/{id}/favorite`).

use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::db::{EntryData, Field};
use cosmic_bwarden_core::protocol::{Action, EntryType, Response};

async fn login(client: &AgentClient, email: &str, password: &str, url: &str) -> Result<()> {
    client
        .send(Action::Login {
            email: email.to_string(),
            password: password.to_string(),
            server_url: Some(url.to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;
    Ok(())
}

async fn forced_sync(client: &AgentClient) -> Result<()> {
    let res = client.send(Action::Sync).await?;
    if let Response::Error { message } = res {
        anyhow::bail!("Sync failed: {}", message);
    }
    Ok(())
}

async fn get_entry(client: &AgentClient, id: &str) -> Result<cosmic_bwarden_core::db::Entry> {
    let res = client
        .send(Action::GetEntry {
            id: id.to_string(),
            password: None,
        })
        .await?;
    match res {
        Response::Entry { entry } => Ok(entry),
        other => anyhow::bail!("Expected entry, got {:?}", other),
    }
}

/// One flow exercising every modification type, each followed by a forced
/// Sync and verified against the freshly-pulled server state:
/// add → edit (name/username/password/notes/custom field) → favorite →
/// unfavorite → delete.
#[tokio::test]
async fn test_all_modifications_survive_forced_sync() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "sync-persist@example.com";
    let password = "syncpersist123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, email, password, &env.vault_url).await?;

    // --- Add: entry with all optional parts must survive Sync ---
    let res = client
        .send(Action::AddEntry {
            name: "Persist Login".to_string(),
            entry_type: EntryType::Login,
            username: Some("user-a".to_string()),
            password: Some("pass-a".to_string().into()),
            notes: Some("note-a".into()),
            fields: vec![Field {
                name: Some("CustomField".to_string()),
                value: Some("value-a".into()),
                ty: Some(cosmic_bwarden_core::api::FieldType::Text),
                linked_id: None,
            }],
        })
        .await?;
    assert!(matches!(res, Response::Ack), "AddEntry failed: {:?}", res);
    forced_sync(&client).await?;

    let res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let id = match res {
        Response::Entries { entries } => {
            let e = entries
                .iter()
                .find(|e| e.name == "Persist Login")
                .expect("added entry must survive Sync");
            e.id.clone()
        }
        other => anyhow::bail!("Expected entries, got {:?}", other),
    };

    let entry = get_entry(&client, &id).await?;
    match &entry.data {
        EntryData::Login {
            username, password, ..
        } => {
            assert_eq!(username.as_deref(), Some("user-a"));
            assert_eq!(password.as_deref(), Some("pass-a"));
        }
        other => anyhow::bail!("Expected Login data, got {:?}", other),
    }
    assert_eq!(entry.notes.as_deref(), Some("note-a"));
    assert_eq!(entry.fields.len(), 1);
    assert_eq!(entry.fields[0].value.as_deref(), Some("value-a"));

    // --- Edit: every editable part changed, must survive Sync ---
    let mut edited = entry;
    edited.name = "Persist Login v2".to_string();
    if let EntryData::Login {
        ref mut username,
        ref mut password,
        ..
    } = edited.data
    {
        *username = Some("user-b".to_string());
        *password = Some("pass-b".to_string().into());
    }
    edited.notes = Some("note-b".into());
    edited.fields[0].value = Some("value-b".into());
    let res = client.send(Action::UpdateEntry { entry: edited }).await?;
    assert!(matches!(res, Response::Ack), "UpdateEntry failed: {:?}", res);
    forced_sync(&client).await?;

    let entry = get_entry(&client, &id).await?;
    assert_eq!(entry.name, "Persist Login v2", "name edit must survive Sync");
    match &entry.data {
        EntryData::Login {
            username, password, ..
        } => {
            assert_eq!(
                username.as_deref(),
                Some("user-b"),
                "username edit must survive Sync"
            );
            assert_eq!(
                password.as_deref(),
                Some("pass-b"),
                "password edit must survive Sync"
            );
        }
        other => anyhow::bail!("Expected Login data, got {:?}", other),
    }
    assert_eq!(
        entry.notes.as_deref(),
        Some("note-b"),
        "notes edit must survive Sync"
    );
    assert_eq!(
        entry.fields[0].value.as_deref(),
        Some("value-b"),
        "custom-field edit must survive Sync"
    );

    // --- Favorite: must survive Sync ---
    let res = client.send(Action::PinEntry { id: id.clone() }).await?;
    assert!(matches!(res, Response::Ack), "PinEntry failed: {:?}", res);
    forced_sync(&client).await?;

    let entry = get_entry(&client, &id).await?;
    assert!(entry.favorite, "favorite must survive Sync");

    // --- Unfavorite: must survive Sync ---
    let res = client.send(Action::UnpinEntry { id: id.clone() }).await?;
    assert!(matches!(res, Response::Ack), "UnpinEntry failed: {:?}", res);
    forced_sync(&client).await?;

    let entry = get_entry(&client, &id).await?;
    assert!(!entry.favorite, "unfavorite must survive Sync");

    // --- Delete: must survive Sync ---
    let res = client.send(Action::DeleteEntry { id: id.clone() }).await?;
    assert!(matches!(res, Response::Ack), "DeleteEntry failed: {:?}", res);
    forced_sync(&client).await?;

    let res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    if let Response::Entries { entries } = res {
        assert!(
            !entries.iter().any(|e| e.id == id),
            "deletion must survive Sync"
        );
    }

    Ok(())
}
