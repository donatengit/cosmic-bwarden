//! Encrypt-at-rest envelope for the persisted server session (refresh token).
//!
//! A refresh token cannot go into a TPM sealed-data object: `TPM2_MAX_SYM_DATA`
//! caps sealed data at 256 bytes, and a Vaultwarden refresh JWT is ~660 (36-byte
//! header + ~282-byte payload carrying an 88-char device token + 342-byte RS256
//! signature from the server's 2048-bit key). Bitwarden cloud's is larger still.
//!
//! So the token is stored on disk under XChaCha20-Poly1305, keyed by material
//! derived from the vault keys. Those keys exist only after a successful unlock
//! — and for PIN unlock that means the TPM already released them under
//! PolicyPCR(0,7) ∧ PolicyAuthValue. The file therefore inherits the TPM's PCR +
//! PIN binding transitively, with no second sealed object and no second unseal.
//!
//! XChaCha20-Poly1305 rather than AES-256-GCM or the vault's own
//! AES-256-CBC + HMAC-SHA256 (`cipherstring.rs`): the envelope is rewritten on
//! every token refresh, and XChaCha20's 192-bit nonce makes a fresh random
//! nonce safe for unlimited rewrites with no counter to persist and no
//! birthday bound worth reasoning about. It is also an AEAD, so the AAD binds
//! each envelope to one account — a file copied between accounts or profiles
//! fails to open instead of decrypting to a token for the wrong server.

use crate::error::{Error, Result};
use crate::locked;

