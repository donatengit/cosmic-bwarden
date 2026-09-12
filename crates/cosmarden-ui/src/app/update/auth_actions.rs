//! Pure `state -> AgentAction` builders for session and TPM-PIN actions.
//!
//! Six of these (`lock`, `logout`, `unlock`, `unlock_with_pin`,
//! `setup_tpm_pin`, `disable_tpm_pin`) were constructed independently in both
//! `auth.rs` (main window) and `applet.rs` (panel applet). Two hand-written
//! copies of the same decision is how the two surfaces drift apart; sharing
//! one builder makes divergence impossible and testable in one place.
//!
//! See `update/vault_actions.rs` for why none of this belongs inside the
//! `Task::perform` async block.

use cosmarden_core::protocol::Action as AgentAction;
use zeroize::Zeroize;

/// Begin a session. The verification code is only meaningful when the server
/// has demanded one, so a blank input must be sent as `None` — an empty string
/// reads as "here is a code" and fails the check.
pub fn login(
    email: String,
    password: String,
    server_url: String,
    remember_me: bool,
    verification_code: &str,
) -> AgentAction {
    AgentAction::Login {
        email,
        password,
        server_url: Some(server_url),
        remember_me,
        two_factor_token: None,
        two_factor_provider: None,
        two_factor_code: None,
        device_verification_code: if verification_code.is_empty() {
            None
        } else {
            Some(verification_code.to_string())
        },
    }
}

/// Unlock with the master password.
pub fn unlock(password: String) -> AgentAction {
    AgentAction::Unlock { password }
}

/// Unlock with the TPM-sealed PIN.
pub fn unlock_with_pin(pin: String) -> AgentAction {
    AgentAction::UnlockWithPin { pin }
}

/// Seal a PIN while the vault is already unlocked (no master-password
/// re-entry needed).
pub fn setup_tpm_pin(pin: String) -> AgentAction {
    AgentAction::SetupTpmPinFromUnlocked { pin }
}

/// Remove the sealed PIN blob.
pub fn disable_tpm_pin() -> AgentAction {
    AgentAction::DisableTpmPin
}

pub fn lock() -> AgentAction {
    AgentAction::Lock
}

pub fn logout() -> AgentAction {
    AgentAction::Logout
}

/// What the PIN field on the master-password unlock screen should do once the
/// vault is open. Each arm routes to a different result message, so this is an
/// enum rather than an `Option<AgentAction>`.
#[derive(Debug)]
pub enum UnlockPinIntent {
    /// Non-empty PIN: reseal it against the current TPM/PCR state. This is how
    /// a user recovers after a firmware or Secure Boot change invalidated the
    /// old blob.
    Reseal(AgentAction),
    /// Empty PIN with a blob still on disk: clear the stale blob.
    ClearStale(AgentAction),
    /// Nothing to do — no TPM, or an empty PIN with nothing sealed. Carries
    /// the unused PIN back so the caller can zeroize it: dropping a plain
    /// `String` leaves the digits sitting in freed memory, and this arm is
    /// reached with a *non-empty* PIN whenever the machine has no TPM.
    Nothing(String),
}

/// Decide what the unlock screen's PIN field means. `pin` is whatever the user
/// left in the box; `tpm_available` and `tpm_configured` describe the machine.
pub fn apply_unlock_pin(tpm_available: bool, tpm_configured: bool, pin: String) -> UnlockPinIntent {
    if !tpm_available {
        return UnlockPinIntent::Nothing(pin);
    }
    if !pin.is_empty() {
        UnlockPinIntent::Reseal(setup_tpm_pin(pin))
    } else if tpm_configured {
        UnlockPinIntent::ClearStale(disable_tpm_pin())
    } else {
        UnlockPinIntent::Nothing(pin)
    }
}

