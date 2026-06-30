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
use tss_esapi::{
    Context, TctiNameConf,
    attributes::ObjectAttributesBuilder,
    interface_types::{
        algorithm::{HashingAlgorithm, PublicAlgorithm},
        reserved_handles::Hierarchy,
    },
    structures::{
        Auth, Digest, KeyedHashScheme, Private, Public, PublicBuilder,
        PublicKeyedHashParameters, SensitiveData,
        SymmetricCipherParameters, SymmetricDefinitionObject,
    },
    traits::{Marshall, UnMarshall},
};

#[derive(serde::Serialize, serde::Deserialize)]
struct SealedBlob {
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

/// Sealed-data-object template: KeyedHash/Null, user-auth protected, DA enabled.
fn sealed_template(pin: &str) -> Result<(Public, Auth)> {
    let attrs = ObjectAttributesBuilder::new()
        .with_fixed_tpm(true)
        .with_fixed_parent(true)
        .with_st_clear(true)
        .with_user_with_auth(true)
        // with_no_da NOT set → TPM enforces dictionary-attack lockout on wrong PINs
        .build()
        .context("building sealed object attributes")?;

    let template = PublicBuilder::new()
        .with_public_algorithm(PublicAlgorithm::KeyedHash)
        .with_name_hashing_algorithm(HashingAlgorithm::Sha256)
        .with_object_attributes(attrs)
        .with_keyed_hash_parameters(PublicKeyedHashParameters::new(KeyedHashScheme::Null))
        .with_keyed_hash_unique_identifier(Digest::default())
        .build()
        .context("building sealed object template")?;

    let auth = Auth::from_bytes(pin.as_bytes()).context("PIN too long for TPM auth")?;

    Ok((template, auth))
}

/// Seals arbitrary bytes under `pin` (DA-protected when pin is non-empty).
/// Pass `pin = ""` for TPM-bound-only (no dictionary-attack protection).
/// The sealed blob is written to `blob_path` with mode 0600.
pub async fn seal_bytes(data: &[u8], pin: &str, blob_path: &Path) -> Result<()> {
    let mut ctx = open_context()?;

    let primary_tmpl = primary_template()?;
    let primary = ctx
        .execute_with_nullauth_session(|c| {
            c.create_primary(Hierarchy::Owner, primary_tmpl, None, None, None, None)
        })
        .context("creating TPM primary key")?;

    let (tmpl, pin_auth) = sealed_template(pin)?;
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

    let blob = SealedBlob {
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

    log::info!("TPM: sealed {} bytes to {}", data.len(), blob_path.display());
    Ok(())
}

/// Unseals bytes from `blob_path` using `pin` (empty string = no PIN auth).
pub async fn unseal_bytes(blob_path: &Path, pin: &str) -> Result<Vec<u8>> {
    let mut ctx = open_context()?;

    let blob_bytes = std::fs::read(blob_path).context("reading TPM blob file")?;
    let blob: SealedBlob =
        postcard::from_bytes(&blob_bytes).context("deserializing TPM blob")?;

    let private = Private::unmarshall(&blob.out_private)
        .context("deserializing TPM private portion")?;
    let public = Public::unmarshall(&blob.out_public)
        .context("deserializing TPM public portion")?;

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

    let pin_auth = Auth::from_bytes(pin.as_bytes()).context("PIN too long for TPM auth")?;
    ctx.tr_set_auth(obj_handle.into(), pin_auth)
        .context("setting PIN auth on TPM object")?;

    let sensitive = ctx
        .execute_with_nullauth_session(|c| c.unseal(obj_handle.into()))
        .context("TPM unseal — wrong PIN or DA lockout")?;

    let _ = ctx.flush_context(obj_handle.into());

    Ok(sensitive.as_bytes().to_vec())
}

/// Seals `vault_keys` (64 bytes) under `pin` (DA-protected by TPM).
/// The sealed blob is written to `blob_path` with mode 0600.
pub async fn seal(vault_keys: &locked::Keys, pin: &str, blob_path: &Path) -> Result<()> {
    let mut ctx = open_context()?;

    // Create storage-parent primary key inside a null-auth HMAC session.
    let primary_tmpl = primary_template()?;
    let primary = ctx
        .execute_with_nullauth_session(|c| {
            c.create_primary(Hierarchy::Owner, primary_tmpl, None, None, None, None)
        })
        .context("creating TPM primary key")?;

    let (tmpl, pin_auth) = sealed_template(pin)?;
    // Seal only enc_key ‖ mac_key (exactly 64 bytes) — keys.data() may be
    // longer due to PKCS7 padding left over from AES-CBC decryption in the
    // locked::Vec buffer.
    let mut key_material = std::vec::Vec::with_capacity(64);
    key_material.extend_from_slice(vault_keys.enc_key());
    key_material.extend_from_slice(vault_keys.mac_key());
    let key_data = SensitiveData::try_from(key_material)
        .context("vault keys too large for TPM sensitive data")?;

    let result = ctx
        .execute_with_nullauth_session(|c| {
            c.create(
                primary.key_handle,
                tmpl,
                Some(pin_auth),
                Some(key_data),
                None,
                None,
            )
        })
        .context("creating sealed TPM object")?;

    let _ = ctx.flush_context(primary.key_handle.into());

    let blob = SealedBlob {
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

    log::info!("TPM: sealed vault keys to {}", blob_path.display());
    Ok(())
}

/// Unseals the vault keys stored in `blob_path` using `pin`.
/// Fails if the PIN is wrong (TPM DA fault) or the blob is corrupt.
pub async fn unseal(blob_path: &Path, pin: &str) -> Result<locked::Keys> {
    let mut ctx = open_context()?;

    let blob_bytes = std::fs::read(blob_path).context("reading TPM blob file")?;
    let blob: SealedBlob =
        postcard::from_bytes(&blob_bytes).context("deserializing TPM blob")?;

    let private = Private::unmarshall(&blob.out_private)
        .context("deserializing TPM private portion")?;
    let public = Public::unmarshall(&blob.out_public)
        .context("deserializing TPM public portion")?;

    // Recreate the same deterministic parent key.
    let primary_tmpl = primary_template()?;
    let primary = ctx
        .execute_with_nullauth_session(|c| {
            c.create_primary(Hierarchy::Owner, primary_tmpl, None, None, None, None)
        })
        .context("recreating TPM primary key for unseal")?;

    // Load the sealed object under the parent.
    let obj_handle = ctx
        .execute_with_nullauth_session(|c| c.load(primary.key_handle, private, public))
        .context("loading sealed object into TPM")?;

    // Parent no longer needed.
    let _ = ctx.flush_context(primary.key_handle.into());

    // Set the PIN as the auth value so the ESAPI includes it in the HMAC session.
    let pin_auth = Auth::from_bytes(pin.as_bytes()).context("PIN too long for TPM auth")?;
    ctx.tr_set_auth(obj_handle.into(), pin_auth)
        .context("setting PIN auth on TPM object")?;

    // Unseal — wrong PIN triggers TPM DA fault here.
    let sensitive = ctx
        .execute_with_nullauth_session(|c| c.unseal(obj_handle.into()))
        .context("TPM unseal — wrong PIN or DA lockout")?;

    let _ = ctx.flush_context(obj_handle.into());

    // Copy unsealed bytes into locked (mlock'd) memory.
    let raw = sensitive.as_bytes();
    anyhow::ensure!(
        raw.len() == 64,
        "TPM unsealed {} bytes; expected 64 (vault keys)",
        raw.len()
    );

    let mut locked_vec = locked::Vec::new();
    locked_vec.extend(raw.iter().copied());
    Ok(locked::Keys::new(locked_vec))
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
