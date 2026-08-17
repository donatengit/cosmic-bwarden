//! Lifecycle of the agent's out-of-sync flag (`sync_failed` in `GetConfig`).
//!
//! The flag is deliberately sticky across lock/unlock cycles — a vault that
//! failed to sync stays out of sync until a sync actually succeeds. What it
//! must NOT do is survive the operations that provably resync the vault:
//! a full password login performs an initial server sync and replaces the
//! local DB, so a stale flag from a previous session (e.g. a degraded PIN
//! unlock with no session token) has to be cleared there.

use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, EntryType, Response};

async fn login(client: &AgentClient, email: &str, password: &str, url: &str) -> Result<()> {
    let res = client
        .send(Action::Login {
            email: email.to_string(),
            password: password.to_string(),
            server_url: Some(url.to_string()),
            remember_me: false,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;
    assert!(matches!(res, Response::Ack), "Login failed: {:?}", res);
    Ok(())
}

async fn config_sync_failed(client: &AgentClient) -> Result<(bool, bool)> {
    let res = client.send(Action::GetConfig).await?;
    match res {
        Response::Config {
            sync_failed,
            is_locked,
            ..
        } => Ok((sync_failed, is_locked)),
        other => anyhow::bail!("expected Config, got {:?}", other),
    }
}

/// Regression for the reported flow: a session without server tokens (like a
/// degraded PIN unlock) marks the vault out of sync; logging in again with
/// the master password syncs the vault with fresh server state — the
/// "Not synced" badge must not survive that login, and the previously
/// failing operation must work again.
///
/// `remember_me: false` keeps tokens out of the keyring so that locking the
/// vault leaves no way to restore a session token — the same condition the
/// TPM degraded-unlock test produces (`test_pin_unlock_without_server_credentials_sync_fails`).
#[tokio::test]
async fn test_login_clears_stale_out_of_sync_flag() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "sync-relogin@example.com";
    let password = "syncrelogin123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, email, password, &env.vault_url).await?;

    // Lock, then force a server sync while locked: without a token it fails
    // honestly and sets the out-of-sync flag.
    let res = client.send(Action::Lock).await?;
    assert!(matches!(res, Response::Ack), "Lock failed: {:?}", res);
    let res = client.send(Action::Sync).await?;
    assert!(
        matches!(res, Response::Error { .. }),
        "sync on a locked vault without a token must fail: {:?}",
        res
    );
    let (sync_failed, is_locked) = config_sync_failed(&client).await?;
    assert!(is_locked, "vault must still be locked");
    assert!(sync_failed, "failed sync must set the out-of-sync flag");

    // Full password login: the initial sync inside the login replaces the
    // local vault with server state, so the flag must be cleared.
    login(&client, email, password, &env.vault_url).await?;

    let (sync_failed, is_locked) = config_sync_failed(&client).await?;
    assert!(!is_locked, "vault must be unlocked after login");
    assert!(
        !sync_failed,
        "successful login must clear the stale out-of-sync flag"
    );

    // And the operation that originally failed must now go through
    // end-to-end (the user's "add a secure note" case).
    let res = client
        .send(Action::AddEntry {
            name: "After Re-login Note".to_string(),
            entry_type: EntryType::SecureNote,
            username: None,
            password: None,
            notes: Some("note after relogin".into()),
            fields: Vec::new(),
            totp: None,
            uris: Vec::new(),
        })
        .await?;
    assert!(
        matches!(res, Response::Ack),
        "AddEntry must succeed after re-login: {:?}",
        res
    );

    Ok(())
}

/// Logout tears the account down; the out-of-sync flag must not leak from
/// the previous session into the next login's first config fetch.
#[tokio::test]
async fn test_logout_clears_out_of_sync_flag() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "sync-relogout@example.com";
    let password = "syncrelogout123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, email, password, &env.vault_url).await?;

    // Same degraded condition as the login test: locked + no token.
    let res = client.send(Action::Lock).await?;
    assert!(matches!(res, Response::Ack), "Lock failed: {:?}", res);
    let res = client.send(Action::Sync).await?;
    assert!(
        matches!(res, Response::Error { .. }),
        "sync on a locked vault without a token must fail: {:?}",
        res
    );
    let (sync_failed, _) = config_sync_failed(&client).await?;
    assert!(sync_failed, "failed sync must set the out-of-sync flag");

    let res = client.send(Action::Logout).await?;
    assert!(matches!(res, Response::Ack), "Logout failed: {:?}", res);

    let (sync_failed, is_locked) = config_sync_failed(&client).await?;
    assert!(
        !sync_failed,
        "logout must not leak the previous account's out-of-sync flag"
    );
    assert!(is_locked, "no account after logout means locked");

    Ok(())
}
