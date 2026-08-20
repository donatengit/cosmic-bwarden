//! Applet activation-token routing.
//!
//! The compositor echoes back the `exec` string we put in a `TokenRequest`
//! when it grants the token. Today the applet only ever requests
//! `open-vault`, but the ecosystem convention (cosmic-applet-status-area's
//! `activate:<id>` prefix) is to route arbitrary exec strings to named
//! in-process actions. `classify_activation` is the pure decision table;
//! `spawn_vault_app` is the only spawn-based action.

use crate::message::Message;
use cosmic::app::Task;

/// What an activation-token grant for a given `exec` string resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationAction {
    /// Launch the standalone vault window (the applet's `open-vault` exec).
    SpawnVault,
    /// In-process quick action: lock the vault (`activate:lock`).
    Lock,
    /// In-process quick action: generate a password and copy it
    /// (`activate:generate`).
    Generate,
    /// Unknown or malformed exec — do nothing. Deliberately strict: spawning
    /// the vault for an exec we never requested would be surprising.
    Ignore,
}

/// Pure mapping from an activation `exec` string to the action it triggers.
///
/// - `"open-vault"` / `""` — the applet's own launch request (the empty
///   string is the pre-routing fallback where the compositor attached no
///   exec).
/// - `"activate:<name>"` — named in-process quick action, mirroring the
///   `activate:` convention of `cosmic-applet-status-area`.
/// - anything else — ignored.
pub fn classify_activation(exec: &str) -> ActivationAction {
    match exec {
        "" | "open-vault" => ActivationAction::SpawnVault,
        exec if exec.starts_with("activate:") => match &exec["activate:".len()..] {
            "lock" => ActivationAction::Lock,
            "generate" => ActivationAction::Generate,
            _ => ActivationAction::Ignore,
        },
        _ => ActivationAction::Ignore,
    }
}

/// Launch the standalone vault window as a separate process, handing it the
/// XDG activation token so the compositor focuses it properly (the
/// notifications applet's pattern for opening its main window from the
/// applet).
pub fn spawn_vault_app(token: Option<String>) -> Task<Message> {
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = std::process::Command::new(exe);
        cmd.env("COSMIC_BWARDEN_MODE", "application");
        cmd.env_remove("COSMIC_PANEL_NAME");
        if let Some(token) = token {
            cmd.env("XDG_ACTIVATION_TOKEN", &token);
            cmd.env("DESKTOP_STARTUP_ID", &token);
        }
        tokio::spawn(cosmic::process::spawn(cmd));
    }
    Task::none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_vault_exec_spawns() {
        assert_eq!(
            classify_activation("open-vault"),
            ActivationAction::SpawnVault
        );
        assert_eq!(classify_activation(""), ActivationAction::SpawnVault);
    }

    #[test]
    fn activate_prefix_routes_named_actions() {
        assert_eq!(classify_activation("activate:lock"), ActivationAction::Lock);
        assert_eq!(
            classify_activation("activate:generate"),
            ActivationAction::Generate
        );
    }

    #[test]
    fn unknown_execs_are_ignored() {
        assert_eq!(
            classify_activation("activate:bogus"),
            ActivationAction::Ignore
        );
        assert_eq!(classify_activation("activate:"), ActivationAction::Ignore);
        assert_eq!(
            classify_activation("something-else"),
            ActivationAction::Ignore
        );
    }
}
