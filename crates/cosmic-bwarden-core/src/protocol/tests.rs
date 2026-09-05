use super::*;

#[test]
fn action_debug_never_prints_secrets() {
    let a = Action::Unlock {
        password: "hunter2-secret".to_string(),
    };
    assert_eq!(format!("{a:?}"), "Unlock");

    let a = Action::UnlockWithPin {
        pin: "1234-secret".to_string(),
    };
    assert!(!format!("{a:?}").contains("1234-secret"));

    let a = Action::SetupTpmPin {
        master_password: "master-secret".to_string(),
        pin: "pin-secret".to_string(),
    };
    let s = format!("{a:?}");
    assert!(!s.contains("master-secret") && !s.contains("pin-secret"));

    // Non-secret scalar (entry id) is allowed for debuggability.
    let a = Action::GetPassword {
        id: "entry-123".to_string(),
        password: Some("x".into()),
    };
    let s = format!("{a:?}");
    assert!(s.contains("entry-123") && !s.contains("\"x\""));

    // Browser save-prompt actions carry a freshly submitted password.
    let a = Action::CheckLoginMatch {
        domain: "example.com".to_string(),
        username: "user@example.com".to_string(),
        password: "submitted-secret".to_string(),
    };
    assert_eq!(format!("{a:?}"), "CheckLoginMatch");

    let a = Action::UpdateLoginPassword {
        id: "entry-123".to_string(),
        password: "new-secret".to_string(),
    };
    assert_eq!(format!("{a:?}"), "UpdateLoginPassword");

    // GeneratePassword's settings are non-secret (just checkbox/length state),
    // safe to print in full.
    let a = Action::GeneratePassword {
        settings: Some(GeneratorSettings::default()),
    };
    let s = format!("{a:?}");
    assert!(s.contains("GeneratePassword") && s.contains("length"));
}

#[test]
fn response_debug_never_prints_secrets() {
    let r = Response::Password {
        password: "leaked-pw".into(),
    };
    assert!(!format!("{r:?}").contains("leaked-pw"));

    let r = Response::Totp {
        code: "123456".into(),
    };
    assert!(!format!("{r:?}").contains("123456"));

    // Error messages remain visible.
    let r = Response::Error {
        message: "boom".to_string(),
    };
    assert!(format!("{r:?}").contains("boom"));

    let r = Response::GeneratedPassword {
        password: "hunter2-generated".into(),
    };
    assert!(!format!("{r:?}").contains("hunter2-generated"));

    let r = Response::PasswordHistory {
        entries: vec![GeneratorHistoryEntry {
            password: "old-generated-secret".into(),
            created_at: 0,
        }],
    };
    assert!(!format!("{r:?}").contains("old-generated-secret"));
}

// Deterministic mini-fuzz: the agent postcard-decodes `Action` from any
// same-UID client, so the decoder is attacker-reachable (threat A2).
// Arbitrary bytes must produce Ok/Err, never a panic or huge allocation.
#[test]
fn action_decode_from_arbitrary_bytes_never_panics() {
    use rand::{Rng as _, SeedableRng as _};
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xDEC0DE);

    for _ in 0..10_000 {
        let len = rng.random_range(0..128);
        let bytes: Vec<u8> = (0..len).map(|_| rng.random()).collect();
        let _ = postcard::from_bytes::<Action>(&bytes);
    }

    // Truncations of a valid message — the classic framing edge case.
    let valid = postcard::to_allocvec(&Action::Lock).unwrap();
    for cut in 0..valid.len() {
        let _ = postcard::from_bytes::<Action>(&valid[..cut]);
    }
}

/// `Secret` is `#[serde(transparent)]`, so moving these fields from `String`
/// to `Secret` for zeroize-on-drop must not change a single wire byte — no
/// protocol bump, no skew with an older peer. Pinned against the raw postcard
/// encoding of the equivalent plain-string payload.
#[test]
fn secret_fields_are_wire_identical_to_plain_strings() {
    #[derive(serde::Serialize)]
    struct PlainPassword<'a> {
        password: &'a str,
    }

    let secret = postcard::to_allocvec(&Response::Password {
        password: "wire-check".into(),
    })
    .expect("encode Secret form");

    // Same variant index, then the transparent string payload.
    let mut plain = postcard::to_allocvec(&PlainPassword {
        password: "wire-check",
    })
    .expect("encode plain form");
    let mut expected = secret[..secret.len() - plain.len()].to_vec();
    expected.append(&mut plain);

    assert_eq!(
        secret, expected,
        "Secret must serialize exactly like the bare string it replaced"
    );
}
