//! Integration tests that hit the REAL TPM device; excluded from normal runs.
//!
//! Run with:
//!   cargo test -p cosmic-bwarden-agent --features tpm -- --ignored
//!
//! Prerequisites: user must be in the `tss` group (or run as root), and a
//! TPM2 device must be accessible at /dev/tpmrm0 or /dev/tpm0.

use super::*;
use cosmic_bwarden_core::locked;

fn make_test_keys(enc_seed: u8, mac_seed: u8) -> locked::Keys {
    let mut v = locked::Vec::new();
    // 32-byte enc key: repeating enc_seed
    v.extend(std::iter::repeat_n(enc_seed, 32));
    // 32-byte mac key: repeating mac_seed
    v.extend(std::iter::repeat_n(mac_seed, 32));
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
    let unsealed = unseal(&path, pin)
        .await
        .expect("unseal with correct PIN failed");
    assert_eq!(
        unsealed.enc_key(),
        keys.enc_key(),
        "enc_key mismatch after round-trip"
    );
    assert_eq!(
        unsealed.mac_key(),
        keys.mac_key(),
        "mac_key mismatch after round-trip"
    );

    // ── wrong PIN → rejected ──
    let wrong = unseal(&path, wrong_pin).await;
    assert!(wrong.is_err(), "wrong PIN should have been rejected by TPM");

    // ── correct PIN still works after one wrong attempt ──
    // (TPM DA lockout triggers only after many consecutive failures)
    let after_wrong = unseal(&path, pin)
        .await
        .expect("correct PIN should still work after one wrong attempt");
    assert_eq!(after_wrong.enc_key(), keys.enc_key());
    assert_eq!(after_wrong.mac_key(), keys.mac_key());

    // ── clear removes the blob ──
    clear(&path).expect("clear failed");
    assert!(!path.exists(), "blob file still exists after clear");

    // ── unseal after clear → file-not-found ──
    let after_clear = unseal(&path, pin).await;
    assert!(
        after_clear.is_err(),
        "unseal after clear should fail with missing file"
    );
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

    seal(&keys_a, "pinForA", &path_a)
        .await
        .expect("seal A failed");
    seal(&keys_b, "pinForB", &path_b)
        .await
        .expect("seal B failed");

    let u_a = unseal(&path_a, "pinForA").await.expect("unseal A failed");
    let u_b = unseal(&path_b, "pinForB").await.expect("unseal B failed");

    assert_eq!(
        u_a.enc_key(),
        keys_a.enc_key(),
        "account A enc_key corrupted"
    );
    assert_eq!(
        u_b.enc_key(),
        keys_b.enc_key(),
        "account B enc_key corrupted"
    );

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

    seal(&keys, "oldpin", &path)
        .await
        .expect("first seal failed");
    seal(&keys, "newpin", &path)
        .await
        .expect("second seal (re-seal) failed");

    // Old PIN no longer works
    let old = unseal(&path, "oldpin").await;
    assert!(old.is_err(), "old PIN should be rejected after re-seal");

    // New PIN works
    let new = unseal(&path, "newpin")
        .await
        .expect("new PIN should work after re-seal");
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
        println!(
            "  [{}] {} — {}",
            if *passed { "OK  " } else { "FAIL" },
            label,
            hint
        );
    }
}

/// `classify_unseal_failure` maps the TSS response codes to user-actionable
/// categories: wrong PIN (DA attempt consumed), changed PCR state (recovery:
/// master password), DA lockout, and everything else. No TPM required — the
/// errors are synthesized.
#[test]
fn classify_unseal_failure_maps_tss_codes() {
    use tss_esapi::error::{ReturnCode, TpmResponseCode};

    let classify = |rc: u16| {
        let err = anyhow::Error::from(tss_esapi::Error::TssError(ReturnCode::Tpm(
            TpmResponseCode::try_from(rc).expect("valid TPM response code"),
        )));
        classify_unseal_failure(&err)
    };

    // TPM_RC_AUTH_FAIL (0x08E): wrong PIN, DA counter incremented.
    assert_eq!(classify(0x08E), UnsealFailure::WrongPin);
    // TPM_RC_POLICY_FAIL (0x09D): policy check failed (PCR state changed).
    assert_eq!(classify(0x09D), UnsealFailure::StateChanged);
    // TPM_RC_LOCKOUT (0x921): TPM in dictionary-attack lockout.
    assert_eq!(classify(0x921), UnsealFailure::Lockout);
    // Some unrelated format-one error → Other.
    assert_eq!(classify(0x097), UnsealFailure::Other);

    // The same code wrapped in anyhow `.context()` layers still classifies —
    // this is how the error arrives at the unlock handler.
    let wrapped = anyhow::Error::from(tss_esapi::Error::TssError(ReturnCode::Tpm(
        TpmResponseCode::try_from(0x09D).unwrap(),
    )))
    .context("TPM unseal — wrong PIN, changed PCRs, or DA lockout");
    assert_eq!(classify_unseal_failure(&wrapped), UnsealFailure::StateChanged);

    // Non-TSS errors classify as Other.
    assert_eq!(
        classify_unseal_failure(&anyhow::anyhow!("no TPM device")),
        UnsealFailure::Other
    );
}
