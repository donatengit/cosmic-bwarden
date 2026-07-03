//! Disable TPM PIN unlock.

use crate::state::State;
use cosmic_bwarden_core::protocol::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

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
