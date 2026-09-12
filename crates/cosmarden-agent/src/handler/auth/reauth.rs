//! Restore a usable server session after an unlock.
//!
//! An unlock recovers the vault keys but never a session token: tokens are
//! `#[serde(skip)]`, so they die with the agent, and `State::lock()` drops them
//! deliberately. Something has to re-mint one, and this module owns the choice.
//!
//! Sources are tried weakest-credential first:
//!
//! 1. **The stored refresh-token envelope** (`session_store`) — a refresh grant
//!    is revocable server-side, expires on its own, cannot change the account,
//!    and never has to satisfy 2FA (Vaultwarden's `refresh_login` bypasses the
//!    two-factor path entirely). This is the preferred source, and the *only*
//!    one a PIN unlock or a plain sync has.
//! 2. **A full password grant**, from a hash the caller derived moments ago out
//!    of a password the user just typed. Never from storage — nothing on this
//!    device keeps a master-password hash — so only the master-password unlock
//!    path can supply one, and it fails closed on a 2FA-protected account.
//!
//! Every surface shares this one definition: both unlock paths and
//! `server::auth::with_refresh`, which runs it when a request finds no token in
//! memory. Hand-written copies is how they would drift, and the PIN path — the
//! one with no fallback — is where a divergence stays invisible until a reboot.
//!
//! If no source works the vault stays unlocked and usable offline; only server
//! sync is lost, and the caller says so loudly with the reason attached.

use crate::keyring;
use crate::session_store;
use crate::state::State;
use cosmarden_core::config::CosmardenConfig;
use cosmarden_core::error::Error;
use cosmarden_core::locked;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Turn a failed password grant into something the user can act on. A 2FA or
/// new-device challenge is not a malfunction — it is the server correctly
/// refusing a silent login — and must not read as a generic sync failure.
fn describe_login_failure(e: &Error) -> String {
    match e {
        Error::TwoFactorRequired { .. } => "two-factor authentication is required, which a \
             silent re-auth cannot satisfy — unlock with your master password to restore sync"
            .to_string(),
        Error::NewDeviceVerificationRequired => "this device needs verification before it can \
             log in — unlock with your master password to restore sync"
            .to_string(),
        other => other.to_string(),
    }
}

/// Store a freshly minted token pair in `state`, mirror it to the keyring when
/// the user asked for session persistence, and roll the on-disk envelope
/// forward so the next restart starts from the newest refresh token.
async fn adopt_tokens(
    state: &Arc<Mutex<State>>,
    config: &CosmardenConfig,
    email: &str,
    access_token: String,
    refresh_token: Option<String>,
) {
    let server = config.server_name();

    if config.persist_session {
        if let Some(rt) = &refresh_token {
            if let Err(e) = keyring::store_tokens(&server, email, &access_token, rt).await {
                log::error!("failed to store refreshed tokens in keyring: {}", e);
            }
        }
    }

    {
        let mut g = state.lock().await;
        if let Some(db) = &mut g.db {
            db.access_token = Some(access_token.into());
            if let Some(rt) = refresh_token {
                db.refresh_token = Some(rt.into());
            }
        }
    }
    session_store::persist_current(state, &server, email).await;
}

/// Exchange the stored refresh token for a live session. `Ok(false)` means
/// there was nothing stored; `Err` means there was, and it did not work.
async fn try_stored_refresh_token(
    state: &Arc<Mutex<State>>,
    config: &CosmardenConfig,
    email: &str,
    keys: &locked::Keys,
) -> anyhow::Result<bool> {
    let Some(refresh_token) = session_store::load(keys, &config.server_name(), email)? else {
        return Ok(false);
    };

    let client = cosmarden_core::api::Client::new(&config.base_url(), &config.identity_url());
    let (access_token, new_refresh, _key) = client
        .exchange_refresh_token(refresh_token.expose())
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    adopt_tokens(state, config, email, access_token, new_refresh).await;
    log::info!("unlock: restored the server session from the stored refresh token");
    Ok(true)
}