/// Map the login form's PIN toggle onto the same reseal/clear decision as the
/// unlock form. After logout the TPM blob is still on disk (`tpm_configured`),
/// so a login that leaves PIN off must `ClearStale` rather than silently keep
/// leftovers. A non-empty PIN with the toggle on reseals against the new
/// session's vault keys.
pub fn apply_login_pin(
    tpm_available: bool,
    tpm_configured: bool,
    pin_enabled: bool,
    mut pin: String,
) -> UnlockPinIntent {
    if !pin_enabled {
        pin.zeroize();
        pin = String::new();
    }
    apply_unlock_pin(tpm_available, tpm_configured, pin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_verification_code_is_omitted_not_sent_empty() {
        match login(
            "a@b.c".to_string(),
            "pw".to_string(),
            "https://vault".to_string(),
            true,
            "",
        ) {
            AgentAction::Login {
                device_verification_code,
                server_url,
                remember_me,
                ..
            } => {
                assert!(device_verification_code.is_none());
                assert_eq!(server_url.as_deref(), Some("https://vault"));
                assert!(remember_me);
            }
            other => panic!("expected Login, got {}", other.variant_name()),
        }
    }

    #[test]
    fn entered_verification_code_is_forwarded() {
        match login(
            "a@b.c".to_string(),
            "pw".to_string(),
            "https://vault".to_string(),
            false,
            "123456",
        ) {
            AgentAction::Login {
                device_verification_code,
                ..
            } => assert_eq!(device_verification_code.as_deref(), Some("123456")),
            other => panic!("expected Login, got {}", other.variant_name()),
        }
    }

    #[test]
    fn no_tpm_means_the_pin_field_does_nothing() {
        // The PIN comes back so the caller can wipe it rather than leaking it
        // into freed memory on the path that sends nothing.
        match apply_unlock_pin(false, false, "1234".to_string()) {
            UnlockPinIntent::Nothing(leftover) => assert_eq!(leftover, "1234"),
            other => panic!("expected Nothing, got {:?}", other),
        }
        // Even with a blob recorded: without hardware there is nothing to act on.
        assert!(matches!(
            apply_unlock_pin(false, true, String::new()),
            UnlockPinIntent::Nothing(_)
        ));
    }

    #[test]
    fn non_empty_pin_reseals() {
        match apply_unlock_pin(true, false, "1234".to_string()) {
            UnlockPinIntent::Reseal(AgentAction::SetupTpmPinFromUnlocked { pin }) => {
                assert_eq!(pin, "1234");
            }
            other => panic!("expected a reseal, got {:?}", other),
        }
    }

    #[test]
    fn empty_pin_clears_only_an_existing_blob() {
        assert!(matches!(
            apply_unlock_pin(true, true, String::new()),
            UnlockPinIntent::ClearStale(AgentAction::DisableTpmPin)
        ));
        // Nothing sealed: an empty box is not a request to disable anything.
        assert!(matches!(
            apply_unlock_pin(true, false, String::new()),
            UnlockPinIntent::Nothing(_)
        ));
    }

    #[test]
    fn login_pin_off_clears_leftover_blob() {
        // After logout the blob is still on disk. Logging in with the PIN
        // toggle off must disable, not silently keep it.
        assert!(matches!(
            apply_login_pin(true, true, false, String::new()),
            UnlockPinIntent::ClearStale(AgentAction::DisableTpmPin)
        ));
    }

    #[test]
    fn login_pin_off_does_nothing_when_no_blob() {
        assert!(matches!(
            apply_login_pin(true, false, false, String::new()),
            UnlockPinIntent::Nothing(_)
        ));
    }

    #[test]
    fn login_pin_on_reseals() {
        match apply_login_pin(true, true, true, "123456".to_string()) {
            UnlockPinIntent::Reseal(AgentAction::SetupTpmPinFromUnlocked { pin }) => {
                assert_eq!(pin, "123456");
            }
            other => panic!("expected a reseal, got {:?}", other),
        }
    }

    #[test]
    fn login_pin_ignored_without_tpm() {
        assert!(matches!(
            apply_login_pin(false, true, true, "123456".to_string()),
            UnlockPinIntent::Nothing(_)
        ));
    }
}
