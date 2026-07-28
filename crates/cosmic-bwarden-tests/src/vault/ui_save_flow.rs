//! E2E coverage for the seam between the UI's save button and the server.
//!
//! The rest of this suite hand-builds `Action::AddEntry`/`Action::UpdateEntry`
//! and sends them, which proves the *agent* handles those actions — it can
//! never catch the client picking the wrong one. That gap let a real bug ship:
//! the vault window sent every new entry through `UpdateEntry`, so the agent
//! issued `PUT /ciphers/new-<unix_secs>` against the placeholder id and
//! Vaultwarden rejected it with HTTP 400, silently discarding the user's work.
//!
//! These tests instead build the draft `Entry` exactly as the UI's
//! `Message::AddEntryRequested` does and route it through the same
//! `entry_save::save_action` the UI calls, so the action under test is chosen
//! by production code rather than by the test author.

use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::db::{Entry, EntryData, Secret};
use cosmic_bwarden_core::protocol::entry_save::save_action;
use cosmic_bwarden_core::protocol::{Action, Response};

/// Mirrors the draft the vault window creates on "Add entry": a placeholder
/// `new-<unix_secs>` id, since the server has not assigned one yet.
fn ui_draft(name: &str, data: EntryData) -> Entry {
    Entry {
        id: cosmic_bwarden_core::protocol::entry_save::new_placeholder_id(),
        org_id: None,
        folder: None,
        folder_id: None,
        name: name.to_string(),
        favorite: false,
        data,
        fields: Vec::new(),
        notes: None,
        history: Vec::new(),
        key: None,
        master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
    }
}

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

/// The regression test proper: a draft carrying the UI's placeholder id must
/// reach the server as a creation and be accepted. Before the fix this failed
/// with `Response::Error` carrying "request failed with status: 400".
#[tokio::test]
async fn test_ui_new_login_draft_is_created_not_updated() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "ui-save-flow@example.com";
    let password = "uisavepassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, email, password, &env.vault_url).await?;

    let draft = ui_draft(
        "UI Draft Login",
        EntryData::Login {
            username: Some("drafted-user".to_string()),
            password: Some(Secret::from("drafted-pass")),
            totp: None,
            uris: Vec::new(),
        },
    );

    // The UI's own mapping decides the action — not this test.
    let action = save_action(draft);
    assert!(
        matches!(action, Action::AddEntry { .. }),
        "a placeholder-id draft must map to a creation, got {}",
        action.variant_name()
    );

    let res = client.send(action).await?;
    if let Response::Error { message } = &res {
        anyhow::bail!("server rejected the UI's new-entry action: {}", message);
    }

    client.send(Action::Sync).await?;

    // It must actually exist server-side, with a real id — not the placeholder.
    let res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let Response::Entries { entries } = res else {
        anyhow::bail!("Expected entries");
    };
    let created = entries
        .iter()
        .find(|e| e.name == "UI Draft Login")
        .expect("the drafted entry must exist on the server");
    assert!(
        !created.id.starts_with("new-"),
        "server must have assigned a real id, got {}",
        created.id
    );

    Ok(())
}

/// The other half of the branch: once an entry carries a real server id,
/// editing it must still go through `UpdateEntry` rather than creating a
/// duplicate on every save.
#[tokio::test]
async fn test_ui_edit_of_saved_entry_updates_in_place() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "ui-save-edit@example.com";
    let password = "uisaveeditpassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, email, password, &env.vault_url).await?;

    let draft = ui_draft(
        "UI Editable",
        EntryData::Login {
            username: Some("before".to_string()),
            password: Some(Secret::from("pw")),
            totp: None,
            uris: Vec::new(),
        },
    );
    client.send(save_action(draft)).await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let Response::Entries { entries } = res else {
        anyhow::bail!("Expected entries");
    };
    let id = entries
        .iter()
        .find(|e| e.name == "UI Editable")
        .expect("entry must exist")
        .id
        .clone();

    // Fetch it the way the detail pane does, edit a field, save again.
    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    let Response::Entry { mut entry } = res else {
        anyhow::bail!("Expected full entry");
    };
    if let EntryData::Login { username, .. } = &mut entry.data {
        *username = Some("after".to_string());
    }

    let action = save_action(entry);
    assert!(
        matches!(action, Action::UpdateEntry { .. }),
        "a server-backed entry must map to an update, got {}",
        action.variant_name()
    );
    let res = client.send(action).await?;
    if let Response::Error { message } = &res {
        anyhow::bail!("server rejected the UI's edit action: {}", message);
    }

    client.send(Action::Sync).await?;

    // Edited in place: one entry, new username — not a second copy.
    let res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let Response::Entries { entries } = res else {
        anyhow::bail!("Expected entries");
    };
    let matching: Vec<_> = entries.iter().filter(|e| e.name == "UI Editable").collect();
    assert_eq!(
        matching.len(),
        1,
        "editing must not create a duplicate entry"
    );
    match &matching[0].data {
        EntryData::Login { username, .. } => assert_eq!(username.as_deref(), Some("after")),
        other => anyhow::bail!("expected a login, got {:?}", other),
    }

    Ok(())
}
