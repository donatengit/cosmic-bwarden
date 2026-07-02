#[cfg(feature = "tpm")]
use crate::keyring;
use crate::state::State;
#[cfg(feature = "tpm")]
use cosmic_bwarden_core::db::Secret;
use cosmic_bwarden_core::protocol::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Minimum length for a TPM-unlock PIN. Short/empty PINs offer negligible
/// protection: the sealed blob is on disk, so brute force is bounded only by TPM
/// dictionary-attack lockout. Enforced in the agent, not just the UI.
#[cfg(feature = "tpm")]
const MIN_PIN_LEN: usize = 6;

#[cfg(feature = "tpm")]
fn validate_pin(pin: &str) -> Result<(), Response> {
    if pin.chars().count() < MIN_PIN_LEN {
        return Err(Response::Error {
            message: format!("PIN must be at least {MIN_PIN_LEN} characters"),
        });
    }
    Ok(())
}

/// Remove the TPM-sealed server-credentials (master-password-hash) blob for an
/// account, if present. Enabling PIN unlock is a fresh start: the vault-key blob
/// is overwritten by the new seal, but the separate server-credentials store must
/// be cleared explicitly so a re-enable never inherits a stale one. Server
/// credentials must then be re-enabled explicitly by the user.
#[cfg(feature = "tpm")]
fn reset_server_credentials_store(server: &str, email: &str) {
    let hash_blob_path = cosmic_bwarden_core::dirs::tpm_hash_blob_file(server, email);
    if hash_blob_path.exists() {
        if let Err(e) = crate::tpm::clear(&hash_blob_path) {
            log::error!("TPM enable: failed to clear stale server-credentials blob: {}", e);
        }
    }
}

/// Query whether the TPM is available and a sealed blob is configured.
pub async fn handle_check_tpm(_state: &Arc<Mutex<State>>) -> Response {
    #[cfg(feature = "tpm")]
    {
        let available = crate::tpm::is_available().await;
        let (configured, server_credentials) = {
            let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                Ok(c) => c,
                Err(_) => return Response::TpmStatus { available, configured: false, server_credentials: false },
            };
            let email = match config.email.as_deref() {
                Some(e) => e,
                None => return Response::TpmStatus { available, configured: false, server_credentials: false },
            };
            let blob_path = cosmic_bwarden_core::dirs::tpm_blob_file(&config.server_name(), email);
            let hash_blob_path = cosmic_bwarden_core::dirs::tpm_hash_blob_file(&config.server_name(), email);
            (blob_path.exists(), hash_blob_path.exists())
        };
        Response::TpmStatus { available, configured, server_credentials }
    }
    #[cfg(not(feature = "tpm"))]
    {
        Response::TpmStatus { available: false, configured: false, server_credentials: false }
    }
}

