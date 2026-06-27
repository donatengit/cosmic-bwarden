use crate::state::State;
use ssh_agent_lib::proto::{Extension, Identity};
use ssh_agent_lib::ssh_key::{PrivateKey, PublicKey, Signature};
use std::sync::Arc;
use tokio::sync::Mutex;
use signature::{Signer as _, RandomizedSigner as _, SignatureEncoding as _};

const SSH_AGENT_RSA_SHA2_256: u32 = 2;
const SSH_AGENT_RSA_SHA2_512: u32 = 4;

#[derive(Clone)]
pub struct SshAgent {
    state: Arc<Mutex<State>>,
}

impl SshAgent {
    pub fn new(state: Arc<Mutex<State>>) -> Self {
        Self { state }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let socket = cosmic_bwarden_core::dirs::ssh_agent_socket_file();
        let _ = std::fs::remove_file(&socket);
        if let Some(parent) = socket.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let listener = tokio::net::UnixListener::bind(&socket)?;

        // Enforce 0600 permissions on the socket, matching the main IPC
        // socket (main.rs) and the security model of a real ssh-agent.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;

        ssh_agent_lib::agent::listen(listener, self).await?;
        Ok(())
    }
}

#[ssh_agent_lib::async_trait]
impl ssh_agent_lib::agent::Session for SshAgent {
    async fn request_identities(
        &mut self,
    ) -> Result<Vec<Identity>, ssh_agent_lib::error::AgentError> {
        let mut state = self.state.lock().await;
        let Some(db) = &state.db else {
            state.request_unlock();
            return Ok(Vec::new());
        };
        let Some(keys) = &state.keys else {
            state.request_unlock();
            return Ok(Vec::new());
        };

        let mut identities = Vec::new();
        for entry in &db.entries {
            if !matches!(entry.data, cosmic_bwarden_core::db::EntryData::SshKey { .. }) {
                continue;
            }
            // Vaultwarden's sync response leaves the SSH-key cipher
            // sub-data empty; `decrypt()` falls back to the
            // "public_key"/"private_key" custom fields where the real
            // (encrypted) values actually live.
            let decrypted = entry.decrypt(keys);
            if let cosmic_bwarden_core::db::EntryData::SshKey { public_key: Some(pk_str), .. } = &decrypted.data {
                if let Ok(pk) = pk_str.parse::<PublicKey>() {
                    identities.push(Identity {
                        pubkey: pk.key_data().clone(),
                        comment: entry.name.clone(),
                    });
                }
            }
        }
        Ok(identities)
    }

    async fn extension(
        &mut self,
        _extension: Extension,
    ) -> Result<Option<Extension>, ssh_agent_lib::error::AgentError> {
        // Return Ok(None) so the library sends SSH_AGENT_SUCCESS without logging
        // an error. SSH clients routinely probe for extensions (command 27) and
        // handle a silent "not supported" response correctly.
        Ok(None)
    }

    async fn sign(
        &mut self,
        request: ssh_agent_lib::proto::SignRequest,
    ) -> Result<Signature, ssh_agent_lib::error::AgentError> {
        let mut state = self.state.lock().await;
        let Some(db) = &state.db else {
            state.request_unlock();
            return Err(ssh_agent_lib::error::AgentError::other(cosmic_bwarden_core::error::Error::Other("agent is locked".to_string())));
        };
        let Some(keys) = &state.keys else {
            state.request_unlock();
            return Err(ssh_agent_lib::error::AgentError::other(cosmic_bwarden_core::error::Error::Other("agent is locked".to_string())));
        };

        let req_pubkey = PublicKey::new(request.pubkey, "");
        let req_bytes = req_pubkey.to_bytes();

        for entry in &db.entries {
            if !matches!(entry.data, cosmic_bwarden_core::db::EntryData::SshKey { .. }) {
                continue;
            }
            // See comment in `request_identities`: decrypt via the entry's
            // fallback-field-aware `decrypt()` rather than the raw
            // (often-empty) SSH-key cipher sub-data.
            let decrypted = entry.decrypt(keys);
            if let cosmic_bwarden_core::db::EntryData::SshKey { private_key: Some(sk), public_key: Some(pk_str), .. } = &decrypted.data {
                if let Ok(pk) = pk_str.parse::<PublicKey>() {
                    if pk.to_bytes() == req_bytes {
                        let sk = PrivateKey::from_openssh(sk.expose())
                            .map_err(|e| ssh_agent_lib::error::AgentError::other(e))?;

                        return sign_with_key(&sk, &request.data, request.flags);
                    }
                }
            }
        }

        Err(ssh_agent_lib::error::AgentError::other(cosmic_bwarden_core::error::Error::Other("no matching key found".to_string())))
    }
}

fn sign_with_key(
    private_key: &PrivateKey,
    data: &[u8],
    flags: u32,
) -> Result<Signature, ssh_agent_lib::error::AgentError> {
    match private_key.key_data() {
        ssh_agent_lib::ssh_key::private::KeypairData::Ed25519(key) => {
            key.try_sign(data)
                .map_err(ssh_agent_lib::error::AgentError::other)
        }
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
                let signature = signing_key.try_sign_with_rng(&mut rng, data)
                    .map_err(ssh_agent_lib::error::AgentError::other)?;
                ("rsa-sha2-512", signature.to_vec())
            } else if flags & SSH_AGENT_RSA_SHA2_256 != 0 {
                let signing_key = rsa::pkcs1v15::SigningKey::<sha2::Sha256>::new(rsa_key);
                let signature = signing_key.try_sign_with_rng(&mut rng, data)
                    .map_err(ssh_agent_lib::error::AgentError::other)?;
                ("rsa-sha2-256", signature.to_vec())
            } else {
                let signing_key = rsa::pkcs1v15::SigningKey::<sha1::Sha1>::new_unprefixed(rsa_key);
                let signature = signing_key.try_sign_with_rng(&mut rng, data)
                    .map_err(ssh_agent_lib::error::AgentError::other)?;
                ("ssh-rsa", signature.to_vec())
            };

            Ok(Signature::new(
                ssh_agent_lib::ssh_key::Algorithm::new(algorithm)
                    .map_err(ssh_agent_lib::error::AgentError::other)?,
                sig_bytes,
            ).map_err(ssh_agent_lib::error::AgentError::other)?)
        }
        other => Err(ssh_agent_lib::error::AgentError::other(cosmic_bwarden_core::error::Error::Other(format!("unsupported key type: {:?}", other)))),
    }
}
