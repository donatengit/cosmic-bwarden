//! TPM2 sealed-secret storage.
//!
//! Vault keys (64 bytes = enc_key_expanded ‖ mac_key_expanded) are sealed in
//! a TPM2 object protected by a user PIN. The TPM enforces dictionary-attack
//! lockout on wrong PINs.
//!
//! The sealed blob is written to a per-account file on disk. Unsealing requires
//! the correct PIN. If the TPM or sealed blob is unavailable, the user falls
//! back to master-password unlock.
//!
//! This module never panics. All errors are returned as `anyhow::Error`.
//!
//! The whole module is compiled only under `--features tpm` (gated at the
//! `mod tpm;` declaration in the crate root), so every submodule inherits that
//! gate. Runtime availability is a separate check: [`is_available`].
//!
//! Layout:
//! - [`policy`] — TPM object templates + PolicyPCR/PolicyAuthValue digest.
//! - [`blob`] — on-disk sealed-blob format (v2) marshalling.
//! - [`ops`] — seal/unseal of the vault keys (public API).

use anyhow::{Context as _, Result};
use std::path::Path;
use tss_esapi::{constants::PropertyTag, Context, TctiNameConf};

mod blob;
mod ops;
mod policy;
#[cfg(test)]
mod tests;

pub use ops::{seal, unseal};

/// Why a TPM unseal failed, classified from the underlying TSS response code.
/// Clients map this to user feedback: a wrong PIN and a changed PCR state must
/// never be presented the same way (one consumed a dictionary-attack attempt,
/// the other means the user's PIN is fine and the machine state changed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsealFailure {
    /// The PIN was wrong (`TPM_RC_AUTH_FAIL`) — a dictionary-attack attempt
    /// was consumed.
    WrongPin,
    /// The policy check failed (`TPM_RC_POLICY_FAIL`) — the PCR state changed
    /// (BIOS/firmware update or Secure Boot toggle). No DA attempt consumed;
    /// recovery is master-password unlock + re-seal.
    StateChanged,
    /// The TPM is in dictionary-attack lockout.
    Lockout,
    /// Anything else: no TPM, blob read failure, wrapper error, unexpected
    /// response code.
    Other,
}

/// Walk the error chain looking for the TSS response code behind the failure.
/// `UnsealFailure::Other` when no TSS error is found.
pub fn classify_unseal_failure(err: &anyhow::Error) -> UnsealFailure {
    use tss_esapi::constants::return_code::{TpmFormatOneError, TpmFormatZeroWarning};
    use tss_esapi::error::{ReturnCode, TpmFormatZeroResponseCode, TpmResponseCode};

    for e in err.chain() {
        if let Some(tss_esapi::Error::TssError(rc)) = e.downcast_ref::<tss_esapi::Error>() {
            return match rc {
                ReturnCode::Tpm(TpmResponseCode::FormatOne(f1)) => match f1.error_number() {
                    TpmFormatOneError::AuthFail => UnsealFailure::WrongPin,
                    TpmFormatOneError::PolicyFail => UnsealFailure::StateChanged,
                    _ => UnsealFailure::Other,
                },
                ReturnCode::Tpm(TpmResponseCode::FormatZero(
                    TpmFormatZeroResponseCode::Warning(w),
                )) => {
                    if w.error_number() == TpmFormatZeroWarning::Lockout {
                        UnsealFailure::Lockout
                    } else {
                        UnsealFailure::Other
                    }
                }
                _ => UnsealFailure::Other,
            };
        }
    }
    UnsealFailure::Other
}

/// Open a TPM2 context, trying (in order) an explicit TCTI from the
/// environment, the kernel resource manager, the raw device, and tpm2-abrmd.
/// Shared by every operation.
pub(crate) fn open_context() -> Result<Context> {
    // 1. Honour an explicitly configured TCTI (the E2E suite points this at
    //    swtpm). `TctiNameConf::from_environment_variable` reads
    //    TPM2TOOLS_TCTI/TCTI/TEST_TCTI — NOT TSS2_TCTI — so read TSS2_TCTI
    //    ourselves first; it is the name the harness and our docs use.
    //
    //    Both arms FAIL CLOSED: if a TCTI is configured but unusable we return
    //    the error rather than falling through to the hardware below. Falling
    //    through is how the TPM E2E suite silently ran against a developer's
    //    real TPM and drove it into dictionary-attack lockout.
    //    An empty value means "unset" and is ignored, so exporting
    //    `TSS2_TCTI=` cannot brick a production agent.
    if let Some(spec) = std::env::var("TSS2_TCTI")
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        log::debug!("TPM: using TCTI from TSS2_TCTI ({spec})");
        let tcti = spec
            .parse::<TctiNameConf>()
            .map_err(|e| anyhow::anyhow!("invalid TSS2_TCTI {spec:?}: {e}"))?;
        return Context::new(tcti).context("failed to open TPM context (from TSS2_TCTI)");
    }
    if let Ok(tcti) = TctiNameConf::from_environment_variable() {
        return Context::new(tcti)
            .context("failed to open TPM context (from TPM2TOOLS_TCTI/TCTI/TEST_TCTI)");
    }
    // 2. Kernel TPM resource manager — preferred for user-space (needs `tss` group).
    // 3. Raw character device — needs root or `tss` user.
    // 4. tpm2-abrmd D-Bus resource manager — optional daemon.
    for spec in &["device:/dev/tpmrm0", "device:/dev/tpm0", "tabrmd:"] {
        if let Ok(tcti) = spec.parse::<TctiNameConf>() {
            if let Ok(ctx) = Context::new(tcti) {
                return Ok(ctx);
            }
        }
    }
    anyhow::bail!(
        "no TPM2 device accessible; ensure the agent user is in the `tss` group \
         (sudo usermod -aG tss $USER) or install tpm2-abrmd"
    )
}

