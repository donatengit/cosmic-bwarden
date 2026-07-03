//! TPM PIN-unlock request handlers.
//!
//! Every handler compiles in both configurations: with `--features tpm` it does
//! the real work; without it, each returns a "TPM support not compiled in this
//! build" error (or an unavailable status). This lets the protocol surface stay
//! identical regardless of the build.
//!
//! Handlers are grouped by concern:
//! - [`status`] — availability, DA lockout, diagnostics (read-only queries).
//! - [`setup`] — enable PIN unlock (from master password or an unlocked vault).
//! - [`unlock`] — unlock the vault with a PIN.
//! - [`disable`] — turn PIN unlock off.
//! - [`server_credentials`] — seal/remove the master-password-hash blob.

#[cfg(feature = "tpm")]
use cosmic_bwarden_core::protocol::Response;

mod disable;
mod server_credentials;
mod setup;
mod status;
mod unlock;

pub use disable::handle_disable_tpm_pin;
pub use server_credentials::{
    handle_disable_tpm_server_credentials, handle_enable_tpm_server_credentials,
};
pub use setup::{handle_setup_tpm_pin, handle_setup_tpm_pin_from_unlocked};
pub use status::{handle_check_tpm, handle_check_tpm_diagnostics, handle_get_tpm_da_status};
pub use unlock::handle_unlock_with_pin;

/// Minimum length for a TPM-unlock PIN; single source in core. Enforced in
/// the agent, not just the UI.
#[cfg(feature = "tpm")]
const MIN_PIN_LEN: usize = cosmic_bwarden_core::MIN_PIN_LEN;

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
