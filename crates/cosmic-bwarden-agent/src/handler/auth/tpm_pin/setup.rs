//! Enable TPM PIN unlock — either by re-validating the master password, or from
//! an already-unlocked vault.

use crate::state::State;
use cosmic_bwarden_core::protocol::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Set up TPM PIN unlock: validate master password, seal derived keys, update config.
pub async fn handle_setup_tpm_pin(
    master_password: String,
    pin: String,
    state: &Arc<Mutex<State>>,
) -> Response {
    #[cfg(feature = "tpm")]
    {
        if let Err(e) = super::validate_pin(&pin) {
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
        super::reset_server_credentials_store(&config.server_name(), &email);

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

/// Set up TPM PIN from an already-unlocked vault (no master password re-entry needed).
pub async fn handle_setup_tpm_pin_from_unlocked(
    pin: String,
    state: &Arc<Mutex<State>>,
) -> Response {
    #[cfg(feature = "tpm")]
    {
        if let Err(e) = super::validate_pin(&pin) {
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
        super::reset_server_credentials_store(&config.server_name(), &email);

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