use chacha20poly1305::aead::{Aead as _, KeyInit as _, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand::TryRngCore as _;
use sha2::Sha256;
use zeroize::{Zeroize as _, Zeroizing};

/// On-disk envelope format version. A file carrying any other version is
/// rejected rather than guessed at; bump only for a breaking layout or
/// algorithm change (and the AAD below changes with it).
pub const ENVELOPE_VERSION: u8 = 1;

/// XChaCha20-Poly1305 nonce length.
const NONCE_LEN: usize = 24;

/// Poly1305 authentication tag length.
const TAG_LEN: usize = 16;

/// HKDF label separating this subkey from every other use of the vault keys.
/// Never reuse the vault's own enc/mac keys directly for a second purpose.
const HKDF_INFO: &[u8] = b"cosmic-bwarden session-envelope v1";

/// Upper bound on a stored envelope. Generous next to a ~660-byte Vaultwarden
/// JWT and a ~2KB cloud one, but bounded so a corrupt or hostile file cannot
/// drive an unbounded read/allocation.
pub const MAX_ENVELOPE_BYTES: usize = 16 * 1024;

/// Additional authenticated data binding an envelope to one account and one
/// format version. Passing a different server/email on open fails the tag
/// check, so an envelope cannot be replayed across accounts or profiles.
pub fn account_aad(server: &str, email: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(2 + server.len() + email.len());
    aad.push(ENVELOPE_VERSION);
    aad.extend_from_slice(server.as_bytes());
    aad.push(0);
    aad.extend_from_slice(email.as_bytes());
    aad
}

/// Derive the envelope subkey from the vault encryption key via HKDF-SHA256.
/// `enc_key` is already a uniformly random 256-bit key (it is itself an HKDF
/// output), so it serves directly as the PRK.
fn derive_key(keys: &locked::Keys) -> Result<Zeroizing<[u8; 32]>> {
    if keys.data().len() < 64 {
        return Err(Error::SessionEnvelope {
            reason: "vault keys too short to derive an envelope key".to_string(),
        });
    }
    let hkdf = Hkdf::<Sha256>::from_prk(keys.enc_key()).map_err(|_| Error::SessionEnvelope {
        reason: "vault encryption key too short for HKDF".to_string(),
    })?;
    let mut subkey = Zeroizing::new([0u8; 32]);
    hkdf.expand(HKDF_INFO, subkey.as_mut())
        .map_err(|_| Error::SessionEnvelope {
            reason: "HKDF expand failed".to_string(),
        })?;
    Ok(subkey)
}

fn cipher_for(keys: &locked::Keys) -> Result<XChaCha20Poly1305> {
    let subkey = derive_key(keys)?;
    XChaCha20Poly1305::new_from_slice(subkey.as_ref()).map_err(|_| Error::SessionEnvelope {
        reason: "invalid AEAD key length".to_string(),
    })
}

/// Encrypt `plaintext` into a self-describing envelope:
/// `version(1) ‖ nonce(24) ‖ ciphertext ‖ tag(16)`.
pub fn seal(keys: &locked::Keys, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let cipher = cipher_for(keys)?;

    // A fresh random nonce per rewrite. Propagated rather than unwrapped: a
    // failing OS RNG must never silently yield a predictable nonce.
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rngs::OsRng
        .try_fill_bytes(&mut nonce_bytes)
        .map_err(|e| Error::SessionEnvelope {
            reason: format!("OS RNG failed while generating a nonce: {e}"),
        })?;

    let ciphertext = cipher
        .encrypt(
            &XNonce::from(nonce_bytes),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| Error::SessionEnvelope {
            reason: "AEAD encryption failed".to_string(),
        })?;

    let mut out = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
    out.push(ENVELOPE_VERSION);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt an envelope produced by [`seal`] into memory-locked storage.
///
/// Fails on a truncated file, an unknown version, a wrong account (AAD
/// mismatch), any tampering, or the wrong vault keys — all of which surface as
/// the same authentication failure, by design.
pub fn open(keys: &locked::Keys, blob: &[u8], aad: &[u8]) -> Result<locked::Vec> {
    if blob.len() > MAX_ENVELOPE_BYTES {
        return Err(Error::SessionEnvelope {
            reason: format!(
                "envelope of {} bytes exceeds the {MAX_ENVELOPE_BYTES}-byte cap",
                blob.len()
            ),
        });
    }
    if blob.len() < 1 + NONCE_LEN + TAG_LEN {
        return Err(Error::SessionEnvelope {
            reason: format!("envelope truncated ({} bytes)", blob.len()),
        });
    }
    if blob[0] != ENVELOPE_VERSION {
        return Err(Error::SessionEnvelope {
            reason: format!(
                "unsupported envelope version {} (expected {ENVELOPE_VERSION})",
                blob[0]
            ),
        });
    }

    let cipher = cipher_for(keys)?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&blob[1..1 + NONCE_LEN]);

    let mut plaintext = cipher
        .decrypt(
            &XNonce::from(nonce_bytes),
            Payload {
                msg: &blob[1 + NONCE_LEN..],
                aad,
            },
        )
        .map_err(|_| Error::SessionEnvelope {
            reason: "envelope authentication failed (wrong account, wrong vault keys, \
                     or the file was modified)"
                .to_string(),
        })?;

    // `Aead::decrypt` can only hand back a plain heap Vec, so the plaintext
    // exists unlocked for the moment it takes to copy it into locked memory.
    // Same transient exposure `locked::Token::from(String)` already accepts for
    // tokens arriving in an HTTP response; zeroized immediately either way.
    let fits = plaintext.len() <= locked::CAPACITY;
    let result = if fits {
        let mut out = locked::Vec::new();
        out.extend(plaintext.iter().copied());
        Ok(out)
    } else {
        // `locked::Vec` is a fixed ArrayVec — extending past capacity panics,
        // and this daemon must never abort on a malformed file.
        Err(Error::SessionEnvelope {
            reason: format!(
                "decrypted session of {} bytes exceeds the {}-byte locked buffer",
                plaintext.len(),
                locked::CAPACITY
            ),
        })
    };
    plaintext.zeroize();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keys(seed: u8) -> locked::Keys {
        let mut v = locked::Vec::new();
        v.extend(std::iter::repeat_n(seed, 64));
        locked::Keys::new(v)
    }

    const TOKEN: &[u8] = b"eyJhbGciOiJSUzI1NiJ9.cGF5bG9hZA.c2lnbmF0dXJl";

    #[test]
    fn roundtrips_through_the_locked_buffer() {
        let keys = test_keys(7);
        let aad = account_aad("https://vault.example", "user@example.com");
        let sealed = seal(&keys, TOKEN, &aad).expect("seal");
        let opened = open(&keys, &sealed, &aad).expect("open");
        assert_eq!(opened.data(), TOKEN);
    }

    /// The whole point of storing it encrypted: the token must not be readable
    /// in the file, and the layout must be the documented one.
    #[test]
    fn envelope_is_versioned_and_hides_the_plaintext() {
        let keys = test_keys(7);
        let aad = account_aad("s", "e");
        let sealed = seal(&keys, TOKEN, &aad).expect("seal");
        assert_eq!(sealed[0], ENVELOPE_VERSION);
        assert_eq!(sealed.len(), 1 + NONCE_LEN + TOKEN.len() + TAG_LEN);
        assert!(
            !sealed.windows(TOKEN.len()).any(|w| w == TOKEN),
            "plaintext token must not appear in the envelope"
        );
    }

    /// A fresh nonce per call: two seals of the same token must differ, or a
    /// rewrite would leak that the token is unchanged.
    #[test]
    fn each_seal_uses_a_fresh_nonce() {
        let keys = test_keys(7);
        let aad = account_aad("s", "e");
        let a = seal(&keys, TOKEN, &aad).expect("seal a");
        let b = seal(&keys, TOKEN, &aad).expect("seal b");
        assert_ne!(a, b, "nonce must not repeat across seals");
    }

    #[test]
    fn wrong_vault_keys_fail_to_open() {
        let aad = account_aad("s", "e");
        let sealed = seal(&test_keys(7), TOKEN, &aad).expect("seal");
        assert!(open(&test_keys(8), &sealed, &aad).is_err());
    }

    /// The AAD binding: an envelope stolen from one account's data dir must not
    /// open under another account, even with the same vault keys.
    #[test]
    fn wrong_account_fails_to_open() {
        let keys = test_keys(7);
        let sealed = seal(&keys, TOKEN, &account_aad("srv-a", "a@example.com")).expect("seal");
        assert!(open(&keys, &sealed, &account_aad("srv-b", "a@example.com")).is_err());
        assert!(open(&keys, &sealed, &account_aad("srv-a", "b@example.com")).is_err());
    }

    #[test]
    fn tampering_is_detected() {
        let keys = test_keys(7);
        let aad = account_aad("s", "e");
        let sealed = seal(&keys, TOKEN, &aad).expect("seal");
        for flip in [1usize, 1 + NONCE_LEN, sealed.len() - 1] {
            let mut bad = sealed.clone();
            bad[flip] ^= 0x01;
            assert!(
                open(&keys, &bad, &aad).is_err(),
                "flipping byte {flip} must fail the tag check"
            );
        }
    }

    /// Malformed input must error, never panic — this runs inside the daemon.
    #[test]
    fn malformed_envelopes_error_without_panicking() {
        let keys = test_keys(7);
        let aad = account_aad("s", "e");
        assert!(open(&keys, &[], &aad).is_err(), "empty");
        assert!(open(&keys, &[ENVELOPE_VERSION; 8], &aad).is_err(), "short");

        let mut wrong_version = seal(&keys, TOKEN, &aad).expect("seal");
        wrong_version[0] = ENVELOPE_VERSION.wrapping_add(1);
        assert!(open(&keys, &wrong_version, &aad).is_err(), "version");

        assert!(
            open(&keys, &vec![ENVELOPE_VERSION; MAX_ENVELOPE_BYTES + 1], &aad).is_err(),
            "over the size cap"
        );
    }

    /// Short vault keys must be rejected, not indexed into (`enc_key()` slices
    /// `[0..32]` and would panic on a truncated buffer).
    #[test]
    fn short_vault_keys_are_rejected() {
        let mut v = locked::Vec::new();
        v.extend(std::iter::repeat_n(1u8, 16));
        let short = locked::Keys::new(v);
        assert!(seal(&short, TOKEN, &account_aad("s", "e")).is_err());
    }
}