/// Full password grant, using a hash the caller derived moments ago from a
/// password the user just typed. Never from storage: nothing on this device
/// keeps a master-password hash, so only the master-password unlock path can
/// supply one. A PIN unlock passes `None` and falls back to prompting.
async fn try_password_grant(
    state: &Arc<Mutex<State>>,
    config: &CosmardenConfig,
    email: &str,
    master_password_hash: &locked::PasswordHash,
) -> anyhow::Result<()> {
    let device_id = config
        .device_id()
        .await
        .map_err(|e| anyhow::anyhow!("could not obtain device_id: {e}"))?;

    let client = cosmarden_core::api::Client::new(&config.base_url(), &config.identity_url());
    let (access_token, refresh_token, _key) = client
        .login(
            email,
            &device_id,
            master_password_hash,
            None,
            None,
            None,
            None,
        )
        .await
        .map_err(|e| anyhow::anyhow!("{}", describe_login_failure(&e)))?;

    adopt_tokens(state, config, email, access_token, refresh_token).await;
    log::info!("unlock: silent re-auth succeeded via the just-entered master password");
    Ok(())
}

/// Try every available source in order. `Err` carries a user-facing reason —
/// a 2FA challenge and an unreachable server need different answers from the
/// user, so the caller must not flatten them into one generic message.
pub(crate) async fn restore_session(
    state: &Arc<Mutex<State>>,
    config: &CosmardenConfig,
    email: &str,
    keys: &locked::Keys,
    master_password_hash: Option<&locked::PasswordHash>,
) -> Result<(), String> {
    let refresh_failure = match try_stored_refresh_token(state, config, email, keys).await {
        Ok(true) => return Ok(()),
        Ok(false) => {
            log::info!("unlock: no stored refresh token for {email}");
            None
        }
        Err(e) => {
            // Expired (past the server's refresh window), revoked, or written
            // by a different account/key. Recoverable only if the caller just
            // took a master password; never silent — this is the primary path.
            log::error!("unlock: stored refresh token unusable: {e:#}");
            Some(format!("{e:#}"))
        }
    };

    match master_password_hash {
        Some(hash) => match try_password_grant(state, config, email, hash).await {
            Ok(()) => Ok(()),
            Err(e) => {
                log::error!("unlock: silent re-auth failed (sync unavailable): {e:#}");
                Err(format!("{e:#}"))
            }
        },
        None => Err(refresh_failure.unwrap_or_else(|| {
            "no saved session on this device — unlock with your master password to restore sync"
                .to_string()
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::describe_login_failure;
    use cosmarden_core::error::Error;

    /// A 2FA challenge is the server working as intended; the message must send
    /// the user to a master-password unlock, not read as a server malfunction.
    #[test]
    fn two_factor_failure_names_the_recovery() {
        let msg = describe_login_failure(&Error::TwoFactorRequired {
            providers: vec![0],
            token: "t".to_string(),
        });
        assert!(msg.contains("two-factor"), "{msg}");
        assert!(msg.contains("master password"), "{msg}");
    }

    #[test]
    fn new_device_failure_names_the_recovery() {
        let msg = describe_login_failure(&Error::NewDeviceVerificationRequired);
        assert!(msg.contains("verification"), "{msg}");
        assert!(msg.contains("master password"), "{msg}");
    }

    /// The master-password hash must never reach durable storage: it is derived
    /// per unlock, handed to `login`, and dropped. This pins the whole chain —
    /// no `State` field, no TPM blob, no session-envelope round trip — so a
    /// future "cache it to skip the KDF" change has to delete this test first.
    #[test]
    fn no_master_password_hash_is_ever_persisted() {
        let field = ["master", "_password_", "hash"].concat();
        for (name, src) in [
            ("state.rs", include_str!("../../state.rs")),
            ("session_store.rs", include_str!("../../session_store.rs")),
            (
                "session_envelope.rs",
                include_str!("../../../../cosmarden-core/src/session_envelope.rs"),
            ),
        ] {
            assert!(
                !src.contains(&field),
                "{name} must not reference the master-password hash"
            );
        }
        // The only sealed TPM object is the vault keys; a hash blob path would
        // reintroduce the artifact we just removed.
        let dirs = include_str!("../../../../cosmarden-core/src/dirs.rs");
        assert!(
            !dirs.contains("tpm_sealed_hash"),
            "no per-account master-password-hash blob may exist"
        );
    }

    /// Everything else keeps its own text — don't paper a network error over
    /// with unrelated 2FA advice.
    #[test]
    fn other_failures_pass_through_unchanged() {
        let msg = describe_login_failure(&Error::Other("connection refused".to_string()));
        assert!(msg.contains("connection refused"), "{msg}");
        assert!(
            !msg.contains("master password"),
            "an unrelated failure must not suggest the 2FA recovery: {msg}"
        );
    }
}
