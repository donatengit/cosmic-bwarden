//! The encrypted session envelope: the refresh token that survives an agent
//! restart so an unlock can restore server sync without a password grant.
//!
//! These assert against a live Vaultwarden, so they cover the parts unit tests
//! cannot: that the refresh grant this design depends on is actually accepted,
//! and that the envelope — not the master-password fallback — is what carries
//! the session across a restart.

use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};
use tokio::time::{sleep, Duration};

/// Path of the agent's session envelope. Resolved against the agent's
/// XDG_DATA_HOME and profile, via core's own `account_hash` so the test and
/// the agent cannot drift apart on the naming scheme.
fn envelope_path(env: &crate::common::TestEnv, email: &str) -> std::path::PathBuf {
    env.data_home
        .join(format!("cosmic-bwarden-{}", env.profile))
        .join(format!(
            "session_{}.enc",
            cosmic_bwarden_core::dirs::account_hash(&env.vault_url, email)
        ))
}

/// A login persists an encrypted refresh token; after an agent restart the
/// unlock restores the session from it rather than falling back to a full
/// password grant; and logout removes it.
///
/// The path assertion reads the agent log, which `setup_env` truncates per
/// test — sound only because this suite runs with `--test-threads=1`.
#[tokio::test]
async fn test_session_envelope_carries_the_session_across_a_restart() -> Result<()> {
    let mut env = setup_env().await?;
    let _profile = crate::state_guard::ProfileEnv::set(&env.profile);

    let email = "session-envelope@example.com";
    let password = "envelopepassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    let res = client
        .send(Action::Login {
            email: email.to_string(),
            password: password.to_string(),
            server_url: Some(env.vault_url.clone()),
            // No keyring backup: the envelope has to carry this alone.
            remember_me: false,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;
    assert!(matches!(res, Response::Ack), "Login failed: {:?}", res);

    // Login must have written the envelope, and the token must not be in it in
    // the clear — a Vaultwarden refresh JWT always starts "eyJ".
    let path = envelope_path(&env, email);
    let raw = std::fs::read(&path)
        .map_err(|e| anyhow::anyhow!("login must persist {}: {e}", path.display()))?;
    assert!(
        !raw.windows(3).any(|w| w == b"eyJ"),
        "a JWT prefix appeared in {} — the envelope is not encrypted",
        path.display()
    );
    use std::os::unix::fs::PermissionsExt as _;
    let mode = std::fs::metadata(&path)?.permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "session envelope must not be group/world readable"
    );

    // Restart the agent: a fresh process with no in-memory tokens.
    if let Some(mut proc) = env.agent_process.take() {
        proc.kill()?;
        // Reap before restarting: see pinned_ops.rs.
        let _ = proc.wait();
    }
    let agent_process = env.start_agent()?;
    sleep(Duration::from_millis(1000)).await;
    env.agent_process = Some(agent_process);

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    let res = client
        .send(Action::Unlock {
            password: password.to_string(),
        })
        .await?;
    assert!(matches!(res, Response::Ack), "Unlock failed: {:?}", res);

    // The session must come from the stored refresh token. Without this the
    // test would pass on the password-grant fallback and prove nothing.
    let log = std::fs::read_to_string(&env.log_path)?;
    assert!(
        log.contains("restored the server session from the stored refresh token"),
        "unlock should have used the stored refresh token; agent log:\n{log}"
    );

    // And the restored session must actually work against the server.
    let sync_res = client.send(Action::Sync).await?;
    assert!(
        matches!(sync_res, Response::Ack),
        "Sync should succeed on the restored session: {:?}",
        sync_res
    );

    // Logout must not leave a usable refresh token behind on disk.
    let res = client.send(Action::Logout).await?;
    assert!(matches!(res, Response::Ack), "Logout failed: {:?}", res);
    assert!(
        !path.exists(),
        "logout must remove the session envelope at {}",
        path.display()
    );

    Ok(())
}

/// An unusable envelope (corrupt, or written under different vault keys) must
/// not strand the unlock: it falls through to the password grant and sync still
/// works. Guards the fallback ordering in `handler::auth::reauth`.
#[tokio::test]
async fn test_corrupt_envelope_falls_back_to_the_password_grant() -> Result<()> {
    let mut env = setup_env().await?;
    let _profile = crate::state_guard::ProfileEnv::set(&env.profile);

    let email = "session-envelope-corrupt@example.com";
    let password = "envelopepassword456";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    client
        .send(Action::Login {
            email: email.to_string(),
            password: password.to_string(),
            server_url: Some(env.vault_url.clone()),
            remember_me: false,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    // Keep the version byte so the file is well-formed enough to reach the
    // AEAD, and let the tag check be what rejects it.
    let path = envelope_path(&env, email);
    let mut raw = std::fs::read(&path)?;
    let last = raw.len() - 1;
    raw[last] ^= 0xff;
    std::fs::write(&path, &raw)?;

    if let Some(mut proc) = env.agent_process.take() {
        proc.kill()?;
        // Reap before restarting: see pinned_ops.rs.
        let _ = proc.wait();
    }
    let agent_process = env.start_agent()?;
    sleep(Duration::from_millis(1000)).await;
    env.agent_process = Some(agent_process);

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    let res = client
        .send(Action::Unlock {
            password: password.to_string(),
        })
        .await?;
    assert!(
        matches!(res, Response::Ack),
        "a corrupt envelope must not break unlock: {:?}",
        res
    );

    let sync_res = client.send(Action::Sync).await?;
    assert!(
        matches!(sync_res, Response::Ack),
        "Sync should succeed via the password-grant fallback: {:?}",
        sync_res
    );

    // The failure must be loud, not swallowed — a silently ignored envelope is
    // how this degrades back into "PIN unlock has no session".
    let log = std::fs::read_to_string(&env.log_path)?;
    assert!(
        log.contains("stored refresh token unusable"),
        "an unusable envelope must be logged at error level; agent log:\n{log}"
    );

    // The successful re-auth must have rewritten a good envelope.
    let repaired = std::fs::read(&path)?;
    assert_ne!(
        repaired, raw,
        "a successful re-auth must rewrite the envelope"
    );

    Ok(())
}
