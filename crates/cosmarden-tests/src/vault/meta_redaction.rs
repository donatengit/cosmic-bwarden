//! `GetEntryMeta` is the action the desktop detail pane sends on a plain
//! selection (`vault_actions::fetch_entry`), and the agent answers it with a
//! redacted entry plus `filled_secrets`.
//!
//! Nothing in this suite sent it before: every read here used `GetEntry`, so the
//! contract the pane depends on — notes are *absent* from a meta read, and the
//! view must fetch them on an explicit reveal — had no end-to-end test. That is
//! the gap the empty-notes bug shipped through (docs/testing.md, "Which action
//! does the client send?").
//!
//! These tests pin the agent's half: what a meta read withholds, what it reports
//! as filled, and that the reveal it forces (`GetEntry`) returns the plaintext.

use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmarden_core::agent_client::AgentClient;
use cosmarden_core::db::EntryData;
use cosmarden_core::protocol::{Action, Response};

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

fn hidden_field(name: &str, value: &str) -> cosmarden_core::db::Field {
    cosmarden_core::db::Field {
        name: Some(name.to_string()),
        value: Some(value.into()),
        ty: Some(cosmarden_core::api::FieldType::Hidden),
        linked_id: None,
    }
}

#[tokio::test]
async fn test_get_entry_meta_redacts_notes_and_reports_them_filled() -> Result<()> {
    let env = setup_env().await?;
    let _profile = crate::state_guard::ProfileEnv::set(&env.profile);

    let email = "meta-notes@example.com";
    let password = "metapassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    client
        .send(Action::AddEntry {
            name: "Meta Login".to_string(),
            entry_type: cosmarden_core::protocol::EntryType::Login,
            username: Some("user".to_string()),
            password: Some("secret".to_string().into()),
            notes: Some("a note the detail pane must reveal".into()),
            fields: vec![hidden_field("Recovery Code", "abc-123")],
            totp: None,
            uris: Vec::new(),
        })
        .await?;
    client.send(Action::Sync).await?;

    let id = find_entry_id(&client, "Meta Login").await?;

    // 1. The meta read the desktop pane actually sends.
    let res = client.send(Action::GetEntryMeta { id: id.clone() }).await?;
    let (entry, filled) = if let Response::EntryMeta {
        entry,
        filled_secrets,
    } = res
    {
        (entry, filled_secrets)
    } else {
        anyhow::bail!("GetEntryMeta must answer EntryMeta, got {res:?}");
    };

    assert!(
        entry.notes.is_none(),
        "GetEntryMeta leaked notes: the pane cannot tell an empty slot from a redacted one"
    );
    match &entry.data {
        EntryData::Login {
            username, password, ..
        } => {
            assert_eq!(username.as_deref(), Some("user"));
            assert!(password.is_none(), "GetEntryMeta leaked the password");
        }
        other => anyhow::bail!("expected a Login entry, got {other:?}"),
    }
    let hidden = entry
        .fields
        .iter()
        .find(|f| f.name.as_deref() == Some("Recovery Code"))
        .expect("hidden custom field must survive redaction by name");
    assert!(
        hidden.value.is_none(),
        "GetEntryMeta leaked a hidden custom field"
    );

    // Presence survives redaction, and is what tells the pane to draw a masked
    // row — notes included.
    for key in ["Password", "Notes", "Recovery Code"] {
        assert!(
            filled.iter().any(|k| k == key),
            "filled_secrets must report {key}: {filled:?}"
        );
    }

    // 2. The reveal the pane performs for that masked notes row.
    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    let entry = if let Response::Entry { entry } = res {
        entry
    } else {
        anyhow::bail!("GetEntry must answer Entry");
    };
    assert_eq!(
        entry.notes.as_ref().map(|n| n.expose()),
        Some("a note the detail pane must reveal"),
        "the reveal path must return the body the meta read withheld"
    );

    Ok(())
}

#[tokio::test]
async fn test_secure_note_meta_reports_its_content_as_the_notes_slot() -> Result<()> {
    let env = setup_env().await?;
    let _profile = crate::state_guard::ProfileEnv::set(&env.profile);

    let email = "meta-secure-note@example.com";
    let password = "metanotepassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    client
        .send(Action::AddSecureNote {
            name: "Meta Note".to_string(),
            notes: "the note body".to_string().into(),
            fields: Vec::new(),
        })
        .await?;
    client.send(Action::Sync).await?;
    let id = find_entry_id(&client, "Meta Note").await?;

    // A SecureNote's content IS its notes, so a meta read reports the type and
    // the filled slot while withholding the body — which is why the pane fetches
    // the entry on selection instead of rendering an unreadable item.
    let res = client.send(Action::GetEntryMeta { id: id.clone() }).await?;
    if let Response::EntryMeta {
        entry,
        filled_secrets,
    } = res
    {
        assert!(matches!(entry.data, EntryData::SecureNote));
        assert!(
            entry.notes.is_none(),
            "GetEntryMeta leaked a SecureNote body"
        );
        assert!(
            filled_secrets.iter().any(|k| k == "Notes"),
            "a SecureNote with content must report the Notes slot filled: {filled_secrets:?}"
        );
    } else {
        anyhow::bail!("GetEntryMeta must answer EntryMeta, got {res:?}");
    }

    let res = client.send(Action::GetEntry { id, password: None }).await?;
    let entry = if let Response::Entry { entry } = res {
        entry
    } else {
        anyhow::bail!("GetEntry must answer Entry");
    };
    assert_eq!(
        entry.notes.as_ref().map(|n| n.expose()),
        Some("the note body"),
        "the content fetch a SecureNote selection triggers must return the body"
    );

    Ok(())
}

/// The id of the named entry, via the sidebar listing the desktop uses.
async fn find_entry_id(client: &AgentClient, name: &str) -> Result<String> {
    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
            domain: None,
        })
        .await?;
    if let Response::SidebarEntries { entries } = res {
        entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.id.clone())
            .ok_or_else(|| anyhow::anyhow!("{name} not found in the sidebar listing"))
    } else {
        anyhow::bail!("Expected SidebarEntries")
    }
}
