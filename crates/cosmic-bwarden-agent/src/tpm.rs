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

use anyhow::{Context as _, Result};
use cosmic_bwarden_core::locked;
use std::path::Path;
use zeroize::Zeroize as _;
use tss_esapi::{
    Context, TctiNameConf,
    attributes::{ObjectAttributesBuilder, SessionAttributesBuilder},
    constants::{PropertyTag, SessionType},
    interface_types::{
        algorithm::{HashingAlgorithm, PublicAlgorithm},
        reserved_handles::Hierarchy,
        session_handles::PolicySession,
    },
    structures::{
        Auth, Digest, KeyedHashScheme, PcrSelectionList, PcrSlot, Private, Public,
        PublicBuilder, PublicKeyedHashParameters, SensitiveData,
        SymmetricCipherParameters, SymmetricDefinition, SymmetricDefinitionObject,
    },
    traits::{Marshall, UnMarshall},
};

/// Sealed-blob format version. v2 binds the object to PCR{0,7} ∧ PolicyAuthValue.
/// v1 (no policy) blobs cannot be unsealed by this code — the user must re-run PIN
/// setup (also required after a firmware/Secure-Boot change invalidates the PCRs).
const SEALED_BLOB_VERSION: u8 = 2;

#[derive(serde::Serialize, serde::Deserialize)]
struct SealedBlob {
    #[serde(default)]
    version: u8,
    out_private: Vec<u8>,
    out_public: Vec<u8>,
}

