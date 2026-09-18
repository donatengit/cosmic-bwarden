//! Detached hand-offs: the vault app (the only unlock path this binary has) and
//! the browser.

use cosmarden_core::vault_binary::{self, APPLET_BINARY, APPLET_BINARY_ENV};
use std::process::Command;

/// Start the full app. Nothing here unlocks anything: the app owns the login,
/// PIN and TPM forms, and this process never sees a secret.
pub fn spawn_vault() {
    let Some(exe) = vault_binary::vault_binary() else {
        // Never fail silently: the click asked for the app, and there is no
        // other way to show it.
        tracing::error!(
            "no {APPLET_BINARY} beside this executable or in PATH; \
             set {APPLET_BINARY_ENV} to open the vault window"
        );
        return;
    };
    spawn_detached(
        vault_binary::vault_command(&exe, None),
        format!("vault window ({})", exe.display()),
    );
}

/// Open a link in the default browser.
pub fn open_link(uri: &str) {
    let mut cmd = Command::new("xdg-open");
    cmd.arg(uri);
    spawn_detached(cmd, format!("xdg-open for {uri}"));
}

/// Spawn `cmd` and reap it on a thread.
///
/// A `Child` that is dropped stays a zombie of this process for the rest of its
/// life once it exits, so every spawn here hands the child to a thread that
/// waits on it. (The libcosmic applet uses `cosmic::process::spawn`'s double
/// fork for the same reason; that is not available without libcosmic, and a
/// waiting thread is the smaller mechanism.)
fn spawn_detached(mut cmd: Command, what: String) -> Option<u32> {
    match cmd.spawn() {
        Ok(child) => {
            let pid = child.id();
            tracing::info!(pid, "{what} started");
            std::thread::spawn(move || {
                let mut child = child;
                if let Err(e) = child.wait() {
                    // Fire-and-forget would hide a hand-off that never worked.
                    tracing::warn!("waiting on {what} failed: {e}");
                }
            });
            Some(pid)
        }
        Err(e) => {
            tracing::error!("failed to spawn {what}: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The child must be reaped: `/proc/<pid>` disappears once the waiting
    /// thread has collected it. A dropped `Child` would leave the entry there
    /// as a zombie.
    #[test]
    fn spawned_children_are_reaped() {
        let pid = spawn_detached(Command::new("/bin/true"), "true".to_string())
            .expect("/bin/true must spawn");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"));
            match stat {
                // Gone from the process table: reaped.
                Err(_) => break,
                Ok(stat) => {
                    let state = stat
                        .rfind(')')
                        .map(|i| stat[i + 1..].trim().to_string())
                        .unwrap_or_default();
                    assert!(
                        !state.starts_with('Z'),
                        "child {pid} became a zombie: {stat}"
                    );
                    assert!(
                        std::time::Instant::now() < deadline,
                        "child {pid} never left the process table"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
    }

    /// A binary that does not exist is a logged error, not a panic.
    #[test]
    fn a_missing_program_reports_instead_of_panicking() {
        assert!(spawn_detached(
            Command::new("/nonexistent/cosmarden-test-binary"),
            "missing".to_string()
        )
        .is_none());
    }

    /// The hand-off command is the shared builder: the vault window must start
    /// in application mode, never as a second tray or applet.
    #[test]
    fn the_vault_command_is_the_shared_application_mode_builder() {
        let exe = std::path::Path::new("/usr/bin/cosmarden-applet");
        let cmd = vault_binary::vault_command(exe, None);
        let envs: std::collections::HashMap<_, _> = cmd
            .get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(std::ffi::OsStr::to_owned)))
            .collect();
        assert_eq!(
            envs.get(std::ffi::OsStr::new("COSMARDEN_MODE")),
            Some(&Some(std::ffi::OsString::from("application")))
        );
    }
}
