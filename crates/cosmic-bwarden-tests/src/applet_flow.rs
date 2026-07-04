use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
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

// Bug (f): after Logout the agent must clear its in-memory Db so that the
// next GetConfig correctly reports has_account=false. Before the fix the Db
// was only cleared on disk; the stale in-memory record caused GetConfig to
// keep reporting has_account=true, leaving the applet stuck on the Unlock
// screen instead of the Login/Setup screen.
#[tokio::test]
async fn test_logout_clears_account_state() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "logout-state@example.com";
    let password = "logoutpassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    let res = client.send(Action::GetConfig).await?;
    if let Response::Config {
        has_account,
        is_locked,
        ..
    } = res
    {
        assert!(has_account, "has_account must be true after login");
        assert!(!is_locked, "vault must be unlocked after login");
    } else {
        anyhow::bail!("Expected Config response after login");
    }

    let res = client.send(Action::Logout).await?;
    assert!(matches!(res, Response::Ack), "Logout must respond Ack");

    // The critical assertion: in-memory Db must be cleared so GetConfig
    // loads the empty on-disk record and returns has_account=false.
    let res = client.send(Action::GetConfig).await?;
    if let Response::Config {
        has_account,
        needs_login,
        ..
    } = res
    {
        assert!(
            !has_account,
            "has_account must be false after logout (in-memory db must be cleared)"
        );
        assert!(needs_login, "needs_login must be true after logout");
    } else {
        anyhow::bail!("Expected Config response after logout");
    }

    Ok(())
}

// Lock must report is_locked=true without clearing the account, and Unlock
// must restore full access — entries visible immediately without re-login.
#[tokio::test]
async fn test_lock_preserves_account_unlock_restores_entries() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "lock-restore@example.com";
    let password = "lockrestorepass123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    client
        .send(Action::AddEntry {
            name: "Restore Test".to_string(),
            entry_type: EntryType::Login,
            username: Some("restoreuser".to_string()),
            password: Some("restorepass".to_string().into()),
            notes: None,
            fields: Vec::new(),
        })
        .await?;
    client.send(Action::Sync).await?;

    client.send(Action::Lock).await?;

    // has_account stays true (only keys are cleared on lock, not the account record)
    let res = client.send(Action::GetConfig).await?;
    if let Response::Config {
        has_account,
        is_locked,
        ..
    } = res
    {
        assert!(has_account, "has_account must remain true after lock");
        assert!(is_locked, "is_locked must be true after lock");
    } else {
        anyhow::bail!("Expected Config after lock");
    }

    // GetSidebarEntries while locked must return an error, not an empty list
    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    assert!(
        matches!(res, Response::Error { .. }),
        "GetSidebarEntries while locked must return Error"
    );

    let res = client
        .send(Action::Unlock {
            password: password.to_string(),
        })
        .await?;
    assert!(matches!(res, Response::Ack), "Unlock must respond Ack");

    // Entries must be accessible immediately after unlock (no re-login needed)
    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(
            entries.len(),
            1,
            "entry must be visible after unlock without re-login"
        );
        assert_eq!(entries[0].name, "Restore Test");
    } else {
        anyhow::bail!("Expected SidebarEntries after unlock, got {:?}", res);
    }

    Ok(())
}

#[tokio::test]
async fn test_applet_unlock_after_lock() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "applet-unlock@example.com";
    let password = "appletpassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    client.send(Action::Lock).await?;

    let res = client.send(Action::GetConfig).await?;
    if let Response::Config { is_locked, .. } = res {
        assert!(is_locked);
    } else {
        anyhow::bail!("Expected config response");
    }

    let res = client
        .send(Action::Unlock {
            password: password.to_string(),
        })
        .await?;
    assert!(matches!(res, Response::Ack));

    let res = client.send(Action::GetConfig).await?;
    if let Response::Config { is_locked, .. } = res {
        assert!(!is_locked);
    } else {
        anyhow::bail!("Expected config response");
    }

    // Mirrors the applet's post-unlock favourites refresh (AppletUnlockResult).
    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: true,
        })
        .await?;
    assert!(
        matches!(res, Response::SidebarEntries { .. }),
        "Expected sidebar entries, got {:?}",
        res
    );

    Ok(())
}

#[tokio::test]
async fn test_applet_unlock_wrong_password() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "applet-wrongpw@example.com";
    let password = "appletpassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    client.send(Action::Lock).await?;

    let res = client
        .send(Action::Unlock {
            password: "wrong-password".to_string(),
        })
        .await?;
    assert!(matches!(res, Response::Error { .. }));

    Ok(())
}

