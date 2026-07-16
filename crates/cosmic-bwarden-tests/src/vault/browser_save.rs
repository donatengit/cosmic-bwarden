//! E2E coverage for the browser save-prompt flow: AddEntry with uris/totp
//! surviving the server round-trip, CheckLoginMatch decisions, and
//! UpdateLoginPassword preserving every other field (the merge/redaction trap).

use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};

#[tokio::test]
async fn test_browser_save_flow() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "browser-save@example.com";
    let password = "browserpassword123";

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

    // 1. Create a Login the way the extension's save bar does: name = domain,
    // origin URI, plus notes and totp to prove UpdateLoginPassword can't wipe them.
    let res = client
        .send(Action::AddEntry {
            name: "example.com".to_string(),
            entry_type: cosmic_bwarden_core::protocol::EntryType::Login,
            username: Some("webuser".to_string()),
            password: Some("webpass1".to_string().into()),
            notes: Some("keep me".into()),
            fields: Vec::new(),
            totp: Some("JBSWY3DPEHPK3PXP".to_string().into()),
            uris: vec![cosmic_bwarden_core::db::Uri {
                uri: "https://example.com".to_string(),
                match_type: None,
            }],
        })
        .await?;
    assert!(matches!(res, Response::Ack), "AddEntry failed: {res:?}");

    client.send(Action::Sync).await?;

    // 2. URIs are metadata: they must survive the server round-trip and be
    // visible in bulk (redacted) reads, or domain matching can never work.
    let res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let id = if let Response::Entries { entries } = res {
        let entry = entries
            .iter()
            .find(|e| e.name == "example.com")
            .expect("saved login not found");
        if let cosmic_bwarden_core::db::EntryData::Login { uris, .. } = &entry.data {
            assert_eq!(uris.len(), 1, "uri dropped on create round-trip");
            assert_eq!(uris[0].uri, "https://example.com");
        } else {
            anyhow::bail!("Expected Login data");
        }
        entry.id.clone()
    } else {
        anyhow::bail!("Expected entries");
    };

    // 3. TOTP is a secret: check it via the full (non-redacted) read.
    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    if let Response::Entry { entry } = res {
        if let cosmic_bwarden_core::db::EntryData::Login { totp, .. } = &entry.data {
            assert_eq!(totp.as_deref(), Some("JBSWY3DPEHPK3PXP"));
        } else {
            anyhow::bail!("Expected Login data");
        }
    } else {
        anyhow::bail!("Expected entry");
    }

    // 4. CheckLoginMatch: same credential → silent (password_matches).
    let res = client
        .send(Action::CheckLoginMatch {
            domain: "example.com".to_string(),
            username: "webuser".to_string(),
            password: "webpass1".to_string(),
        })
        .await?;
    if let Response::LoginMatch {
        entry_id,
        password_matches,
        ..
    } = res
    {
        assert_eq!(entry_id.as_deref(), Some(id.as_str()));
        assert!(password_matches, "identical password must match");
    } else {
        anyhow::bail!("Expected LoginMatch");
    }

    // 5. CheckLoginMatch: changed password → update candidate.
    let res = client
        .send(Action::CheckLoginMatch {
            domain: "example.com".to_string(),
            username: "webuser".to_string(),
            password: "webpass2-changed".to_string(),
        })
        .await?;
    if let Response::LoginMatch {
        entry_id,
        name,
        password_matches,
    } = res
    {
        assert_eq!(entry_id.as_deref(), Some(id.as_str()));
        assert_eq!(name.as_deref(), Some("example.com"));
        assert!(!password_matches);
    } else {
        anyhow::bail!("Expected LoginMatch");
    }

    // 6. CheckLoginMatch: unknown domain → no match (save-new candidate).
    let res = client
        .send(Action::CheckLoginMatch {
            domain: "other-site.net".to_string(),
            username: "webuser".to_string(),
            password: "webpass1".to_string(),
        })
        .await?;
    if let Response::LoginMatch { entry_id, .. } = res {
        assert!(entry_id.is_none(), "unrelated domain must not match");
    } else {
        anyhow::bail!("Expected LoginMatch");
    }

    // 7. UpdateLoginPassword: swaps the password and must preserve notes,
    // totp, and uris (regression guard for the redact/merge notes trap).
    let res = client
        .send(Action::UpdateLoginPassword {
            id: id.clone(),
            password: "webpass2-changed".to_string(),
        })
        .await?;
    assert!(
        matches!(res, Response::Ack),
        "UpdateLoginPassword failed: {res:?}"
    );

    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    if let Response::Entry { entry } = res {
        assert_eq!(
            entry.notes.as_deref(),
            Some("keep me"),
            "notes wiped by password update"
        );
        if let cosmic_bwarden_core::db::EntryData::Login {
            password,
            totp,
            uris,
            username,
        } = &entry.data
        {
            assert_eq!(password.as_deref(), Some("webpass2-changed"));
            assert_eq!(totp.as_deref(), Some("JBSWY3DPEHPK3PXP"));
            assert_eq!(username.as_deref(), Some("webuser"));
            assert_eq!(uris.len(), 1);
            assert_eq!(uris[0].uri, "https://example.com");
        } else {
            anyhow::bail!("Expected Login data");
        }
    } else {
        anyhow::bail!("Expected entry");
    }

    // 8. After the update, the same submit is silent again.
    let res = client
        .send(Action::CheckLoginMatch {
            domain: "example.com".to_string(),
            username: "webuser".to_string(),
            password: "webpass2-changed".to_string(),
        })
        .await?;
    if let Response::LoginMatch {
        password_matches, ..
    } = res
    {
        assert!(password_matches);
    } else {
        anyhow::bail!("Expected LoginMatch");
    }

    Ok(())
}
