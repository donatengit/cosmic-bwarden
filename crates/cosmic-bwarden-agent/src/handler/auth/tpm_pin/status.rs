//! Read-only TPM status queries: availability, DA lockout, diagnostics.

use crate::state::State;
use cosmic_bwarden_core::protocol::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

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