#[tokio::test]
async fn test_favourites_only_when_query_empty() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "applet-favourites@example.com";
    let password = "appletpassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    for i in 1..=3 {
        client
            .send(Action::AddEntry {
                name: format!("Entry {}", i),
                entry_type: EntryType::Login,
                username: Some(format!("user{}", i)),
                password: Some(format!("pass{}", i).into()),
                notes: None,
                fields: Vec::new(),
            })
            .await?;
    }
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let entries = if let Response::SidebarEntries { entries } = res {
        entries
    } else {
        anyhow::bail!("Expected sidebar entries");
    };
    assert_eq!(entries.len(), 3);

    let id1 = entries
        .iter()
        .find(|e| e.name == "Entry 1")
        .unwrap()
        .id
        .clone();
    client.send(Action::PinEntry { id: id1.clone() }).await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: true,
        })
        .await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, id1);
    } else {
        anyhow::bail!("Expected sidebar entries");
    }

    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    if let Response::SidebarEntries { entries } = res {
        assert_eq!(entries.len(), 3);
    } else {
        anyhow::bail!("Expected sidebar entries");
    }

    Ok(())
}

#[tokio::test]
async fn test_ssh_public_key_in_sidebar_entries() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "applet-ssh@example.com";
    let password = "appletpassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    client
        .send(Action::AddSshKey {
            name: "My SSH Key".to_string(),
            private_key: "PRIVATE KEY CONTENT".to_string().into(),
            public_key: Some("ssh-ed25519 AAAA...".to_string()),
            notes: None,
            fields: Vec::new(),
        })
        .await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    if let Response::SidebarEntries { entries } = res {
        let entry = entries
            .iter()
            .find(|e| e.name == "My SSH Key")
            .expect("ssh entry not found");
        assert_eq!(entry.entry_type, EntryType::SshKey);
        assert_eq!(entry.public_key.as_deref(), Some("ssh-ed25519 AAAA..."));
    } else {
        anyhow::bail!("Expected sidebar entries");
    }

    Ok(())
}

#[tokio::test]
async fn test_get_password_for_secure_note() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "applet-note@example.com";
    let password = "appletpassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    client
        .send(Action::AddSecureNote {
            name: "My Note".to_string(),
            notes: "secret note content".to_string().into(),
            fields: Vec::new(),
        })
        .await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let id = if let Response::SidebarEntries { entries } = res {
        entries
            .iter()
            .find(|e| e.name == "My Note")
            .expect("note entry not found")
            .id
            .clone()
    } else {
        anyhow::bail!("Expected sidebar entries");
    };

    let res = client
        .send(Action::GetPassword { id, password: None })
        .await?;
    if let Response::Password { password } = res {
        assert_eq!(password, "secret note content");
    } else {
        anyhow::bail!("Expected password response, got {:?}", res);
    }

    Ok(())
}

#[tokio::test]
async fn test_get_password_reprompt_flow() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "applet-reprompt@example.com";
    let password = "appletpassword123";

    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    client
        .send(Action::AddEntry {
            name: "Sensitive".to_string(),
            entry_type: EntryType::Login,
            username: Some("user".to_string()),
            password: Some("topsecret".to_string().into()),
            notes: None,
            fields: Vec::new(),
        })
        .await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let mut entry = if let Response::Entries { entries } = res {
        entries
            .into_iter()
            .find(|e| e.name == "Sensitive")
            .expect("entry not found")
    } else {
        anyhow::bail!("Expected entries");
    };

    entry.master_password_reprompt = cosmic_bwarden_core::api::CipherRepromptType::Password;
    client
        .send(Action::UpdateEntry {
            entry: entry.clone(),
        })
        .await?;
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetPassword {
            id: entry.id.clone(),
            password: None,
        })
        .await?;
    assert!(matches!(res, Response::Error { message } if message == "reprompt_required"));

    let res = client
        .send(Action::GetPassword {
            id: entry.id.clone(),
            password: Some("wrong-password".to_string()),
        })
        .await?;
    assert!(matches!(res, Response::Error { .. }));

    let res = client
        .send(Action::GetPassword {
            id: entry.id.clone(),
            password: Some(password.to_string()),
        })
        .await?;
    if let Response::Password { password: p } = res {
        assert_eq!(p, "topsecret");
    } else {
        anyhow::bail!("Expected password response, got {:?}", res);
    }

    Ok(())
}