/// Returns true if a TPM2 device is accessible at runtime.
/// Never panics; returns false on any error.
pub async fn is_available() -> bool {
    open_context().is_ok()
}

/// TPMA_PERMANENT.inLockout bit.
const TPMA_PERMANENT_IN_LOCKOUT: u32 = 0x0000_0200;

/// Read the TPM's dictionary-attack (lockout) status. Uses `GetCapability`, which
/// needs no authorization. Counters are TPM-global (shared by all DA-protected
/// objects), self-heal over `recovery_interval_secs`, and reset on a successful
/// authorization. Returns `available: false` if the TPM can't be opened.
pub async fn da_status() -> cosmarden_core::protocol::TpmDaStatus {
    use cosmarden_core::protocol::TpmDaStatus;
    let mut ctx = match open_context() {
        Ok(c) => c,
        Err(_) => return TpmDaStatus::default(), // available: false
    };
    let get = |ctx: &mut Context, tag: PropertyTag| ctx.get_tpm_property(tag).ok().flatten();

    let max_tries = get(&mut ctx, PropertyTag::MaxAuthFail);
    let lockout_counter = get(&mut ctx, PropertyTag::LockoutCounter);
    let recovery_interval_secs = get(&mut ctx, PropertyTag::LockoutInterval);
    let in_lockout = get(&mut ctx, PropertyTag::Permanent)
        .map(|p| p & TPMA_PERMANENT_IN_LOCKOUT != 0)
        .unwrap_or(false);
    let remaining = match (max_tries, lockout_counter) {
        (Some(m), Some(c)) => Some(m.saturating_sub(c)),
        _ => None,
    };

    TpmDaStatus {
        available: true,
        max_tries,
        lockout_counter,
        remaining,
        in_lockout,
        recovery_interval_secs,
    }
}

/// Deletes the sealed blob file, disabling PIN unlock for this account.
///
/// Missing file is success (already revoked). Any other unlink error is
/// returned so disable cannot `Ack` a blob that is still on disk.
pub fn clear(blob_path: &Path) -> Result<()> {
    match std::fs::remove_file(blob_path) {
        Ok(()) => {
            log::info!("TPM: cleared sealed blob {}", blob_path.display());
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => {
            log::error!(
                "TPM: failed to clear sealed blob {}: {}",
                blob_path.display(),
                e
            );
            Err(e).with_context(|| format!("failed to remove TPM blob {}", blob_path.display()))
        }
    }
}

/// Perform system-level checks to diagnose why TPM is unavailable.
/// Returns a list of (label, passed, hint) triples.
pub fn diagnostics() -> Vec<(String, bool, String)> {
    let mut checks = Vec::new();

    let tpmrm0_exists = std::path::Path::new("/dev/tpmrm0").exists();
    checks.push((
        "/dev/tpmrm0 exists".to_string(),
        tpmrm0_exists,
        "Install kernel tpm2 driver or check BIOS TPM settings".to_string(),
    ));

    let tpm0_exists = std::path::Path::new("/dev/tpm0").exists();
    checks.push((
        "/dev/tpm0 exists".to_string(),
        tpm0_exists,
        "TPM hardware not detected — check BIOS settings".to_string(),
    ));

    let tpmrm0_accessible = std::fs::File::open("/dev/tpmrm0").is_ok();
    checks.push((
        "Agent can open /dev/tpmrm0".to_string(),
        tpmrm0_accessible,
        "Add user to 'tss' group: sudo usermod -aG tss $USER, then log out and back in".to_string(),
    ));

    let context_ok = open_context().is_ok();
    checks.push((
        "TPM2 context opens".to_string(),
        context_ok,
        "Install tpm2-abrmd or ensure /dev/tpmrm0 is accessible".to_string(),
    ));

    checks
}