fn open_context() -> Result<Context> {
    // 1. Honour TSS2_TCTI env var (used by tests to point at swtpm).
    if let Ok(tcti) = TctiNameConf::from_environment_variable() {
        return Context::new(tcti).context("failed to open TPM context (from TSS2_TCTI)");
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
pub async fn da_status() -> cosmic_bwarden_core::protocol::TpmDaStatus {
    use cosmic_bwarden_core::protocol::TpmDaStatus;
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

/// AES-128-CFB symmetric cipher storage parent (same template as tss-esapi examples).
/// Creating a primary key with this exact template always produces the same key
/// (deterministic from the TPM's owner-hierarchy seed), so we never need to persist it.
fn primary_template() -> Result<Public> {
    let attrs = ObjectAttributesBuilder::new()
        .with_fixed_tpm(true)
        .with_fixed_parent(true)
        .with_st_clear(false)
        .with_sensitive_data_origin(true)
        .with_user_with_auth(true)
        .with_decrypt(true)
        .with_restricted(true)
        .build()
        .context("building primary key attributes")?;

    PublicBuilder::new()
        .with_public_algorithm(PublicAlgorithm::SymCipher)
        .with_name_hashing_algorithm(HashingAlgorithm::Sha256)
        .with_object_attributes(attrs)
        .with_symmetric_cipher_parameters(SymmetricCipherParameters::new(
            SymmetricDefinitionObject::AES_128_CFB,
        ))
        .with_symmetric_cipher_unique_identifier(Digest::default())
        .build()
        .context("building primary key template")
}

/// PCR selection the sealed blobs are bound to: SHA-256 bank, PCR 0 (firmware /
/// UEFI code) and PCR 7 (Secure Boot state). Booting different firmware or
/// changing Secure Boot changes these, so the policy no longer satisfies and the
/// blob cannot be unsealed — the intended anti-evil-maid property.
fn pcr_selection_list() -> Result<PcrSelectionList> {
    PcrSelectionList::builder()
        .with_selection(HashingAlgorithm::Sha256, &[PcrSlot::Slot0, PcrSlot::Slot7])
        .build()
        .context("building PCR selection list")
}

/// Compute the authPolicy digest for "PolicyPCR(0,7) ∧ PolicyAuthValue" using a
/// trial session. Sealing sets this as the object's authPolicy; unsealing must
/// satisfy the same policy (correct PCRs) AND supply the PIN (auth value).
fn compute_policy_digest(ctx: &mut Context) -> Result<Digest> {
    let trial = ctx
        .start_auth_session(
            None,
            None,
            None,
            SessionType::Trial,
            SymmetricDefinition::AES_128_CFB,
            HashingAlgorithm::Sha256,
        )
        .context("starting trial policy session")?
        .ok_or_else(|| anyhow::anyhow!("TPM returned no trial session handle"))?;
    let (attrs, mask) = SessionAttributesBuilder::new().build();
    ctx.tr_sess_set_attributes(trial, attrs, mask)
        .context("setting trial session attributes")?;

    let policy_session = PolicySession::try_from(trial)
        .context("converting trial session to policy session")?;
    ctx.policy_pcr(policy_session, Digest::default(), pcr_selection_list()?)
        .context("trial policy_pcr")?;
    ctx.policy_auth_value(policy_session)
        .context("trial policy_auth_value")?;
    ctx.policy_get_digest(policy_session)
        .context("reading trial policy digest")
    // `trial` flushes when `ctx` drops (handle manager flushes on drop).
}

/// Sealed-data-object template bound to `policy_digest`. `userWithAuth=false` means
/// the PIN (object auth value) is usable *only* through the policy, which also
/// requires the PCRs to match. DA lockout stays enabled (no_da not set).
fn sealed_template(policy_digest: Digest) -> Result<Public> {
    let attrs = ObjectAttributesBuilder::new()
        .with_fixed_tpm(true)
        .with_fixed_parent(true)
        .with_st_clear(false)
        .with_user_with_auth(false)
        // with_no_da NOT set → TPM enforces dictionary-attack lockout on wrong PINs
        .build()
        .context("building sealed object attributes")?;

    PublicBuilder::new()
        .with_public_algorithm(PublicAlgorithm::KeyedHash)
        .with_name_hashing_algorithm(HashingAlgorithm::Sha256)
        .with_object_attributes(attrs)
        .with_keyed_hash_parameters(PublicKeyedHashParameters::new(KeyedHashScheme::Null))
        .with_keyed_hash_unique_identifier(Digest::default())
        .with_auth_policy(policy_digest)
        .build()
        .context("building sealed object template")
}

/// Seals arbitrary bytes bound to PCR{0,7} ∧ `pin` (PolicyAuthValue). Pass
/// `pin = ""` for PCR-bound only (still requires matching PCRs, no PIN entropy).
/// The sealed blob is written to `blob_path` with mode 0600.
pub async fn seal_bytes(data: &[u8], pin: &str, blob_path: &Path) -> Result<()> {
    let mut ctx = open_context()?;

    let primary_tmpl = primary_template()?;
    let primary = ctx
        .execute_with_nullauth_session(|c| {
            c.create_primary(Hierarchy::Owner, primary_tmpl, None, None, None, None)
        })
        .context("creating TPM primary key")?;

    let policy_digest = compute_policy_digest(&mut ctx)?;
    let tmpl = sealed_template(policy_digest)?;
    let pin_auth = Auth::from_bytes(pin.as_bytes()).context("PIN too long for TPM auth")?;
    let sensitive_data = SensitiveData::try_from(data.to_vec())
        .context("data too large for TPM sensitive data")?;

    let result = ctx
        .execute_with_nullauth_session(|c| {
            c.create(
                primary.key_handle,
                tmpl,
                Some(pin_auth),
                Some(sensitive_data),
                None,
                None,
            )
        })
        .context("creating sealed TPM object")?;

    let _ = ctx.flush_context(primary.key_handle.into());

    write_blob(&result, blob_path)?;
    log::info!("TPM: sealed {} bytes to {} (PCR-bound)", data.len(), blob_path.display());
    Ok(())
}

/// Unseals bytes from `blob_path` using `pin`. Fails on wrong PIN (DA fault),
/// changed PCRs (firmware/Secure-Boot change), or an unsupported blob version.
pub async fn unseal_bytes(blob_path: &Path, pin: &str) -> Result<Vec<u8>> {
    let mut ctx = open_context()?;
    let (private, public) = read_blob(blob_path)?;

    let primary_tmpl = primary_template()?;
    let primary = ctx
        .execute_with_nullauth_session(|c| {
            c.create_primary(Hierarchy::Owner, primary_tmpl, None, None, None, None)
        })
        .context("recreating TPM primary key for unseal")?;

    let obj_handle = ctx
        .execute_with_nullauth_session(|c| c.load(primary.key_handle, private, public))
        .context("loading sealed object into TPM")?;

    let _ = ctx.flush_context(primary.key_handle.into());

    let sensitive = unseal_with_policy(&mut ctx, obj_handle.into(), pin)?;
    Ok(sensitive.as_bytes().to_vec())
}

/// Seals `vault_keys` (64 bytes) bound to PCR{0,7} ∧ `pin`.
pub async fn seal(vault_keys: &locked::Keys, pin: &str, blob_path: &Path) -> Result<()> {
    // Seal only enc_key ‖ mac_key (exactly 64 bytes) — keys.data() may be longer
    // due to PKCS7 padding left over from AES-CBC decryption in the locked buffer.
    let mut key_material = std::vec::Vec::with_capacity(64);
    key_material.extend_from_slice(vault_keys.enc_key());
    key_material.extend_from_slice(vault_keys.mac_key());
    let res = seal_bytes(&key_material, pin, blob_path).await;
    key_material.zeroize();
    res.context("sealing vault keys")?;
    log::info!("TPM: sealed vault keys to {}", blob_path.display());
    Ok(())
}

/// Unseals the vault keys stored in `blob_path` using `pin`.
pub async fn unseal(blob_path: &Path, pin: &str) -> Result<locked::Keys> {
    let raw = unseal_bytes(blob_path, pin).await?;
    anyhow::ensure!(
        raw.len() == 64,
        "TPM unsealed {} bytes; expected 64 (vault keys)",
        raw.len()
    );

    // Copy unsealed bytes into locked (mlock'd) memory, then wipe the transient.
    let mut locked_vec = locked::Vec::new();
    locked_vec.extend(raw.iter().copied());
    let mut raw = raw;
    raw.zeroize();
    Ok(locked::Keys::new(locked_vec))
}

/// Unseal `obj_handle` under a PolicyPCR(0,7) ∧ PolicyAuthValue session with
/// command/response parameter encryption, so the unsealed key is never exposed in
/// the clear on the TPM bus (mitigates bus sniffing).
fn unseal_with_policy(
    ctx: &mut Context,
    obj_handle: tss_esapi::handles::ObjectHandle,
    pin: &str,
) -> Result<SensitiveData> {
    let session = ctx
        .start_auth_session(
            None,
            None,
            None,
            SessionType::Policy,
            SymmetricDefinition::AES_128_CFB,
            HashingAlgorithm::Sha256,
        )
        .context("starting policy session for unseal")?
        .ok_or_else(|| anyhow::anyhow!("TPM returned no policy session handle"))?;
    let (attrs, mask) = SessionAttributesBuilder::new()
        .with_decrypt(true)
        .with_encrypt(true)
        .build();
    ctx.tr_sess_set_attributes(session, attrs, mask)
        .context("setting policy session attributes")?;

    let policy_session = PolicySession::try_from(session)
        .context("converting auth session to policy session")?;
    ctx.policy_pcr(policy_session, Digest::default(), pcr_selection_list()?)
        .context("policy_pcr (PCR state changed? re-run PIN setup)")?;
    ctx.policy_auth_value(policy_session)
        .context("policy_auth_value")?;

    let pin_auth = Auth::from_bytes(pin.as_bytes()).context("PIN too long for TPM auth")?;
    ctx.tr_set_auth(obj_handle.into(), pin_auth)
        .context("setting PIN auth on TPM object")?;

    ctx.execute_with_session(Some(session), |c| c.unseal(obj_handle.into()))
        .context("TPM unseal — wrong PIN, changed PCRs, or DA lockout")
}

/// Serialize a freshly created sealed object to disk (0600, versioned).
fn write_blob(
    result: &tss_esapi::structures::CreateKeyResult,
    blob_path: &Path,
) -> Result<()> {
    let blob = SealedBlob {
        version: SEALED_BLOB_VERSION,
        out_private: result.out_private.marshall().context("marshalling TPM private")?,
        out_public: result.out_public.marshall().context("marshalling TPM public")?,
    };
    let blob_bytes = postcard::to_allocvec(&blob).context("serializing TPM blob")?;

    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(blob_path)
        .and_then(|mut f| {
            use std::io::Write as _;
            f.write_all(&blob_bytes)
        })
        .context("writing TPM blob to disk")?;
    Ok(())
}

/// Read and version-check a sealed blob, returning its TPM private/public parts.
fn read_blob(blob_path: &Path) -> Result<(Private, Public)> {
    let blob_bytes = std::fs::read(blob_path).context("reading TPM blob file")?;
    let blob: SealedBlob =
        postcard::from_bytes(&blob_bytes).context("deserializing TPM blob")?;
    anyhow::ensure!(
        blob.version == SEALED_BLOB_VERSION,
        "sealed blob version {} is not supported (expected {}); re-run PIN setup",
        blob.version,
        SEALED_BLOB_VERSION
    );
    let private = Private::unmarshall(&blob.out_private)
        .context("deserializing TPM private portion")?;
    let public = Public::unmarshall(&blob.out_public)
        .context("deserializing TPM public portion")?;
    Ok((private, public))
}

/// Deletes the sealed blob file, disabling PIN unlock for this account.
pub fn clear(blob_path: &Path) -> Result<()> {
    let _ = std::fs::remove_file(blob_path);
    log::info!("TPM: cleared sealed blob {}", blob_path.display());
    Ok(())
}

// ─── Integration tests ────────────────────────────────────────────────────────
//
// These tests hit the REAL TPM device and are excluded from normal runs.
//
// Run with:
//   cargo test -p cosmic-bwarden-agent --features tpm -- --ignored
//
// Prerequisites: user must be in the `tss` group (or run as root), and a
// TPM2 device must be accessible at /dev/tpmrm0 or /dev/tpm0.

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_bwarden_core::locked;

    fn make_test_keys(enc_seed: u8, mac_seed: u8) -> locked::Keys {
        let mut v = locked::Vec::new();
        // 32-byte enc key: repeating enc_seed
        v.extend(std::iter::repeat(enc_seed).take(32));
        // 32-byte mac key: repeating mac_seed
        v.extend(std::iter::repeat(mac_seed).take(32));
        locked::Keys::new(v)
    }

    fn test_blob_path(suffix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "cosmic_bwarden_tpm_test_{}_{}.bin",
            std::process::id(),
            suffix
        ))
    }

    /// Seal → unseal round-trip with correct PIN: keys survive intact.
    /// Wrong PIN is rejected. Clear removes the blob.
    #[tokio::test]
    #[ignore = "requires real TPM (/dev/tpmrm0 or /dev/tpm0); \
                run: cargo test -p cosmic-bwarden-agent --features tpm -- --ignored"]
    async fn tpm_seal_unseal_round_trip() {
        let path = test_blob_path("round_trip");
        let _ = std::fs::remove_file(&path); // clean up any leftover

        let keys = make_test_keys(0xAA, 0xBB);
        let pin = "testPIN9999";
        let wrong_pin = "wrongPIN000";

        // ── seal ──
        seal(&keys, pin, &path).await.expect("seal failed");
        assert!(path.exists(), "blob file was not created");

        // ── correct PIN → full key recovery ──
        let unsealed = unseal(&path, pin).await.expect("unseal with correct PIN failed");
        assert_eq!(unsealed.enc_key(), keys.enc_key(), "enc_key mismatch after round-trip");
        assert_eq!(unsealed.mac_key(), keys.mac_key(), "mac_key mismatch after round-trip");

        // ── wrong PIN → rejected ──
        let wrong = unseal(&path, wrong_pin).await;
        assert!(wrong.is_err(), "wrong PIN should have been rejected by TPM");

        // ── correct PIN still works after one wrong attempt ──
        // (TPM DA lockout triggers only after many consecutive failures)
        let after_wrong = unseal(&path, pin).await
            .expect("correct PIN should still work after one wrong attempt");
        assert_eq!(after_wrong.enc_key(), keys.enc_key());
        assert_eq!(after_wrong.mac_key(), keys.mac_key());

        // ── clear removes the blob ──
        clear(&path).expect("clear failed");
        assert!(!path.exists(), "blob file still exists after clear");

        // ── unseal after clear → file-not-found ──
        let after_clear = unseal(&path, pin).await;
        assert!(after_clear.is_err(), "unseal after clear should fail with missing file");
    }

    /// Two independent accounts with different PINs can coexist: sealing one
    /// does not interfere with the other.
    #[tokio::test]
    #[ignore = "requires real TPM; run: cargo test -p cosmic-bwarden-agent --features tpm -- --ignored"]
    async fn tpm_seal_two_independent_accounts() {
        let path_a = test_blob_path("account_a");
        let path_b = test_blob_path("account_b");
        let _ = std::fs::remove_file(&path_a);
        let _ = std::fs::remove_file(&path_b);

        let keys_a = make_test_keys(0x11, 0x22);
        let keys_b = make_test_keys(0x33, 0x44);

        seal(&keys_a, "pinForA", &path_a).await.expect("seal A failed");
        seal(&keys_b, "pinForB", &path_b).await.expect("seal B failed");

        let u_a = unseal(&path_a, "pinForA").await.expect("unseal A failed");
        let u_b = unseal(&path_b, "pinForB").await.expect("unseal B failed");

        assert_eq!(u_a.enc_key(), keys_a.enc_key(), "account A enc_key corrupted");
        assert_eq!(u_b.enc_key(), keys_b.enc_key(), "account B enc_key corrupted");

        // Cross-PIN: B's PIN rejects A's blob
        let cross = unseal(&path_a, "pinForB").await;
        assert!(cross.is_err(), "B's PIN should not unseal A's blob");

        let _ = std::fs::remove_file(&path_a);
        let _ = std::fs::remove_file(&path_b);
    }

    /// Re-sealing an existing blob (same path, new PIN) replaces it.
    #[tokio::test]
    #[ignore = "requires real TPM; run: cargo test -p cosmic-bwarden-agent --features tpm -- --ignored"]
    async fn tpm_reseal_replaces_old_blob() {
        let path = test_blob_path("reseal");
        let _ = std::fs::remove_file(&path);

        let keys = make_test_keys(0xCC, 0xDD);

        seal(&keys, "oldpin", &path).await.expect("first seal failed");
        seal(&keys, "newpin", &path).await.expect("second seal (re-seal) failed");

        // Old PIN no longer works
        let old = unseal(&path, "oldpin").await;
        assert!(old.is_err(), "old PIN should be rejected after re-seal");

        // New PIN works
        let new = unseal(&path, "newpin").await.expect("new PIN should work after re-seal");
        assert_eq!(new.enc_key(), keys.enc_key());
        assert_eq!(new.mac_key(), keys.mac_key());

        let _ = std::fs::remove_file(&path);
    }

    /// `is_available()` must not panic and diagnostics must return at least
    /// 4 entries regardless of whether a TPM is present.
    #[tokio::test]
    #[ignore = "requires real TPM; run: cargo test -p cosmic-bwarden-agent --features tpm -- --ignored"]
    async fn tpm_availability_and_diagnostics() {
        let available = is_available().await;
        println!("TPM available: {}", available);

        let checks = diagnostics();
        assert_eq!(checks.len(), 4, "expected exactly 4 diagnostic checks");
        for (label, passed, hint) in &checks {
            println!("  [{}] {} — {}", if *passed { "OK  " } else { "FAIL" }, label, hint);
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
        "Add user to 'tss' group: sudo usermod -aG tss $USER, then log out and back in"
            .to_string(),
    ));

    let context_ok = open_context().is_ok();
    checks.push((
        "TPM2 context opens".to_string(),
        context_ok,
        "Install tpm2-abrmd or ensure /dev/tpmrm0 is accessible".to_string(),
    ));

    checks
}
