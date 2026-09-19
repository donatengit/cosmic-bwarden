//! Where the vault app (`cosmarden-applet`) is and how to start it.
//!
//! Shared by every frontend that hands off to the full app: the COSMIC applet's
//! activation-token flow and the Plasma tray (`cosmarden-plasma-tray`). Two
//! hand-written resolvers would drift, and the failure mode of a drift is
//! silent — the click does nothing, because the binary was looked up somewhere
//! the other does not look.
//!
//! Resolution order: [`APPLET_BINARY_ENV`], then a sibling of the running
//! executable, then `PATH`. The sibling step is what lets a tray binary find
//! the app: `cosmarden-plasma-tray` is only ever itself, so it must *not*
//! re-spawn the current executable and must look up the applet beside it
//! instead.

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
/// environment.
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
/// the XDG activation token is passed through when the caller has one, so the
/// compositor focuses the new window.
///
/// Kept separate from any spawn so argv and environment are unit-testable — and
/// so a frontend with a different detach mechanism (libcosmic's double fork vs.
/// a reaper thread) still starts the app the same way.
pub fn vault_command(exe: &std::path::Path, token: Option<&str>) -> std::process::Command {
    let mut cmd = std::process::Command::new(exe);
    cmd.env("COSMARDEN_MODE", "application");
    cmd.env_remove("COSMIC_PANEL_NAME");
    if let Some(token) = token {
        cmd.env("XDG_ACTIVATION_TOKEN", token);
        cmd.env("DESKTOP_STARTUP_ID", token);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory with an executable `cosmarden-applet` in it, plus the path
    /// to that binary (the directory doubles as a fake `PATH` entry).
    fn dir_with_applet() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let exe = dir.path().join(APPLET_BINARY);
        std::fs::write(&exe, b"#!/bin/sh\nexit 0\n").expect("write fake applet");
        (dir, exe)
    }

    fn env_of(
        cmd: &std::process::Command,
    ) -> std::collections::HashMap<std::ffi::OsString, Option<std::ffi::OsString>> {
        cmd.get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(std::ffi::OsStr::to_owned)))
            .collect()
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
                Some(&dir.path().join("cosmarden-plasma-tray")),
                None
            ),
            Some(override_path.to_path_buf())
        );
    }

    #[test]
    fn vault_binary_finds_the_sibling_and_never_the_tray_itself() {
        let (dir, exe) = dir_with_applet();
        // Running as `cosmarden-plasma-tray`: resolution must return the
        // applet, not the running tray binary — re-spawning that would start a
        // second tray and never a vault window.
        assert_eq!(
            resolve_vault_binary(None, Some(&dir.path().join("cosmarden-plasma-tray")), None),
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
                Some(&elsewhere.path().join("cosmarden-plasma-tray")),
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
                Some(&empty.path().join("cosmarden-plasma-tray")),
                Some(&path_var)
            ),
            None
        );
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
}
