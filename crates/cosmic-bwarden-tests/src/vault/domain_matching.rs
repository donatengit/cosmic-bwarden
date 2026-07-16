/// E2E tests for `GetSidebarEntries`' `domain` filter: the browser popup's
/// tab-host matching (exact / boundary-subdomain / PSL eTLD+1) against entry
/// URI hosts, including the co.uk non-bridging guarantee. Matching-rule unit
/// tests live in core's `domain` module; this exercises the full agent path
/// (encrypted URIs → sidebar host cache → filter).
use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::db::Uri;
use cosmic_bwarden_core::protocol::{Action, EntryType, Response};

async fn add_login(client: &AgentClient, name: &str, uris: Vec<Uri>) -> Result<()> {
    client
        .send(Action::AddEntry {
            name: name.to_string(),
            entry_type: EntryType::Login,
            username: Some("user".to_string()),
            password: Some("pass123".to_string().into()),
            notes: None,
            fields: Vec::new(),
            totp: None,
            uris,
        })
        .await?;
    Ok(())
}

async fn names_for_domain(client: &AgentClient, domain: &str) -> Result<Vec<String>> {
    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
            domain: Some(domain.to_string()),
        })
        .await?;
    match res {
        Response::SidebarEntries { entries } => {
            let mut names: Vec<String> = entries.into_iter().map(|e| e.name).collect();
            names.sort();
            Ok(names)
        }
        r => anyhow::bail!("expected SidebarEntries, got {r:?}"),
    }
}

#[tokio::test]
async fn test_sidebar_domain_filter() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "domain-match@example.com";
    let password = "domainpassword123";
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

    let uri = |u: &str| Uri {
        uri: u.to_string(),
        match_type: None,
    };
    add_login(&client, "Facebook", vec![uri("https://facebook.com")]).await?;
    add_login(&client, "Bank", vec![uri("https://mybank.co.uk")]).await?;
    add_login(&client, "Google", vec![uri("https://accounts.google.com")]).await?;
    // Legacy shape: no URIs, hostname as name (the save bar's old convention).
    add_login(&client, "iqos.ru", Vec::new()).await?;
    client.send(Action::Sync).await?;

    // Page on a subdomain surfaces the apex-host entry (the original UX gap).
    assert_eq!(names_for_domain(&client, "account.facebook.com").await?, ["Facebook"]);
    assert_eq!(names_for_domain(&client, "facebook.com").await?, ["Facebook"]);

    // Boundary check: look-alike hosts never match.
    assert!(names_for_domain(&client, "notfacebook.com").await?.is_empty());
    assert!(names_for_domain(&client, "facebook.com.evil.net").await?.is_empty());

    // Multi-label public suffix never bridges unrelated sites.
    assert_eq!(names_for_domain(&client, "mybank.co.uk").await?, ["Bank"]);
    assert!(names_for_domain(&client, "evil.co.uk").await?.is_empty());

    // Sibling subdomains match via the PSL eTLD+1 rule (default build).
    assert_eq!(names_for_domain(&client, "mail.google.com").await?, ["Google"]);

    // Legacy name-only entry matches its own host, exact and subdomain.
    assert_eq!(names_for_domain(&client, "iqos.ru").await?, ["iqos.ru"]);
    assert_eq!(names_for_domain(&client, "shop.iqos.ru").await?, ["iqos.ru"]);

    // A typed query wins over domain (popup contract).
    let res = client
        .send(Action::GetSidebarEntries {
            query: Some("Bank".to_string()),
            entry_type: None,
            only_pinned: false,
            domain: Some("facebook.com".to_string()),
        })
        .await?;
    match res {
        Response::SidebarEntries { entries } => {
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].name, "Bank");
        }
        r => anyhow::bail!("expected SidebarEntries, got {r:?}"),
    }

    Ok(())
}
