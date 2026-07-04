//! TOTP code generation from a Bitwarden-stored secret.
//!
//! Bitwarden stores either a bare base32 seed or a full `otpauth://` URL.
//! Both must produce codes that match what Google Authenticator/Aegis/etc.
//! generate for the same secret:
//! - the bare seed is base32 and must be *decoded* to raw key bytes (using the
//!   ASCII of the base32 text as the key silently generates wrong codes);
//! - an `otpauth://` URL carries `algorithm`/`digits`/`period` parameters that
//!   must be honoured, not replaced with SHA1/6/30 defaults.

use totp_rs::{Algorithm, Secret, TOTP};

/// Build a generator from a stored TOTP secret (bare base32 or otpauth URL).
///
/// Uses the `*_unchecked` constructors: RFC 4226 recommends ≥128-bit secrets,
/// but real providers routinely issue 80-bit (10-byte) seeds and authenticator
/// apps accept them — rejecting those would break working accounts.
fn build(secret: &str) -> Result<TOTP, String> {
    if secret.starts_with("otpauth://") {
        return TOTP::from_url_unchecked(secret).map_err(|e| format!("Invalid otpauth URL: {e}"));
    }
    // Bare seed: base32, case-insensitive, often displayed in space-separated
    // groups of four. Normalize, then decode to raw key bytes.
    let normalized: String = secret
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase();
    let key = Secret::Encoded(normalized)
        .to_bytes()
        .map_err(|e| format!("Invalid TOTP secret: {e:?}"))?;
    Ok(TOTP::new_unchecked(
        Algorithm::SHA1,
        6,
        1,
        30,
        key,
        None,
        String::new(),
    ))
}

/// Generate the current TOTP code for a stored secret.
pub(super) fn generate_code(secret: &str) -> Result<String, String> {
    let totp = build(secret)?;
    totp.generate_current()
        .map_err(|e| format!("TOTP generation failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::build;

    // RFC 6238 appendix B SHA1 test vector: the ASCII seed
    // "12345678901234567890" (base32: GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ)
    // at T=59s yields 94287082 (8 digits) / 287082 (6 digits).
    const RFC_SEED_B32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn bare_base32_seed_matches_rfc6238_vector() {
        let totp = build(RFC_SEED_B32).unwrap();
        assert_eq!(totp.generate(59), "287082");
    }

    #[test]
    fn otpauth_url_digits_and_period_are_honoured() {
        let url = format!(
            "otpauth://totp/Example:user@example.com?secret={RFC_SEED_B32}&algorithm=SHA1&digits=8&period=30"
        );
        let totp = build(&url).unwrap();
        // 8 digits — the URL's parameters, not the 6-digit default.
        assert_eq!(totp.generate(59), "94287082");
    }

    // A 10-byte (80-bit) seed — shorter than RFC 4226's 128-bit recommendation
    // but common in the wild (and used by our own E2E test). Must be accepted,
    // and the bare-seed path must agree with the otpauth path for the same
    // secret, proving the bare path base32-decodes rather than using the
    // base32 text as raw key bytes.
    #[test]
    fn short_seed_accepted_and_paths_agree() {
        let seed = "JBSWY3DPEHPK3PXP";
        let bare = build(seed).unwrap();
        let url = build(&format!("otpauth://totp/x?secret={seed}")).unwrap();
        let t = 1_700_000_000;
        assert_eq!(bare.generate(t), url.generate(t));
    }

    #[test]
    fn seed_normalization_spaces_and_case() {
        let a = build("gezd gnbv gy3t qojq gezd gnbv gy3t qojq").unwrap();
        assert_eq!(a.generate(59), "287082");
    }

    #[test]
    fn garbage_secret_is_an_error_not_a_panic() {
        assert!(build("not-base32-!!!").is_err());
        assert!(build("otpauth://totp/x?secret=%%%").is_err());
    }
}
