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

#[cfg(feature = "tpm")]
use cosmic_bwarden_core::protocol::Response;

mod disable;
mod flags;
mod setup;
mod status;
mod unlock;

pub use disable::handle_disable_tpm_pin;
pub use setup::{handle_setup_tpm_pin, handle_setup_tpm_pin_from_unlocked};
pub use status::{handle_check_tpm, handle_check_tpm_diagnostics, handle_get_tpm_da_status};
pub use unlock::handle_unlock_with_pin;

/// Minimum length for a TPM-unlock PIN; single source in core. Enforced in
/// the agent, not just the UI.
#[cfg(feature = "tpm")]
const MIN_PIN_LEN: usize = cosmic_bwarden_core::MIN_PIN_LEN;

// `Response` is deliberately unboxed (see the enum's large_enum_variant note);
// this Err is constructed once per failed validation, not on a hot path.
#[allow(clippy::result_large_err)]
#[cfg(feature = "tpm")]
fn validate_pin(pin: &str) -> Result<(), Response> {
    if pin.chars().count() < MIN_PIN_LEN {
        return Err(Response::Error {
            message: format!("PIN must be at least {MIN_PIN_LEN} characters"),
        });
    }
    Ok(())
}
