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
use cosmarden_core::agent_client::AgentClient;
use cosmarden_core::api::{Client, KdfType};
use cosmarden_core::cipherstring::CipherString;
use cosmarden_core::db::{Entry, EntryData, Secret};
use cosmarden_core::identity::Identity;
use cosmarden_core::locked;
use cosmarden_core::protocol::entry_save::save_action;
use cosmarden_core::protocol::{Action, Response};
use std::collections::HashMap;

/// Mirrors the draft the vault window creates on "Add entry": a placeholder
/// `new-<unix_secs>` id, since the server has not assigned one yet.
fn ui_draft(name: &str, data: EntryData) -> Entry {
    Entry {
        id: cosmarden_core::protocol::entry_save::new_placeholder_id(),
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
        master_password_reprompt: cosmarden_core::api::CipherRepromptType::None,
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
    let _profile = crate::state_guard::ProfileEnv::set(&env.profile);

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
    let _profile = crate::state_guard::ProfileEnv::set(&env.profile);

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

/// The vault keys, derived in the test process so it can create a cipher the
/// way the official Bitwarden clients do — with its own item-level key. The
/// agent's session is separate; this one exists only to talk to the server
/// directly.
async fn derive_vault_keys(
    url: &str,
    email: &str,
    password: &str,
) -> Result<(String, locked::Keys, String)> {
    let identity = Identity::new(
        email,
        &locked::Password::from_string(password),
        KdfType::Pbkdf2,
        600_000,
        None,
        None,
    )?;

    let client = Client::new(&format!("{url}/api"), &format!("{url}/identity"));
    let (access_token, _refresh_token, protected_key) = client
        .login(
            email,
            "5f9ecb16-8f1a-4d1b-9d3f-0000000000e2",
            &identity.master_password_hash,
            None,
            None,
            None,
            None,
        )
        .await?;
    let protected_key =
        protected_key.ok_or_else(|| anyhow::anyhow!("no protected key in login"))?;

    let (vault_keys, _org_keys) = cosmarden_core::vault::unlock_from_keys(
        &identity.keys,
        &protected_key,
        None,
        &HashMap::new(),
    )?;
    // The account UUID is required as `encryptedFor` on the direct cipher
    // create below (Vaultwarden 1.37+); it comes from the sync profile.
    let user_id = client.sync(&access_token).await?.user_id;
    Ok((access_token, vault_keys, user_id))
}

/// Create a login cipher that carries its own item-level key, the shape the
/// official clients produce: `Key` is the item key encrypted with the user
/// key, and `name`/`username` are encrypted with the item key. It also carries
/// a password-history entry, which the server replaces wholesale on every
/// `PUT` — the same way it does with the item key.
async fn create_item_key_cipher(
    url: &str,
    access_token: &str,
    user_id: &str,
    vault_keys: &locked::Keys,
    item_keys: &locked::Keys,
) -> Result<String> {
    let protected_item_key =
        CipherString::encrypt_symmetric(vault_keys, item_keys.data())?.to_string();

    let res = reqwest::Client::new()
        .post(format!("{url}/api/ciphers"))
        .bearer_auth(access_token)
        .json(&serde_json::json!({
            "type": 1,
            "encryptedFor": user_id,
            "name": CipherString::encrypt_symmetric(item_keys, b"Keyed Entry")?.to_string(),
            "key": protected_item_key,
            "favorite": false,
            "reprompt": 0,
            "notes": null,
            "login": {
                "username": CipherString::encrypt_symmetric(item_keys, b"before")?.to_string(),
                "password": CipherString::encrypt_symmetric(item_keys, b"pw")?.to_string(),
                "totp": null,
                "uris": [{
                    "uri": CipherString::encrypt_symmetric(item_keys, b"https://keyed.example")?.to_string(),
                    "match": null,
                }],
            },
            "fields": [],
            "passwordHistory": [{
                "lastUsedDate": "2026-01-01T00:00:00Z",
                "password": CipherString::encrypt_symmetric(item_keys, b"older-pw")?.to_string(),
            }],
        }))
        .send()
        .await?;
    if !res.status().is_success() {
        anyhow::bail!("creating the item-key cipher failed: {}", res.status());
    }
    let body: serde_json::Value = res.json().await?;
    body["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("server returned no id"))
}

/// Read the stored cipher straight from the server, so the assertions cover
/// what was actually persisted rather than what the agent cached.
async fn server_cipher(url: &str, access_token: &str, id: &str) -> Result<serde_json::Value> {
    let res = reqwest::Client::new()
        .get(format!("{url}/api/ciphers/{id}"))
        .bearer_auth(access_token)
        .send()
        .await?;
    anyhow::ensure!(res.status().is_success(), "GET cipher: {}", res.status());
    Ok(res.json().await?)
}

async fn get_full_entry(client: &AgentClient, id: &str) -> Result<Entry> {
    match client
        .send(Action::GetEntry {
            id: id.to_string(),
            password: None,
        })
        .await?
    {
        Response::Entry { entry } => Ok(entry),
        other => anyhow::bail!("expected Entry from GetEntry, got {other:?}"),
    }
}

/// Regression for entry `33d63576-fb96-4c0e-baa4-b46700e7ac59` in a live vault:
/// after an edit through the UI's own save mapping, the entry's name — and
/// every other field — came back as the raw base64 cipherstring and the agent
/// logged `failed to decrypt 'name': invalid mac`.
///
/// The cipher carried an item-level key (`Cipher.Key`), so the update encrypted
/// its fields with that key, but `PUT /ciphers/{id}` was sent without a `key`
/// field and replaces the cipher wholesale — the server dropped the key, and
/// nothing could decrypt the fields it had just written. Items created by the
/// official Bitwarden clients carry such a key; the entries this suite used to
/// build itself (via `AddEntry`) never did, which is why the gap was invisible.
#[tokio::test]
async fn test_ui_edit_of_item_key_entry_keeps_it_decryptable() -> Result<()> {
    let env = setup_env().await?;
    let _profile = crate::state_guard::ProfileEnv::set(&env.profile);

    let email = "ui-item-key@example.com";
    let password = "uiitemkeypassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, email, password, &env.vault_url).await?;

    let (access_token, vault_keys, user_id) =
        derive_vault_keys(&env.vault_url, email, password).await?;

    let mut item_key_bytes = locked::Vec::new();
    item_key_bytes.extend((0u8..64).map(|i| i.wrapping_mul(7).wrapping_add(3)));
    let item_keys = locked::Keys::new(item_key_bytes);

    let id = create_item_key_cipher(
        &env.vault_url,
        &access_token,
        &user_id,
        &vault_keys,
        &item_keys,
    )
    .await?;

    client.send(Action::Sync).await?;

    // Before the edit the agent reads the item through its item key.
    let before = get_full_entry(&client, &id).await?;
    anyhow::ensure!(
        before.name == "Keyed Entry",
        "the agent must decrypt a keyed item before the edit, got name {:?}",
        before.name
    );
    anyhow::ensure!(
        before.key.is_some(),
        "the synced entry must carry the item key"
    );

    // The user's flow: open the entry, type into a field, press Save. The
    // action comes from the UI's own mapping, not from this test.
    let mut edited = before.clone();
    match &mut edited.data {
        EntryData::Login { username, .. } => *username = Some("after".to_string()),
        other => anyhow::bail!("expected a login, got {:?}", other),
    }
    edited.name = "Keyed Entry Edited".to_string();
    let action = save_action(edited);
    anyhow::ensure!(
        matches!(action, Action::UpdateEntry { .. }),
        "a server-backed entry must map to an update, got {}",
        action.variant_name()
    );
    if let Response::Error { message } = client.send(action).await? {
        anyhow::bail!("server rejected the UI's edit action: {}", message);
    }

    client.send(Action::Sync).await?;

    // The edited fields must still decrypt, and the stored cipher must still
    // carry its item key.
    let after = get_full_entry(&client, &id).await?;
    anyhow::ensure!(
        after.name == "Keyed Entry Edited",
        "the name must decrypt after the edit, got {:?}",
        after.name
    );
    match &after.data {
        EntryData::Login { username, .. } => anyhow::ensure!(
            username.as_deref() == Some("after"),
            "the username must decrypt after the edit, got {:?}",
            username
        ),
        other => anyhow::bail!("expected a login, got {:?}", other),
    }
    anyhow::ensure!(
        after.key.is_some(),
        "the item key must survive the edit (dropping it makes the entry unreadable)"
    );

    // The password history the cipher arrived with must survive the edit too:
    // sending an empty list replaces the stored one with nothing.
    anyhow::ensure!(
        after.history.len() == 1,
        "the item's password history must survive the edit, got {} entries",
        after.history.len()
    );

    let stored = server_cipher(&env.vault_url, &access_token, &id).await?;
    let stored_key = stored.get("key").or_else(|| stored.get("Key"));
    anyhow::ensure!(
        stored_key.is_some_and(serde_json::Value::is_string),
        "PUT without the item key cleared it on the server: {stored_key:?}"
    );
    // And the stored fields decrypt with the item key, independently of the agent.
    let stored_name = stored
        .get("name")
        .or_else(|| stored.get("Name"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("no stored name"))?;
    anyhow::ensure!(
        CipherString::new(stored_name)?.decrypt_symmetric(&vault_keys, Some(&item_keys))?
            == b"Keyed Entry Edited",
        "the stored name must be encrypted with the item key"
    );

    let stored_history = stored
        .get("passwordHistory")
        .or_else(|| stored.get("PasswordHistory"))
        .and_then(|v| v.as_array())
        .map_or(0, Vec::len);
    anyhow::ensure!(
        stored_history == 1,
        "PUT wiped the stored password history ({stored_history} entries left)"
    );

    Ok(())
}
