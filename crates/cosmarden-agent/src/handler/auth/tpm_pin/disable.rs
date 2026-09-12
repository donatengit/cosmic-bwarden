//! Disable TPM PIN unlock.

use crate::state::State;
use cosmarden_core::protocol::Response;
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
        let config = match cosmarden_core::config::CosmardenConfig::load_legacy() {
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

        let blob_path = cosmarden_core::dirs::tpm_blob_file(&config.server_name(), &email);
        if let Err(e) = crate::tpm::clear(&blob_path) {
            log::error!("TPM clear (vault keys) failed: {}", e);
            return Response::Error {
                message: format!("failed to remove TPM blob: {e}"),
            };
        }
        let mut updated_config = config;
        updated_config.tpm_enabled = false;
        if let Err(e) = updated_config.save_legacy() {
            log::error!("TPM disable: failed to save config: {}", e);
            return Response::Error {
                message: format!("failed to persist TPM disable: {e}"),
            };
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
