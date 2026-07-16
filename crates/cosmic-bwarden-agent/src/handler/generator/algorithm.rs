//! Charset-based password generation. Pure and stateless: no vault, no I/O.
//!
//! Security-critical invariant: must use a CSPRNG (`rand::rngs::OsRng`), never
//! `rand::rng()`/`ThreadRng` or a seeded RNG (the latter is only ever
//! appropriate in deterministic tests elsewhere in this codebase).

use cosmic_bwarden_core::protocol::GeneratorSettings;
use rand::rngs::OsRng;
use rand::seq::{IndexedRandom as _, SliceRandom as _};
use rand::TryRngCore as _;

pub const MIN_LENGTH: u8 = 8;
pub const MAX_LENGTH: u8 = 32;

const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const DIGITS: &str = "0123456789";
// Bitwarden's basic special-character toggle set. Deliberately excludes
// quotes, backslash, and whitespace: generated passwords routinely land in
// CLI args, JSON payloads, and shell commands across this project's surfaces,
// and those are the characters that actually break something there.
const SPECIAL: &str = "!@#$%^&*";

/// Generate a password matching `settings`. Rejects (does not clamp) invalid
/// input, since callers include untrusted JSON clients (CLI, browser
/// extension) that can send anything.
pub fn generate_password(settings: &GeneratorSettings) -> Result<String, String> {
    if !(MIN_LENGTH..=MAX_LENGTH).contains(&settings.length) {
        return Err(format!(
            "length must be between {MIN_LENGTH} and {MAX_LENGTH}"
        ));
    }

    let mut pools: Vec<Vec<char>> = Vec::with_capacity(4);
    if settings.uppercase {
        pools.push(UPPER.chars().collect());
    }
    if settings.lowercase {
        pools.push(LOWER.chars().collect());
    }
    if settings.numbers {
        pools.push(DIGITS.chars().collect());
    }
    if settings.special {
        pools.push(SPECIAL.chars().collect());
    }
    if pools.is_empty() {
        return Err("select at least one character group".to_string());
    }

    // `OsRng` is fallible in rand 0.9 (`TryRngCore`); `.unwrap_err()` adapts it
    // to the infallible `RngCore` needed by `choose`/`shuffle`, panicking only
    // on genuine OS RNG failure (documented as "highly unlikely" post-boot).
    let mut rng = OsRng.unwrap_err();
    let length = settings.length as usize;

    // Guarantee at least one character from each selected pool.
    let mut chars: Vec<char> = pools
        .iter()
        .map(|pool| *pool.choose(&mut rng).expect("pool is non-empty"))
        .collect();

    // Fill the remainder from the union of selected pools.
    let union: Vec<char> = pools.into_iter().flatten().collect();
    while chars.len() < length {
        chars.push(*union.choose(&mut rng).expect("union is non-empty"));
    }

    chars.shuffle(&mut rng);
    Ok(chars.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(
        uppercase: bool,
        lowercase: bool,
        numbers: bool,
        special: bool,
        length: u8,
    ) -> GeneratorSettings {
        GeneratorSettings {
            uppercase,
            lowercase,
            numbers,
            special,
            length,
        }
    }

    #[test]
    fn rejects_empty_selection() {
        let s = settings(false, false, false, false, 14);
        assert!(generate_password(&s).is_err());
    }

    #[test]
    fn rejects_length_below_minimum() {
        let s = settings(true, true, true, true, 7);
        assert!(generate_password(&s).is_err());
    }

    #[test]
    fn rejects_length_above_maximum() {
        let s = settings(true, true, true, true, 33);
        assert!(generate_password(&s).is_err());
    }

    #[test]
    fn accepts_boundary_lengths() {
        assert!(generate_password(&settings(true, true, true, true, MIN_LENGTH)).is_ok());
        assert!(generate_password(&settings(true, true, true, true, MAX_LENGTH)).is_ok());
    }

    #[test]
    fn output_has_requested_length() {
        for length in [8, 14, 20, 32] {
            let s = settings(true, true, true, true, length);
            let pw = generate_password(&s).unwrap();
            assert_eq!(pw.chars().count(), length as usize);
        }
    }

    #[test]
    fn output_only_contains_selected_charsets() {
        let s = settings(true, false, false, false, 20);
        let pw = generate_password(&s).unwrap();
        assert!(pw.chars().all(|c| UPPER.contains(c)));

        let s = settings(false, false, true, true, 20);
        let pw = generate_password(&s).unwrap();
        assert!(pw
            .chars()
            .all(|c| DIGITS.contains(c) || SPECIAL.contains(c)));
    }

    // Every call forces one char from each selected pool, so — unlike a purely
    // random draw — every selected class is guaranteed present in every single
    // generated string, not just "eventually" across many generations.
    #[test]
    fn every_selected_class_present_in_every_generation() {
        for _ in 0..200 {
            let s = settings(true, true, true, true, MIN_LENGTH);
            let pw = generate_password(&s).unwrap();
            assert!(pw.chars().any(|c| UPPER.contains(c)));
            assert!(pw.chars().any(|c| LOWER.contains(c)));
            assert!(pw.chars().any(|c| DIGITS.contains(c)));
            assert!(pw.chars().any(|c| SPECIAL.contains(c)));
        }
    }

    // Coarse distribution smoke test: catches a broken RNG wiring regression
    // (e.g. accidentally seeding a fixed value) without asserting exact
    // statistical properties.
    #[test]
    fn distribution_smoke_test_no_repeated_identical_output() {
        let s = settings(true, true, true, true, MAX_LENGTH);
        let samples: Vec<String> = (0..50).map(|_| generate_password(&s).unwrap()).collect();
        let unique: std::collections::HashSet<_> = samples.iter().collect();
        assert_eq!(
            unique.len(),
            samples.len(),
            "generated passwords must not repeat"
        );
    }
}
