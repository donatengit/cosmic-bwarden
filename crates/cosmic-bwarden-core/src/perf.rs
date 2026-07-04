//! Performance measurement tests — `#[ignore]`d so they never run in the
//! normal suite. Run in release mode with output:
//!
//! ```sh
//! RUSTFLAGS="-C target-cpu=native" cargo test --release -p cosmic-bwarden-core \
//!     --lib -- perf --ignored --nocapture --test-threads=1
//! ```
//!
//! Baselines and analysis live in `docs/review/06_performance.md`.

#![cfg(test)]

use crate::api::KdfType;
use crate::cipherstring::CipherString;
use crate::identity::Identity;
use crate::locked;
use std::time::Instant;

fn pw() -> locked::Password {
    locked::Password::from_string("correct horse battery staple")
}

fn median_of<F: FnMut()>(runs: usize, mut f: F) -> std::time::Duration {
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t = Instant::now();
        f();
        samples.push(t.elapsed());
    }
    samples.sort();
    samples[runs / 2]
}

/// Vaultwarden's default KDF for new accounts (PBKDF2-SHA256, 600k).
#[test]
#[ignore = "perf measurement, run explicitly in release"]
fn perf_kdf_pbkdf2_600k() {
    let d = median_of(3, || {
        Identity::new("a@b.com", &pw(), KdfType::Pbkdf2, 600_000, None, None).unwrap();
    });
    eprintln!("PERF kdf_pbkdf2_600k median: {d:?}");
}

/// Bitwarden's default Argon2id parameters (t=3, m=64 MiB, p=4).
#[test]
#[ignore = "perf measurement, run explicitly in release"]
fn perf_kdf_argon2id_default() {
    let d = median_of(3, || {
        Identity::new("a@b.com", &pw(), KdfType::Argon2id, 3, Some(64), Some(4)).unwrap();
    });
    eprintln!("PERF kdf_argon2id_3_64MiB_4 median: {d:?}");
}

/// The sidebar-cache hot path at 5k entries: decrypt name+username for every
/// entry, then one case-insensitive search pass over the results.
#[test]
#[ignore = "perf measurement, run explicitly in release"]
fn perf_vault_decrypt_5k_entries() {
    // Cheap KDF — we're timing decryption, not key derivation.
    let id = Identity::new("a@b.com", &pw(), KdfType::Pbkdf2, 1_000, None, None).unwrap();
    let keys = id.keys;

    let n = 5_000;
    let encrypted: Vec<(String, String)> = (0..n)
        .map(|i| {
            let name =
                CipherString::encrypt_symmetric(&keys, format!("Entry number {i}").as_bytes())
                    .unwrap()
                    .to_string();
            let user =
                CipherString::encrypt_symmetric(&keys, format!("user{i}@example.com").as_bytes())
                    .unwrap()
                    .to_string();
            (name, user)
        })
        .collect();

    let t = Instant::now();
    let decrypted: Vec<(String, String)> = encrypted
        .iter()
        .map(|(n, u)| {
            (
                crate::vault::decrypt(n, &keys, None).unwrap(),
                crate::vault::decrypt(u, &keys, None).unwrap(),
            )
        })
        .collect();
    let decrypt_all = t.elapsed();

    let t = Instant::now();
    let hits = decrypted
        .iter()
        .filter(|(n, u)| n.to_lowercase().contains("4242") || u.to_lowercase().contains("4242"))
        .count();
    let search = t.elapsed();

    eprintln!(
        "PERF vault_decrypt_5k: decrypt_all={decrypt_all:?} ({:?}/entry), search_pass={search:?}, hits={hits}",
        decrypt_all / n
    );
}
