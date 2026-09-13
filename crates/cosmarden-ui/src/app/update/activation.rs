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

/// File name of the vault-window binary. The same in every install layout:
/// `target/release`, `~/.local/bin` (`just install`) and `/usr/bin` (deb, AUR).
pub const APPLET_BINARY: &str = "cosmarden-applet";

/// Overrides the vault-window binary for layouts that keep it elsewhere
/// (packaging, tests). Unset in normal installs.
pub const APPLET_BINARY_ENV: &str = "COSMARDEN_APPLET_BIN";

/// Resolution order for the vault-window binary: [`APPLET_BINARY_ENV`], then a
/// sibling of the running executable, then `PATH`.
///
/// Pure so the order is unit-tested; [`vault_binary`] supplies the process
/// environment. The sibling step is what lets the Plasma tray open the full
/// app: `cosmarden-tray` must never re-spawn itself (it only ever calls
/// `run_tray`), so it looks up the `cosmarden-applet` beside it — which is the
/// running binary itself when the applet is the one asking.
pub fn resolve_vault_binary(
    override_path: Option<&std::path::Path>,
    current_exe: Option<&std::path::Path>,
    path_var: Option<&std::ffi::OsStr>,
) -> Option<std::path::PathBuf> {
    if let Some(path) = override_path {
        return Some(path.to_path_buf());
    }
    if let Some(dir) = current_exe.and_then(std::path::Path::parent) {
        let sibling = dir.join(APPLET_BINARY);
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    std::env::split_paths(path_var?)
        .map(|dir| dir.join(APPLET_BINARY))
        .find(|candidate| candidate.is_file())
}

/// [`resolve_vault_binary`] against the real environment.
pub fn vault_binary() -> Option<std::path::PathBuf> {
    resolve_vault_binary(
        std::env::var_os(APPLET_BINARY_ENV)
            .map(std::path::PathBuf::from)
            .as_deref(),
        std::env::current_exe().ok().as_deref(),
        std::env::var_os("PATH").as_deref(),
    )
}

/// The vault window's argv and environment: the applet binary in application
/// mode. `COSMIC_PANEL_NAME` is removed so it does not start as a panel applet;
/// the XDG activation token is passed through when we have one, so the
/// compositor focuses the new window. Kept separate from the spawn (the same
/// split as `open_link_command`) so argv and environment are unit-testable.
fn vault_command(exe: &std::path::Path, token: Option<&str>) -> std::process::Command {
    let mut cmd = std::process::Command::new(exe);
    cmd.env("COSMARDEN_MODE", "application");
    cmd.env_remove("COSMIC_PANEL_NAME");
    if let Some(token) = token {
        cmd.env("XDG_ACTIVATION_TOKEN", token);
        cmd.env("DESKTOP_STARTUP_ID", token);
    }
    cmd
}

/// Launch the standalone vault window, handing it the XDG activation token so
/// the compositor focuses it properly (the notifications applet's pattern for
/// opening its main window from the applet).
///
/// Two callers: the COSMIC panel's activation-token flow, where the running
/// binary already *is* the vault binary, and the Plasma tray's `Open Vault`,
/// which has no token channel and resolves its sibling binary instead.
pub fn spawn_vault_app(token: Option<String>) -> Task<Message> {
    let Some(exe) = vault_binary() else {
        // Never fail silently: the click asked for the full app, and there is
        // no way to show it.
        tracing::error!(
            "no {APPLET_BINARY} beside this executable or in PATH; \
             set {APPLET_BINARY_ENV} to open the vault window"
        );
        return Task::none();
    };
    spawn_vault(&exe, token);
    Task::none()
}

fn spawn_vault(exe: &std::path::Path, token: Option<String>) {
    tokio::spawn(spawn_detached(
        vault_command(exe, token.as_deref()),
        format!("vault window ({})", exe.display()),
    ));
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

    /// A directory with an executable `cosmarden-applet` in it, plus the path
    /// to that binary (the directory doubles as a fake `PATH` entry).
    fn dir_with_applet() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let exe = dir.path().join(APPLET_BINARY);
        std::fs::write(&exe, b"#!/bin/sh\nexit 0\n").expect("write fake applet");
        (dir, exe)
    }

    #[test]
    fn vault_binary_prefers_the_override() {
        let (dir, _) = dir_with_applet();
        let override_path = std::path::Path::new("/opt/cosmarden/cosmarden-applet");
        // The override wins even when a sibling exists: explicit config beats
        // discovery.
        assert_eq!(
            resolve_vault_binary(
                Some(override_path),
                Some(&dir.path().join("cosmarden-tray")),
                None
            ),
            Some(override_path.to_path_buf())
        );
    }

    #[test]
    fn vault_binary_finds_the_sibling_and_never_the_tray_itself() {
        let (dir, exe) = dir_with_applet();
        // Running as `cosmarden-tray`: resolution must return the applet, not
        // the running tray binary — re-spawning that would start a second tray
        // and never a vault window.
        assert_eq!(
            resolve_vault_binary(None, Some(&dir.path().join("cosmarden-tray")), None),
            Some(exe)
        );
    }

    #[test]
    fn vault_binary_falls_back_to_path() {
        let (dir, exe) = dir_with_applet();
        let elsewhere = tempfile::tempdir().expect("tempdir");
        let path_var = std::env::join_paths([elsewhere.path(), dir.path()]).expect("join PATH");
        assert_eq!(
            resolve_vault_binary(
                None,
                Some(&elsewhere.path().join("cosmarden-tray")),
                Some(&path_var)
            ),
            Some(exe)
        );
    }

    #[test]
    fn vault_binary_is_none_when_nothing_resolves() {
        let empty = tempfile::tempdir().expect("tempdir");
        let path_var = std::env::join_paths([empty.path()]).expect("join PATH");
        assert_eq!(
            resolve_vault_binary(
                None,
                Some(&empty.path().join("cosmarden-tray")),
                Some(&path_var)
            ),
            None
        );
    }

    fn env_of(
        cmd: &std::process::Command,
    ) -> std::collections::HashMap<std::ffi::OsString, Option<std::ffi::OsString>> {
        cmd.get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(std::ffi::OsStr::to_owned)))
            .collect()
    }

    #[test]
    fn vault_command_runs_the_applet_in_application_mode() {
        let exe = std::path::Path::new("/usr/bin/cosmarden-applet");
        let cmd = vault_command(exe, None);
        assert_eq!(cmd.get_program(), exe);
        let envs = env_of(&cmd);
        assert_eq!(
            envs.get(std::ffi::OsStr::new("COSMARDEN_MODE")),
            Some(&Some(std::ffi::OsString::from("application"))),
            "the vault binary must not start in applet or tray mode"
        );
        assert_eq!(
            envs.get(std::ffi::OsStr::new("COSMIC_PANEL_NAME")),
            Some(&None),
            "a panel variable would make it start as an applet"
        );
        assert!(!envs.contains_key(std::ffi::OsStr::new("XDG_ACTIVATION_TOKEN")));
    }

    #[test]
    fn vault_command_passes_the_activation_token_when_given() {
        let cmd = vault_command(
            std::path::Path::new("/usr/bin/cosmarden-applet"),
            Some("t0k3n"),
        );
        let envs = env_of(&cmd);
        for key in ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"] {
            assert_eq!(
                envs.get(std::ffi::OsStr::new(key)),
                Some(&Some(std::ffi::OsString::from("t0k3n"))),
                "{key} must carry the token"
            );
        }
    }

    /// End-to-end spawn (real fork+exec, no panel needed): the script exits
    /// non-zero unless it was started as the vault window, and writes the token
    /// it saw. Existence of the marker therefore proves both.
    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn spawn_vault_starts_the_binary_as_the_vault_window() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("marker");
        let script = dir.path().join(APPLET_BINARY);
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\n\
                 [ \"$COSMARDEN_MODE\" = application ] || exit 3\n\
                 [ -z \"${{COSMIC_PANEL_NAME:-}}\" ] || exit 4\n\
                 printf '%s' \"${{XDG_ACTIVATION_TOKEN:-}}\" > {}\n",
                marker.display()
            ),
        )
        .expect("write fake applet");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake applet");

        spawn_vault(&script, Some("t0k3n".to_string()));

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Ok(token) = std::fs::read_to_string(&marker) {
                assert_eq!(
                    token, "t0k3n",
                    "the activation token must reach the spawned vault window"
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "spawned vault window never ran: COSMARDEN_MODE / COSMIC_PANEL_NAME wrong?"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
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
