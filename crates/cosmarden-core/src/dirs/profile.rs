//! XDG project name for Cosmarden (`cosmarden` / `cosmarden-<profile>`).

const PROFILE_ENV: &str = "COSMARDEN_PROFILE";

pub fn profile() -> String {
    match std::env::var(PROFILE_ENV) {
        Ok(profile) if !profile.is_empty() => format!("cosmarden-{profile}"),
        _ => "cosmarden".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        profile: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn capture() -> Self {
            Self {
                profile: std::env::var_os(PROFILE_ENV),
                _lock: ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner()),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.profile.as_ref() {
                Some(v) => std::env::set_var(PROFILE_ENV, v),
                None => std::env::remove_var(PROFILE_ENV),
            }
        }
    }

    #[test]
    fn default_profile_is_cosmarden() {
        let _g = EnvGuard::capture();
        std::env::remove_var(PROFILE_ENV);
        assert_eq!(profile(), "cosmarden");
    }

    #[test]
    fn named_profile_is_cosmarden_prefixed() {
        let _g = EnvGuard::capture();
        std::env::set_var(PROFILE_ENV, "foo");
        assert_eq!(profile(), "cosmarden-foo");
    }

    #[test]
    fn config_id_is_com_enikeev_cosmarden() {
        let id = crate::config::CONFIG_ID;
        assert_eq!(id, "com.enikeev.cosmarden");
        for segment in id.split('.') {
            assert!(
                !segment.is_empty()
                    && segment
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "CONFIG_ID segment {segment:?} is not D-Bus-safe"
            );
        }
    }
}
