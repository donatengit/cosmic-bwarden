//! Seal or remove the TPM-bound master-password-hash ("server credentials") blob,
//! which enables silent server re-authentication after a PIN unlock.

use crate::state::State;
use cosmic_bwarden_core::protocol::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

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
