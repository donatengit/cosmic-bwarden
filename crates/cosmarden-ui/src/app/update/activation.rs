//! Applet activation-token routing and detached process spawning.
//!
//! The compositor echoes back the `exec` string we put in a `TokenRequest`
//! when it grants the token. Today the applet only ever requests
//! `open-vault`, but the ecosystem convention (cosmic-applet-status-area's
//! `activate:<id>` prefix) is to route arbitrary exec strings to named
//! in-process actions. `classify_activation` is the pure decision table;
//! `spawn_vault_app` and `open_link` are the spawn-based actions. Every
//! spawn goes through `cosmic::process::spawn`'s double fork, so no child
//! is ever left behind as an unreaped zombie of the applet.

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
        cmd.env("COSMARDEN_MODE", "application");
        cmd.env_remove("COSMIC_PANEL_NAME");
        if let Some(token) = token {
            cmd.env("XDG_ACTIVATION_TOKEN", &token);
            cmd.env("DESKTOP_STARTUP_ID", &token);
        }
        tokio::spawn(cosmic::process::spawn(cmd));
    }
    Task::none()
}

/// Build the `xdg-open` command for a link: a single exact argument, no
/// shell involved. Kept as its own function so the argv is unit-testable.
fn open_link_command(uri: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(uri);
    cmd
}

/// Detach-spawn `cmd` via the double fork and log a failure if it cannot be
/// launched. Returns the spawned pid when successful.
///
/// A plain `Command::spawn()` whose `Child` is dropped would leave the
/// exited child as a zombie of the applet for the whole panel session —
/// `cosmic::process::spawn` avoids that: the intermediate child is reaped
/// by its own `waitpid`, and the real child is reparented to init, which
/// reaps it on exit.
async fn spawn_detached(cmd: std::process::Command, what: String) -> Option<u32> {
    let pid = cosmic::process::spawn(cmd).await;
    if pid.is_none() {
        // The only signal cosmic::process::spawn gives: fork or exec failed.
        eprintln!("failed to spawn {what}");
    }
    pid
}

/// Open a link in the default browser, detached from the applet (see
/// `spawn_detached` — never a bare dropped `Child`).
pub fn open_link(uri: String) -> Task<Message> {
    tokio::spawn(spawn_detached(
        open_link_command(&uri),
        format!("xdg-open for {uri}"),
    ));
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

    #[test]
    fn open_link_command_targets_xdg_open_with_exact_uri() {
        let cmd = open_link_command("https://example.com/path?q=1");
        assert_eq!(cmd.get_program(), "xdg-open");
        assert_eq!(
            cmd.get_args().collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new("https://example.com/path?q=1")]
        );
    }

    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn spawn_detached_leaves_no_unreaped_direct_child() {
        // Regression test for the zombie bug: a bare `Command::spawn()` with
        // a dropped `Child` leaves the exited helper as a zombie child of
        // the applet for the whole panel session (the anomaly class seen as
        // `Z< [bash] <defunct>` under other applets). The double fork must
        // reparent the real child to init (ppid != ours) or have it reaped
        // before we stop polling.
        let pid = spawn_detached(std::process::Command::new("/bin/true"), "true".to_string())
            .await
            .expect("spawn_detached should launch /bin/true");
        let me = std::process::id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok();
            match stat {
                // Reaped by init — the ideal end state.
                None => break,
                Some(stat) => {
                    // /proc/<pid>/stat: comm is parenthesized and may contain
                    // spaces, so split after the last ')' — field 4 is ppid.
                    let rest = &stat[stat.rfind(')').map(|i| i + 1).unwrap_or(0)..];
                    let ppid: u32 = rest
                        .split_whitespace()
                        .nth(1)
                        .expect("stat ppid field")
                        .parse()
                        .expect("numeric ppid");
                    // Reparented to init; init reaps it on exit. (When the
                    // test itself runs as PID 1 there is no init to defer
                    // to, so nothing further can be asserted.)
                    if ppid != me || me == 1 {
                        break;
                    }
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "detached child {pid} still a direct child of {me} after 5s — zombie leak"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}
