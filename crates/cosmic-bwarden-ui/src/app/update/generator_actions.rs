//! Pure `state -> AgentAction` builders for the password generator.
//!
//! The `settings` field carries a side effect that is easy to get backwards:
//! `Some(s)` **persists `s` as the new device-wide "last used" settings** and
//! generates with them, while `None` reuses whatever is already stored. Both
//! return a password, so a mixed-up call looks correct on screen and silently
//! rewrites (or silently ignores) the user's saved preferences — exactly the
//! kind of wrong-action bug no test sees while it is built inside a
//! `Task::perform` closure. See `docs/password_generator_plan.md`.

use cosmic_bwarden_core::protocol::{Action as AgentAction, GeneratorSettings};

/// Generate using the settings the user has dialed in on the generator pane,
/// and persist them as the new device-wide default.
pub fn generate_with(settings: GeneratorSettings) -> AgentAction {
    AgentAction::GeneratePassword {
        settings: Some(settings),
    }
}

/// Generate using the stored settings, leaving them untouched. This is what
/// quick-generate surfaces (applet, extension) must use — they have no
/// settings UI, so sending anything else would overwrite the preferences the
/// user set in the main window.
pub fn generate_with_stored() -> AgentAction {
    AgentAction::GeneratePassword { settings: None }
}

/// Drop one entry from the local 7-day history, keyed by its timestamp.
pub fn delete_history_entry(created_at: u64) -> AgentAction {
    AgentAction::DeleteGeneratedPassword { created_at }
}

pub fn fetch_settings() -> AgentAction {
    AgentAction::GetGeneratorSettings
}

pub fn fetch_history() -> AgentAction {
    AgentAction::GetPasswordHistory
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_settings_are_persisted_as_the_new_default() {
        let settings = GeneratorSettings::default();
        match generate_with(settings) {
            AgentAction::GeneratePassword { settings } => {
                assert!(
                    settings.is_some(),
                    "the generator pane must send its settings so they become the stored default"
                );
            }
            other => panic!("expected GeneratePassword, got {}", other.variant_name()),
        }
    }

    #[test]
    fn quick_generate_never_overwrites_stored_settings() {
        match generate_with_stored() {
            AgentAction::GeneratePassword { settings } => {
                assert!(
                    settings.is_none(),
                    "a surface with no settings UI must not rewrite the stored defaults"
                );
            }
            other => panic!("expected GeneratePassword, got {}", other.variant_name()),
        }
    }

    #[test]
    fn history_delete_targets_the_chosen_timestamp() {
        match delete_history_entry(1_785_237_518) {
            AgentAction::DeleteGeneratedPassword { created_at } => {
                assert_eq!(created_at, 1_785_237_518);
            }
            other => panic!(
                "expected DeleteGeneratedPassword, got {}",
                other.variant_name()
            ),
        }
    }
}
