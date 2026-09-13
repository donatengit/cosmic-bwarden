//! XDG project name for Cosmarden (`cosmarden` / `cosmarden-<profile>`).

const PROFILE_ENV: &str = "COSMARDEN_PROFILE";
const LEGACY_PROFILE_ENV: &str = "COSMIC_BWARDEN_PROFILE";

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
        Some(profile) => format!("cosmarden-{profile}"),
        None => "cosmarden".to_string(),
    }
}

fn nonempty_env(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        saved: Vec<(String, Option<std::ffi::OsString>)>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn capture() -> Self {
            let keys = LEGACY_ENV.iter().flat_map(|(n, o)| [*n, *o]);
            Self {
                saved: keys.map(|k| (k.to_string(), std::env::var_os(k))).collect(),
                _lock: ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner()),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (k, v) in &self.saved {
                match v {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
            }
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
    fn config_id_is_com_enikeev_cosmarden() {
        assert_eq!(crate::config::CONFIG_ID, "com.enikeev.cosmarden");
    }
}