/// Set up TPM PIN unlock: validate master password, seal derived keys, update config.
pub async fn handle_setup_tpm_pin(
    master_password: String,
    pin: String,
    state: &Arc<Mutex<State>>,
) -> Response {
    #[cfg(feature = "tpm")]
    {
        if let Err(e) = validate_pin(&pin) {
            return e;
        }
        if !crate::tpm::is_available().await {
            return Response::Error {
                message: "TPM is not available on this system".to_string(),
            };
        }

        // Load config and DB.
        let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
            Ok(c) => c,
            Err(e) => {
                return Response::Error {
                    message: format!("failed to load config: {}", e),
                }
            }
        };
        let email = match config.email.as_ref() {
            Some(e) => e.clone(),
            None => {
                return Response::Error {
                    message: "email not set in config".to_string(),
                }
            }
        };
        let db = match cosmic_bwarden_core::db::Db::load(&config.server_name(), &email) {
            Ok(d) => d,
            Err(e) => {
                return Response::Error {
                    message: format!("failed to load db: {}", e),
                }
            }
        };

        if !db.has_account() {
            return Response::Error {
                message: "no account configured — please login first".to_string(),
            };
        }

        // Derive identity from the provided master password to validate it AND
        // obtain the vault encryption keys we will seal.
        let mut pw_vec = cosmic_bwarden_core::locked::Vec::new();
        pw_vec.extend(master_password.as_bytes().iter().copied());
        let pw = cosmic_bwarden_core::locked::Password::new(pw_vec);

        let kdf = db.kdf.unwrap_or(cosmic_bwarden_core::api::KdfType::Pbkdf2);
        let iterations = db.iterations.unwrap_or(100_000);

        let identity = match cosmic_bwarden_core::identity::Identity::new(
            &email,
            &pw,
            kdf,
            iterations,
            db.memory,
            db.parallelism,
        ) {
            Ok(id) => id,
            Err(e) => {
                return Response::Error {
                    message: format!("key derivation failed: {}", e),
                }
            }
        };

        // Validate the password by attempting vault decryption.
        let prot_key = db.protected_key.as_ref().map(|s| s.expose()).unwrap_or("");
        let org_keys: std::collections::HashMap<_, _> = db
            .protected_org_keys
            .iter()
            .map(|(k, v)| (k.clone(), v.expose().to_string()))
            .collect();

        if let Err(e) = cosmic_bwarden_core::vault::unlock_from_keys(
            &identity.keys,
            prot_key,
            db.protected_private_key.as_ref().map(|s| s.expose()),
            &org_keys,
        ) {
            return Response::Error {
                message: format!("master password incorrect: {}", e),
            };
        }

        // Seal the identity keys into the TPM.
        let blob_path =
            cosmic_bwarden_core::dirs::tpm_blob_file(&config.server_name(), &email);

        if let Err(e) = crate::tpm::seal(&identity.keys, &pin, &blob_path).await {
            let msg = format!("TPM seal failed: {:#}", e);
            log::error!("{}", msg);
            return Response::Error { message: msg };
        }

        // Fresh enable: reset all other TPM stores so re-enabling never inherits
        // stale state (the vault-key blob was just overwritten by the seal above).
        reset_server_credentials_store(&config.server_name(), &email);

        // Update config.
        let mut updated_config = config;
        updated_config.tpm_enabled = true;
        updated_config.tpm_store_server_credentials = false;
        if let Err(e) = updated_config.save_legacy() {
            log::error!("TPM setup: failed to save config: {}", e);
        }

        // Reflect in-memory state.
        {
            let mut g = state.lock().await;
            g.tpm_configured = true;
        }

        log::info!("TPM PIN unlock configured for {}", email);
        Response::Ack
    }
    #[cfg(not(feature = "tpm"))]
    {
        let _ = (master_password, pin, state);
        Response::Error {
            message: "TPM support not compiled in this build".to_string(),
        }
    }
}

