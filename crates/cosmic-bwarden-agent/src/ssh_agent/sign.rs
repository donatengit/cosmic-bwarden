use super::identities::cache_has_pubkey;
use crate::state::{State, VaultSession};
use signature::{RandomizedSigner as _, SignatureEncoding as _, Signer as _};
use ssh_agent_lib::proto::SignRequest;
use ssh_agent_lib::ssh_key::{PrivateKey, PublicKey, Signature};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const SSH_AGENT_RSA_SHA2_256: u32 = 2;
const SSH_AGENT_RSA_SHA2_512: u32 = 4;

/// Production bound for a sign request that arrived while the vault is locked.
pub const DEFAULT_SIGN_WAIT: Duration = Duration::from_secs(90);

pub async fn sign_request(
    state: &Arc<Mutex<State>>,
    request: SignRequest,
    wait: Duration,
) -> Result<Signature, ssh_agent_lib::error::AgentError> {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let mut g = state.lock().await;
        if g.keys.is_some() {
            return sign_unlocked(&g, &request);
        }
        g.request_unlock();
        if !cache_has_pubkey(&g.ssh_identity_cache, &request.pubkey) {
            return Err(ssh_agent_lib::error::AgentError::other(
                cosmic_bwarden_core::error::Error::Other("no matching key found".to_string()),
            ));
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return timeout_err();
        }
        let mut rx = g.session_tx.subscribe();
        drop(g);

        let session = match tokio::time::timeout(
            remaining,
            rx.wait_for(|s| matches!(*s, VaultSession::Unlocked | VaultSession::LoggedOut)),
        )
        .await
        {
            Err(_) => return timeout_err(),
            Ok(Err(_)) => {
                return Err(ssh_agent_lib::error::AgentError::other(
                    cosmic_bwarden_core::error::Error::Other("agent is locked".to_string()),
                ));
            }
            Ok(Ok(flag)) => *flag,
        };
        if session == VaultSession::LoggedOut {
            return Err(ssh_agent_lib::error::AgentError::other(
                cosmic_bwarden_core::error::Error::Other("agent is locked".to_string()),
            ));
        }
    }
}

fn timeout_err() -> Result<Signature, ssh_agent_lib::error::AgentError> {
    Err(ssh_agent_lib::error::AgentError::other(
        cosmic_bwarden_core::error::Error::Other("unlock wait timed out".to_string()),
    ))
}

fn sign_unlocked(
    state: &State,
    request: &SignRequest,
) -> Result<Signature, ssh_agent_lib::error::AgentError> {
    let Some(keys) = &state.keys else {
        return Err(ssh_agent_lib::error::AgentError::other(
            cosmic_bwarden_core::error::Error::Other("agent is locked".to_string()),
        ));
    };
    let Some(db) = &state.db else {
        return Err(ssh_agent_lib::error::AgentError::other(
            cosmic_bwarden_core::error::Error::Other("agent is locked".to_string()),
        ));
    };
    let empty_org_keys = std::collections::HashMap::new();
    let org_keys = state.org_keys.as_ref().unwrap_or(&empty_org_keys);

    let req_pubkey = PublicKey::new(request.pubkey.clone(), "");
    let req_bytes = req_pubkey.to_bytes();

    for entry in &db.entries {
        if !matches!(
            entry.data,
            cosmic_bwarden_core::db::EntryData::SshKey { .. }
        ) {
            continue;
        }
        let decrypted = entry.decrypt(keys, org_keys);
        if let cosmic_bwarden_core::db::EntryData::SshKey {
            private_key: Some(sk),
            public_key: Some(pk_str),
            ..
        } = &decrypted.data
        {
            if let Ok(pk) = pk_str.parse::<PublicKey>() {
                if pk.to_bytes() == req_bytes {
                    let sk = PrivateKey::from_openssh(sk.expose())
                        .map_err(ssh_agent_lib::error::AgentError::other)?;
                    return sign_with_key(&sk, &request.data, request.flags);
                }
            }
        }
    }

    Err(ssh_agent_lib::error::AgentError::other(
        cosmic_bwarden_core::error::Error::Other("no matching key found".to_string()),
    ))
}

fn sign_with_key(
    private_key: &PrivateKey,
    data: &[u8],
    flags: u32,
) -> Result<Signature, ssh_agent_lib::error::AgentError> {
    match private_key.key_data() {
        ssh_agent_lib::ssh_key::private::KeypairData::Ed25519(key) => key
            .try_sign(data)
            .map_err(ssh_agent_lib::error::AgentError::other),
        ssh_agent_lib::ssh_key::private::KeypairData::Rsa(key) => {
            let p = rsa::BigUint::from_bytes_be(key.private.p.as_bytes());
            let q = rsa::BigUint::from_bytes_be(key.private.q.as_bytes());
            let e = rsa::BigUint::from_bytes_be(key.public.e.as_bytes());
            let n = rsa::BigUint::from_bytes_be(key.public.n.as_bytes());
            let d = rsa::BigUint::from_bytes_be(key.private.d.as_bytes());

            let rsa_key = rsa::RsaPrivateKey::from_components(n, e, d, vec![p, q])
                .map_err(ssh_agent_lib::error::AgentError::other)?;

            let mut rng = rsa::rand_core::OsRng;

            let (algorithm, sig_bytes) = if flags & SSH_AGENT_RSA_SHA2_512 != 0 {
                let signing_key = rsa::pkcs1v15::SigningKey::<sha2::Sha512>::new(rsa_key);
                let signature = signing_key
                    .try_sign_with_rng(&mut rng, data)
                    .map_err(ssh_agent_lib::error::AgentError::other)?;
                ("rsa-sha2-512", signature.to_vec())
            } else if flags & SSH_AGENT_RSA_SHA2_256 != 0 {
                let signing_key = rsa::pkcs1v15::SigningKey::<sha2::Sha256>::new(rsa_key);
                let signature = signing_key
                    .try_sign_with_rng(&mut rng, data)
                    .map_err(ssh_agent_lib::error::AgentError::other)?;
                ("rsa-sha2-256", signature.to_vec())
            } else {
                let signing_key = rsa::pkcs1v15::SigningKey::<sha1::Sha1>::new_unprefixed(rsa_key);
                let signature = signing_key
                    .try_sign_with_rng(&mut rng, data)
                    .map_err(ssh_agent_lib::error::AgentError::other)?;
                ("ssh-rsa", signature.to_vec())
            };

            Ok(Signature::new(
                ssh_agent_lib::ssh_key::Algorithm::new(algorithm)
                    .map_err(ssh_agent_lib::error::AgentError::other)?,
                sig_bytes,
            )
            .map_err(ssh_agent_lib::error::AgentError::other)?)
        }
        other => Err(ssh_agent_lib::error::AgentError::other(
            cosmic_bwarden_core::error::Error::Other(format!("unsupported key type: {:?}", other)),
        )),
    }
}
