//! Read-only TPM status queries: availability, DA lockout, diagnostics.

use crate::state::State;
use cosmarden_core::protocol::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Query whether the TPM is available and a sealed blob is configured.
pub async fn handle_check_tpm(state: &Arc<Mutex<State>>) -> Response {
    #[cfg(feature = "tpm")]
    {
        let available = crate::tpm::is_available().await;
        let configured = {
            let config = match cosmarden_core::config::CosmardenConfig::load_legacy() {
                Ok(c) => c,
                Err(_) => {
                    return Response::TpmStatus {
                        available,
                        configured: false,
                    }
                }
            };
            let email = match config.email.as_deref() {
                Some(e) => e,
                None => {
                    return Response::TpmStatus {
                        available,
                        configured: false,
                    }
                }
            };
            cosmarden_core::dirs::tpm_blob_file(&config.server_name(), email).exists()
        };
        // Refresh the agent-side flag from blob existence, not just at startup:
        // if the blob was deleted/replaced (TPM reset, clear), `request_unlock`
        // must stop offering PIN unlock instead of pointing at a dead blob.
        {
            let mut state_guard = state.lock().await;
            state_guard.tpm_configured = configured;
        }
        Response::TpmStatus {
            available,
            configured,
        }
    }
    #[cfg(not(feature = "tpm"))]
    {
        let _ = state;
        Response::TpmStatus {
            available: false,
            configured: false,
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
            status: cosmarden_core::protocol::TpmDaStatus::default(),
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
                "Rebuild with --features cosmarden-agent/tpm (requires libtss2-dev)".to_string(),
            )],
        }
    }
}
