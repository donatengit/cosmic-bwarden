//! XDG project name for Cosmarden (`cosmarden` / `cosmarden-<profile>`).

const PROFILE_ENV: &str = "COSMARDEN_PROFILE";
const LEGACY_PROFILE_ENV: &str = "COSMIC_BWARDEN_PROFILE";
const NEW_PROFILE: &str = "cosmarden";
const OLD_PROFILE: &str = "cosmic-bwarden";

const LEGACY_ENV: &[(&str, &str)] = &[
    ("COSMARDEN_CONFIG", "COSMIC_BWARDEN_CONFIG"),
    ("COSMARDEN_SOCKET", "COSMIC_BWARDEN_SOCKET"),
    ("COSMARDEN_SSH_SOCKET", "COSMIC_BWARDEN_SSH_SOCKET"),
    ("COSMARDEN_PROFILE", "COSMIC_BWARDEN_PROFILE"),
    ("COSMARDEN_MODE", "COSMIC_BWARDEN_MODE"),
];

/// Copy each pre-rename `COSMIC_BWARDEN_*` into `COSMARDEN_*` when the new
/// name is unset, so clap and `dirs` keep seeing one prefix.
pub fn adopt_legacy_env() {
    for (new, old) in LEGACY_ENV {
        if std::env::var_os(new).is_none() {
            if let Some(v) = std::env::var_os(old) {
                std::env::set_var(new, v);
            }
        }
    }
}

pub fn profile() -> String {
    match nonempty_env(PROFILE_ENV).or_else(|| nonempty_env(LEGACY_PROFILE_ENV)) {
        Some(profile) => format!("{NEW_PROFILE}-{profile}"),
        None => NEW_PROFILE.to_string(),
    }
}

fn nonempty_env(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn project_dirs(name: &str) -> directories::ProjectDirs {
    directories::ProjectDirs::from("", "", name).unwrap()
}

/// Rename `cosmic-bwarden` XDG trees to `cosmarden` when the new tree is
/// absent. One-shot; a failed rename is logged and the old tree is left.
pub fn migrate_legacy_dirs() {
    let new_name = profile();
    let old_name = match new_name.strip_prefix(&format!("{NEW_PROFILE}-")) {
        Some(rest) => format!("{OLD_PROFILE}-{rest}"),
        None if new_name == NEW_PROFILE => OLD_PROFILE.to_string(),
        None => return,
    };
    let new = project_dirs(&new_name);
    let old = project_dirs(&old_name);
    rename_if_needed(new.config_dir(), old.config_dir());
    rename_if_needed(new.cache_dir(), old.cache_dir());
    rename_if_needed(new.data_dir(), old.data_dir());
    if let (Some(n), Some(o)) = (new.runtime_dir(), old.runtime_dir()) {
        rename_if_needed(n, o);
    }

    let cosmic_new = cosmic_app_dir(crate::config::CONFIG_ID);
    for legacy_id in ["com.system76.CosmicBWarden", "com.enikeev.cosmic_bwarden"] {
        let cosmic_old = cosmic_app_dir(legacy_id);
        rename_if_needed(&cosmic_new, &cosmic_old);
        remove_empty_dir(&cosmic_old);
    }
}

fn cosmic_app_dir(app_id: &str) -> std::path::PathBuf {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|b| b.config_dir().to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from(".config"));
    config_home.join("cosmic").join(app_id)
}

fn remove_empty_dir(path: &std::path::Path) {
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let child = entry.path();
            if child.is_dir() {
                remove_empty_dir(&child);
            }
        }
    }
    let _ = std::fs::remove_dir(path);
}

