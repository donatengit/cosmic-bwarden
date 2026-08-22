//! Post-login TPM PIN offer. Same reseal/clear decision as the desktop login
//! form: always ask when a TPM is present, and delete leftovers if the user
//! leaves the PIN empty.

use anyhow::{Context, Result};
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};

/// What to do with the user's (possibly empty) PIN after a successful login.
#[derive(Debug, PartialEq, Eq)]
pub enum LoginPinIntent {
    Skip,
    Setup,
    Disable,
}

pub fn login_pin_intent(available: bool, configured: bool, pin: &str) -> LoginPinIntent {
    if !available {
        return LoginPinIntent::Skip;
    }
    if !pin.is_empty() {
        LoginPinIntent::Setup
    } else if configured {
        LoginPinIntent::Disable
    } else {
        LoginPinIntent::Skip
    }
}

/// Offer PIN setup after login. Prompts even when a leftover blob is still on
/// disk (post-logout); an empty answer then disables PIN rather than keeping it.
pub async fn offer_after_login(client: &AgentClient) -> Result<()> {
    let (available, configured) = match client.send(Action::CheckTpm).await {
        Ok(Response::TpmStatus {
            available,
            configured,
            ..
        }) => (available, configured),
        _ => return Ok(()),
    };
    if !available {
        return Ok(());
    }

    if configured {
        eprintln!(
            "PIN unlock is still configured on this device. Enter a PIN (min {} chars) to keep it, or leave empty to disable:",
            cosmic_bwarden_core::MIN_PIN_LEN
        );
    } else {
        eprintln!(
            "TPM2 available. Enter a PIN (min {} chars) to enable PIN unlock, or leave empty to skip:",
            cosmic_bwarden_core::MIN_PIN_LEN
        );
    }

    let pin = match rpassword::prompt_password("PIN: ") {
        Ok(p) => p.trim().to_string(),
        Err(_) => return Ok(()),
    };

    match login_pin_intent(available, configured, &pin) {
        LoginPinIntent::Skip => Ok(()),
        LoginPinIntent::Setup => {
            if pin.chars().count() < cosmic_bwarden_core::MIN_PIN_LEN {
                eprintln!(
                    "PIN must be at least {} characters — skipping.",
                    cosmic_bwarden_core::MIN_PIN_LEN
                );
                return Ok(());
            }
            match client
                .send(Action::SetupTpmPinFromUnlocked { pin })
                .await
                .context("failed to set up TPM PIN")?
            {
                Response::Ack => println!("PIN unlock enabled."),
                Response::Error { message } => eprintln!("PIN setup failed: {}", message),
                _ => eprintln!("Unexpected response from PIN setup"),
            }
            Ok(())
        }
        LoginPinIntent::Disable => {
            match client
                .send(Action::DisableTpmPin)
                .await
                .context("failed to disable TPM PIN")?
            {
                Response::Ack => println!("PIN unlock disabled."),
                Response::Error { message } => eprintln!("PIN disable failed: {}", message),
                _ => eprintln!("Unexpected response from PIN disable"),
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_tpm_skips() {
        assert_eq!(
            login_pin_intent(false, true, "123456"),
            LoginPinIntent::Skip
        );
    }

    #[test]
    fn first_login_empty_skips() {
        assert_eq!(login_pin_intent(true, false, ""), LoginPinIntent::Skip);
    }

    #[test]
    fn first_login_pin_setups() {
        assert_eq!(
            login_pin_intent(true, false, "123456"),
            LoginPinIntent::Setup
        );
    }

    #[test]
    fn leftover_empty_disables() {
        assert_eq!(login_pin_intent(true, true, ""), LoginPinIntent::Disable);
    }

    #[test]
    fn leftover_pin_reseals() {
        assert_eq!(login_pin_intent(true, true, "123456"), LoginPinIntent::Setup);
    }
}