/// Unlock the vault using the TPM-sealed keys and a PIN.
pub async fn handle_unlock_with_pin(pin: String, state: &Arc<Mutex<State>>) -> Response {
    #[cfg(feature = "tpm")]
    {
        let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
            Ok(c) => c,
            Err(e) => {
                return Response::Error {
                    message: format!("failed to load config: {}", e),
                }
            }
        };
        let email = match config.email.as_ref() {
            Some(e) => e.clone(),
            None => {
                return Response::Error {
                    message: "email not set in config".to_string(),
                }
            }
        };
        let mut db = match cosmic_bwarden_core::db::Db::load(&config.server_name(), &email) {
            Ok(d) => d,
            Err(e) => {
                return Response::Error {
                    message: format!("failed to load db: {}", e),
                }
            }
        };

        if !db.has_account() {
            return Response::Error {
                message: "no account configured — please login first".to_string(),
            };
        }

        let blob_path =
            cosmic_bwarden_core::dirs::tpm_blob_file(&config.server_name(), &email);
        let hash_blob_path =
            cosmic_bwarden_core::dirs::tpm_hash_blob_file(&config.server_name(), &email);

        // Unseal the vault symmetric keys from the TPM (these are the same keys stored
        // in state.keys after a normal password unlock — NOT the identity/KDF keys).
        let vault_keys = match crate::tpm::unseal(&blob_path, &pin).await {
            Ok(k) => k,
            Err(e) => {
                let msg = format!("TPM unseal failed: {:#}", e);
                log::error!("{}", msg);
                return Response::Error { message: msg };
            }
        };

        // Try to unseal the server credentials (master_password_hash) if available.
        // This blob is TPM-bound only (no PIN) and enables silent re-auth after PIN unlock.
        let maybe_hash = if hash_blob_path.exists() {
            match crate::tpm::unseal_bytes(&hash_blob_path, "").await {
                Ok(bytes) => {
                    let mut locked_vec = cosmic_bwarden_core::locked::Vec::new();
                    locked_vec.extend(bytes.iter().copied());
                    Some(cosmic_bwarden_core::locked::PasswordHash::new(locked_vec))
                }
                Err(e) => {
                    log::warn!("pin unlock: could not unseal server credentials from TPM: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Derive org keys from the vault symmetric keys directly (no protected-key
        // decryption needed — we already have the vault keys).
        let org_keys_raw: std::collections::HashMap<_, _> = db
            .protected_org_keys
            .iter()
            .map(|(k, v)| (k.clone(), v.expose().to_string()))
            .collect();

        let org_keys = match cosmic_bwarden_core::vault::decrypt_org_keys(
            &vault_keys,
            db.protected_private_key.as_ref().map(|s| s.expose()),
            &org_keys_raw,
        ) {
            Ok(k) => k,
            Err(e) => {
                return Response::Error {
                    message: format!("org key derivation failed: {}", e),
                }
            }
        };

        let keys = vault_keys;

        // Restore session tokens — Db::load() never has them (serde skip).
        // Try keyring first, then fall back to whatever was in memory before
        // locking (covers a lock→pin-unlock cycle without agent restart).
        // PIN unlock cannot do a silent re-auth (no master_password_hash),
        // so if tokens are unavailable server sync will fail until the user
        // logs out and back in.
        if db.access_token.is_none() && config.persist_session {
            match keyring::get_tokens(&config.server_name(), &email).await {
                Ok(Some((at, rt))) => {
                    db.access_token = Some(Secret::from(at));
                    db.refresh_token = Some(Secret::from(rt));
                }
                Ok(None) => {}
                Err(e) => log::warn!("pin unlock: could not load tokens from keyring: {}", e),
            }
        }

        let needs_reauth = {
            let mut g = state.lock().await;

            // In-memory copy covers lock→pin-unlock without agent restart when
            // persist_session is false or keyring was unavailable.
            if db.access_token.is_none() {
                if let Some(prev_db) = &g.db {
                    db.access_token = prev_db.access_token.clone();
                    db.refresh_token = prev_db.refresh_token.clone();
                }
            }

            let has_token = db.access_token.is_some();
            let has_hash = maybe_hash.is_some();

            if !has_token && !has_hash {
                log::warn!(
                    "pin unlock: no session token available for {} — \
                     server sync will fail until you log out and log in again",
                    email
                );
            }

            g.keys = Some(keys);
            g.org_keys = Some(org_keys);
            g.master_password_hash = maybe_hash;
            g.pinned_ids.clear();
            for entry in &db.entries {
                if entry.favorite {
                    g.pinned_ids.insert(entry.id.clone());
                }
            }
            g.db = Some(db);
            g.rebuild_sidebar_cache();
            g.broadcast(cosmic_bwarden_core::protocol::Event::Unlocked);
            !has_token && has_hash
        };

        // If we have a sealed hash but no session token, do a silent re-auth now
        // (same path as master-password unlock when tokens are missing).
        if needs_reauth {
            log::info!("pin unlock: no session token, attempting silent re-auth via sealed hash");
            let hash = {
                let g = state.lock().await;
                g.master_password_hash.clone()
            };
            if let Some(master_password_hash) = hash {
                let client = cosmic_bwarden_core::api::Client::new(
                    &config.base_url(),
                    &config.identity_url(),
                );
                match config.device_id().await {
                    Ok(device_id) => {
                        match client
                            .login(&email, &device_id, &master_password_hash,
                                   None, None, None, None)
                            .await
                        {
                            Ok((access_token, refresh_token, _protected_key)) => {
                                log::info!("pin unlock: silent re-auth succeeded");
                                {
                                    let mut g = state.lock().await;
                                    if let Some(db) = &mut g.db {
                                        db.access_token = Some(Secret::from(access_token.clone()));
                                        db.refresh_token = refresh_token.as_ref().map(|rt| Secret::from(rt.clone()));
                                    }
                                }
                                if config.persist_session {
                                    if let Some(rt) = &refresh_token {
                                        if let Err(e) = keyring::store_tokens(
                                            &config.server_name(), &email, &access_token, rt,
                                        ).await {
                                            log::error!("pin unlock: failed to store refreshed tokens in keyring: {}", e);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("pin unlock: silent re-auth failed (sync will be unavailable): {}", e);
                            }
                        }
                    }
                    Err(e) => log::warn!("pin unlock: could not obtain device_id for silent re-auth: {}", e),
                }
            }
        }

        log::info!("vault unlocked via TPM PIN for {}", email);
        Response::Ack
    }
    #[cfg(not(feature = "tpm"))]
    {
        let _ = (pin, state);
        Response::Error {
            message: "TPM support not compiled in this build".to_string(),
        }
    }
}

/// Set up TPM PIN from an already-unlocked vault (no master password re-entry needed).
pub async fn handle_setup_tpm_pin_from_unlocked(
    pin: String,
    state: &Arc<Mutex<State>>,
) -> Response {
    #[cfg(feature = "tpm")]
    {
        if let Err(e) = validate_pin(&pin) {
            return e;
        }
        if !crate::tpm::is_available().await {
            return Response::Error {
                message: "TPM is not available on this system".to_string(),
            };
        }

        let keys = {
            let g = state.lock().await;
            match g.keys.clone() {
                Some(k) => k,
                None => {
                    return Response::Error {
                        message: "vault is locked — unlock first before setting up PIN".to_string(),
                    }
                }
            }
        };

        let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
            Ok(c) => c,
            Err(e) => {
                return Response::Error {
                    message: format!("failed to load config: {}", e),
                }
            }
        };
        let email = match config.email.as_ref() {
            Some(e) => e.clone(),
            None => {
                return Response::Error {
                    message: "email not set in config".to_string(),
                }
            }
        };

        let blob_path = cosmic_bwarden_core::dirs::tpm_blob_file(&config.server_name(), &email);

        if let Err(e) = crate::tpm::seal(&keys, &pin, &blob_path).await {
            let msg = format!("TPM seal failed: {:#}", e);
            log::error!("{}", msg);
            return Response::Error { message: msg };
        }

        // Fresh enable: reset all other TPM stores so re-enabling never inherits
        // stale state (the vault-key blob was just overwritten by the seal above).
        reset_server_credentials_store(&config.server_name(), &email);

        let mut updated_config = config;
        updated_config.tpm_enabled = true;
        updated_config.tpm_store_server_credentials = false;
        if let Err(e) = updated_config.save_legacy() {
            log::error!("TPM setup: failed to save config: {}", e);
        }

        {
            let mut g = state.lock().await;
            g.tpm_configured = true;
        }

        log::info!("TPM PIN unlock configured for {} (from unlocked state)", email);
        Response::Ack
    }
    #[cfg(not(feature = "tpm"))]
    {
        let _ = (pin, state);
        Response::Error {
            message: "TPM support not compiled in this build".to_string(),
        }
    }
}

/// Return the TPM dictionary-attack lockout status (attempts remaining, etc).
pub async fn handle_get_tpm_da_status() -> Response {
    #[cfg(feature = "tpm")]
    {
        Response::TpmDaStatus {
            status: crate::tpm::da_status().await,
        }
    }
    #[cfg(not(feature = "tpm"))]
    {
        Response::TpmDaStatus {
            status: cosmic_bwarden_core::protocol::TpmDaStatus::default(),
        }
    }
}

/// Return system-level diagnostic checks explaining why TPM may be unavailable.
pub async fn handle_check_tpm_diagnostics() -> Response {
    #[cfg(feature = "tpm")]
    {
        let checks = crate::tpm::diagnostics();
        Response::TpmDiagnostics { checks }
    }
    #[cfg(not(feature = "tpm"))]
    {
        Response::TpmDiagnostics {
            checks: vec![(
                "TPM feature compiled in".to_string(),
                false,
                "Rebuild with --features cosmic-bwarden-agent/tpm (requires libtss2-dev)"
                    .to_string(),
            )],
        }
    }
}

/// Disable TPM PIN unlock.
///
/// The vault must already be unlocked — being authenticated in the vault is the
/// only authorization needed. Re-entering the master password adds no security
/// because the vault symmetric keys are already in memory.
pub async fn handle_disable_tpm_pin(state: &Arc<Mutex<State>>) -> Response {
    #[cfg(feature = "tpm")]
    {
        let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
            Ok(c) => c,
            Err(e) => {
                return Response::Error {
                    message: format!("failed to load config: {}", e),
                }
            }
        };
        let email = match config.email.as_ref() {
            Some(e) => e.clone(),
            None => {
                return Response::Error {
                    message: "email not set in config".to_string(),
                }
            }
        };

        // Gate on vault being unlocked — this is the authorization check.
        {
            let g = state.lock().await;
            if g.keys.is_none() {
                return Response::Error {
                    message: "vault is locked — unlock first before disabling PIN".to_string(),
                };
            }
        }

        let blob_path = cosmic_bwarden_core::dirs::tpm_blob_file(&config.server_name(), &email);
        if let Err(e) = crate::tpm::clear(&blob_path) {
            log::error!("TPM clear (vault keys) failed: {}", e);
        }
        // Also remove the server-credentials blob — it was sealed with the same PIN
        // and is meaningless without the vault keys blob.
        let hash_blob_path = cosmic_bwarden_core::dirs::tpm_hash_blob_file(&config.server_name(), &email);
        if hash_blob_path.exists() {
            if let Err(e) = crate::tpm::clear(&hash_blob_path) {
                log::error!("TPM clear (server credentials) failed: {}", e);
            }
        }

        let mut updated_config = config;
        updated_config.tpm_enabled = false;
        updated_config.tpm_store_server_credentials = false;
        if let Err(e) = updated_config.save_legacy() {
            log::error!("TPM disable: failed to save config: {}", e);
        }

        {
            let mut g = state.lock().await;
            g.tpm_configured = false;
        }

        log::info!("TPM PIN unlock disabled for {}", email);
        Response::Ack
    }
    #[cfg(not(feature = "tpm"))]
    {
        let _ = state;
        Response::Error {
            message: "TPM support not compiled in this build".to_string(),
        }
    }
}

/// Seal the in-memory master_password_hash into a TPM blob (TPM-bound, no PIN).
/// Enables silent server re-authentication after PIN unlock.
/// Fails if the vault was unlocked via PIN (hash not in memory).
pub async fn handle_enable_tpm_server_credentials(state: &Arc<Mutex<State>>) -> Response {
    #[cfg(feature = "tpm")]
    {
        if !crate::tpm::is_available().await {
            return Response::Error {
                message: "TPM is not available on this system".to_string(),
            };
        }

        let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
            Ok(c) => c,
            Err(e) => {
                return Response::Error {
                    message: format!("failed to load config: {}", e),
                }
            }
        };
        let email = match config.email.as_ref() {
            Some(e) => e.clone(),
            None => {
                return Response::Error {
                    message: "email not set in config".to_string(),
                }
            }
        };

        // Extract the hash from memory (only present after master-password unlock or login).
        let hash_bytes = {
            let g = state.lock().await;
            if g.keys.is_none() {
                return Response::Error {
                    message: "vault is locked — unlock first".to_string(),
                };
            }
            match &g.master_password_hash {
                Some(h) => h.hash().to_vec(),
                None => {
                    return Response::Error {
                        message: "server credentials not available — please unlock with master password first to enable this feature".to_string(),
                    }
                }
            }
        };

        let hash_blob_path =
            cosmic_bwarden_core::dirs::tpm_hash_blob_file(&config.server_name(), &email);

        // Seal with empty PIN — TPM-bound, no dictionary-attack protection needed here.
        if let Err(e) = crate::tpm::seal_bytes(&hash_bytes, "", &hash_blob_path).await {
            let msg = format!("TPM seal (server credentials) failed: {:#}", e);
            log::error!("{}", msg);
            return Response::Error { message: msg };
        }

        let mut updated_config = config;
        updated_config.tpm_store_server_credentials = true;
        if let Err(e) = updated_config.save_legacy() {
            log::error!("enable TPM server credentials: failed to save config: {}", e);
        }

        log::info!("TPM server credentials sealed for {}", email);
        Response::Ack
    }
    #[cfg(not(feature = "tpm"))]
    {
        let _ = state;
        Response::Error {
            message: "TPM support not compiled in this build".to_string(),
        }
    }
}

/// Remove the TPM-sealed server-credentials blob.
pub async fn handle_disable_tpm_server_credentials(state: &Arc<Mutex<State>>) -> Response {
    #[cfg(feature = "tpm")]
    {
        let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
            Ok(c) => c,
            Err(e) => {
                return Response::Error {
                    message: format!("failed to load config: {}", e),
                }
            }
        };
        let email = match config.email.as_ref() {
            Some(e) => e.clone(),
            None => {
                return Response::Error {
                    message: "email not set in config".to_string(),
                }
            }
        };

        {
            let g = state.lock().await;
            if g.keys.is_none() {
                return Response::Error {
                    message: "vault is locked — unlock first before changing this setting".to_string(),
                };
            }
        }

        let hash_blob_path =
            cosmic_bwarden_core::dirs::tpm_hash_blob_file(&config.server_name(), &email);
        if let Err(e) = crate::tpm::clear(&hash_blob_path) {
            log::error!("TPM clear (server credentials) failed: {}", e);
        }

        let mut updated_config = config;
        updated_config.tpm_store_server_credentials = false;
        if let Err(e) = updated_config.save_legacy() {
            log::error!("disable TPM server credentials: failed to save config: {}", e);
        }

        log::info!("TPM server credentials disabled for {}", email);
        Response::Ack
    }
    #[cfg(not(feature = "tpm"))]
    {
        let _ = state;
        Response::Error {
            message: "TPM support not compiled in this build".to_string(),
        }
    }
}