fn rename_if_needed(new: &std::path::Path, old: &std::path::Path) {
    if new.exists() || !old.exists() {
        return;
    }
    match std::fs::rename(old, new) {
        Ok(()) => log::info!(
            "migrated profile dir {} -> {}",
            old.display(),
            new.display()
        ),
        Err(e) => log::error!(
            "failed to migrate profile dir {} -> {}: {}; leaving the old tree",
            old.display(),
            new.display(),
            e
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        saved: Vec<(String, Option<std::ffi::OsString>)>,
        xdg: Vec<(String, Option<std::ffi::OsString>)>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn capture() -> Self {
            let keys: Vec<&str> = LEGACY_ENV
                .iter()
                .flat_map(|(n, o)| [*n, *o])
                .chain([
                    "HOME",
                    "XDG_CONFIG_HOME",
                    "XDG_CACHE_HOME",
                    "XDG_DATA_HOME",
                    "XDG_RUNTIME_DIR",
                ])
                .collect();
            let mut saved = Vec::new();
            let mut xdg = Vec::new();
            for k in keys {
                let slot = if k.starts_with("XDG_") || k == "HOME" {
                    &mut xdg
                } else {
                    &mut saved
                };
                slot.push((k.to_string(), std::env::var_os(k)));
            }
            Self {
                saved,
                xdg,
                _lock: ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner()),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (k, v) in self.saved.iter().chain(self.xdg.iter()) {
                match v {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
            }
        }
    }

    fn redirect_xdg(root: &std::path::Path) {
        std::env::set_var("HOME", root);
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_CACHE_HOME", root.join("cache"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_RUNTIME_DIR", root.join("runtime"));
        for d in ["config", "cache", "data", "runtime"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
    }

    #[test]
    fn default_profile_is_cosmarden() {
        let _g = EnvGuard::capture();
        std::env::remove_var(PROFILE_ENV);
        std::env::remove_var(LEGACY_PROFILE_ENV);
        assert_eq!(profile(), "cosmarden");
    }

    #[test]
    fn named_profile_is_cosmarden_prefixed() {
        let _g = EnvGuard::capture();
        std::env::remove_var(LEGACY_PROFILE_ENV);
        std::env::set_var(PROFILE_ENV, "foo");
        assert_eq!(profile(), "cosmarden-foo");
    }

    #[test]
    fn legacy_profile_env_is_an_alias() {
        let _g = EnvGuard::capture();
        std::env::remove_var(PROFILE_ENV);
        std::env::set_var(LEGACY_PROFILE_ENV, "bar");
        assert_eq!(profile(), "cosmarden-bar");
    }

    #[test]
    fn adopt_legacy_env_fills_unset_new_names() {
        let _g = EnvGuard::capture();
        std::env::remove_var("COSMARDEN_SOCKET");
        std::env::set_var("COSMIC_BWARDEN_SOCKET", "/tmp/old.sock");
        adopt_legacy_env();
        assert_eq!(std::env::var("COSMARDEN_SOCKET").unwrap(), "/tmp/old.sock");
    }

    #[test]
    fn migrate_legacy_dirs_renames_old_tree() {
        let _g = EnvGuard::capture();
        std::env::remove_var(PROFILE_ENV);
        std::env::remove_var(LEGACY_PROFILE_ENV);
        std::env::remove_var("COSMARDEN_CONFIG");
        std::env::remove_var("COSMIC_BWARDEN_CONFIG");
        let tmp = tempfile::TempDir::new().unwrap();
        redirect_xdg(tmp.path());
        let old = project_dirs("cosmic-bwarden").config_dir().to_path_buf();
        let new = project_dirs("cosmarden").config_dir().to_path_buf();
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("config.json"), "{\"migrated\":true}").unwrap();
        migrate_legacy_dirs();
        assert!(new.join("config.json").is_file(), "new={new:?} old={old:?}");
        assert!(!old.exists(), "old tree still present: {old:?}");
        assert_eq!(
            std::fs::read_to_string(crate::dirs::config_file()).unwrap(),
            "{\"migrated\":true}"
        );
    }

    #[test]
    fn migrate_legacy_dirs_moves_cosmic_config_app_id() {
        let _g = EnvGuard::capture();
        std::env::remove_var(PROFILE_ENV);
        std::env::remove_var(LEGACY_PROFILE_ENV);
        let tmp = tempfile::TempDir::new().unwrap();
        redirect_xdg(tmp.path());
        let old = cosmic_app_dir("com.system76.CosmicBWarden");
        let new = cosmic_app_dir(crate::config::CONFIG_ID);
        std::fs::create_dir_all(old.join("v1")).unwrap();
        std::fs::write(old.join("v1").join("config.ron"), "()").unwrap();
        migrate_legacy_dirs();
        assert!(new.join("v1").join("config.ron").is_file());
        assert!(!old.exists());
    }

    #[test]
    fn migrate_legacy_dirs_drops_empty_cosmic_config_leftover() {
        let _g = EnvGuard::capture();
        std::env::remove_var(PROFILE_ENV);
        std::env::remove_var(LEGACY_PROFILE_ENV);
        let tmp = tempfile::TempDir::new().unwrap();
        redirect_xdg(tmp.path());
        let old = cosmic_app_dir("com.system76.CosmicBWarden");
        std::fs::create_dir_all(old.join("v1")).unwrap();
        std::fs::create_dir_all(cosmic_app_dir(crate::config::CONFIG_ID)).unwrap();
        migrate_legacy_dirs();
        assert!(!old.exists());
    }

    #[test]
    fn config_id_is_com_enikeev_cosmarden() {
        let id = crate::config::CONFIG_ID;
        assert_eq!(id, "com.enikeev.cosmarden");
    }
}
