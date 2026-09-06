//! Detects test state leaking into the developer's real home.
//!
//! This module **never deletes anything**. It only lists directories and
//! compares two listings. That is deliberate: an earlier draft of the cleanup
//! plan proposed having `TestEnv::Drop` call `remove_dir_all` on paths derived
//! from `cosmic_bwarden_core::dirs::`, but those functions read process-global
//! env — and `COSMIC_BWARDEN_PROFILE` in this test binary is whatever the last
//! sibling test's `set_var` left behind, or unset. Unset resolves to the
//! *live* `cosmic-bwarden` profile, so such a `Drop` would erase the
//! developer's real vault cache. Detecting a leak is strictly safer than
//! deleting after one.
//!
//! See `docs/test_cleanup_plan.md`.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// The roots a stray agent/CLI spawn can write a profile dir into, resolved
/// exactly the way `directories` resolves them (XDG var, else the `$HOME`
/// default).
fn real_roots() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    // Own uid without pulling in libc/rustix: /proc/self is owned by us.
    let uid = std::fs::metadata("/proc/self")
        .map(|m| {
            use std::os::unix::fs::MetadataExt as _;
            m.uid()
        })
        .unwrap_or(0);

    let with_default = |var: &str, default: Option<PathBuf>| -> Option<PathBuf> {
        std::env::var_os(var)
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .or(default)
    };

    [
        with_default("XDG_CONFIG_HOME", home.as_ref().map(|h| h.join(".config"))),
        with_default("XDG_CACHE_HOME", home.as_ref().map(|h| h.join(".cache"))),
        with_default(
            "XDG_DATA_HOME",
            home.as_ref().map(|h| h.join(".local/share")),
        ),
        with_default(
            "XDG_RUNTIME_DIR",
            Some(PathBuf::from(format!("/run/user/{uid}"))),
        ),
        Some(std::env::temp_dir()),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// A listing of every `cosmic-bwarden*` entry under the real roots.
///
/// Take one before a spawn and another after; anything the spawn added shows
/// up in [`Self::added_since`].
#[derive(Debug, Clone)]
pub struct RealHomeSnapshot {
    entries: BTreeSet<PathBuf>,
}

impl RealHomeSnapshot {
    pub fn take() -> Self {
        let mut entries = BTreeSet::new();
        for root in real_roots() {
            let Ok(dir) = std::fs::read_dir(&root) else {
                // A root that does not exist yet cannot hold residue. If one
                // is created later, the diff surfaces its contents.
                continue;
            };
            for entry in dir.flatten() {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("cosmic-bwarden")
                {
                    entries.insert(entry.path());
                }
            }
        }
        Self { entries }
    }

    /// Paths present now that were absent when `self` was taken.
    pub fn added_since(&self) -> Vec<PathBuf> {
        Self::take()
            .entries
            .difference(&self.entries)
            .cloned()
            .collect()
    }

    /// Fail the calling test if anything new appeared. Use in a test that
    /// deliberately spawns the agent, where the assertion is the point.
    #[track_caller]
    pub fn assert_nothing_added(&self, context: &str) {
        let added = self.added_since();
        assert!(
            added.is_empty(),
            "{context} leaked state into the real home — these paths did not \
             exist before and must not have been created:\n  {}\n\
             The spawn is missing an explicit COSMIC_BWARDEN_PROFILE, HOME, or \
             one of the four XDG_* vars. See docs/test_cleanup_plan.md.",
            added
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n  ")
        );
    }

    /// Report (never panic) — for use from `Drop`, where unwinding would mask
    /// the real test failure or abort the process during another panic. Goes
    /// to stderr rather than `log::error!` because this crate installs no
    /// logger, and a leak that only ever reached a discarded log record would
    /// be exactly the silent failure this guard exists to prevent.
    pub fn report_additions(&self, context: &str) {
        let added = self.added_since();
        if added.is_empty() {
            return;
        }
        let list = added
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n  ");
        eprintln!(
            "[state-guard] ERROR: {context} leaked state into the real home — \
             these paths were created during the test and nothing removed \
             them:\n  {list}\nSee docs/test_cleanup_plan.md."
        );
    }
}

/// Sets `COSMIC_BWARDEN_PROFILE` for the duration of a test and restores the
/// previous value on drop.
///
/// The bare `std::env::set_var` this replaces was process-global and never
/// restored, so the value outlived the test that set it. Any later subprocess
/// spawned without an explicit environment inherited it — that is how
/// `paths.rs` came to run agents under other tests' profiles, and (when it ran
/// before any of them) under the *live* `cosmic-bwarden` profile.
///
/// Hold it in a binding for the whole test body:
/// `let _profile = ProfileEnv::set(&env.profile);`
#[must_use = "the profile is restored when this guard drops; bind it to a variable"]
pub struct ProfileEnv {
    previous: Option<std::ffi::OsString>,
}

impl ProfileEnv {
    pub fn set(profile: &str) -> Self {
        let previous = std::env::var_os("COSMIC_BWARDEN_PROFILE");
        std::env::set_var("COSMIC_BWARDEN_PROFILE", profile);
        Self { previous }
    }
}

impl Drop for ProfileEnv {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(v) => std::env::set_var("COSMIC_BWARDEN_PROFILE", v),
            None => std::env::remove_var("COSMIC_BWARDEN_PROFILE"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `XDG_CACHE_HOME` is process-global, so these two tests must not run
    /// concurrently — and neither may run while another test reads it.
    /// AGENTS.md: "Env overrides are process-global — serialize such tests
    /// behind the helper's lock rather than hoping the scheduler is kind."
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// The snapshot must actually notice a new `cosmic-bwarden*` dir. Written
    /// against a fake root via `XDG_CACHE_HOME` so the assertion never depends
    /// on — or touches — the developer's real directories.
    #[test]
    fn snapshot_detects_a_new_profile_dir() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = tempfile::tempdir().expect("tempdir");
        let cache = temp.path().join("cache");
        std::fs::create_dir_all(&cache).expect("cache dir");

        // Scoped override: this test owns the process env only for its own
        // body, and restores whatever was there before.
        let prev = std::env::var_os("XDG_CACHE_HOME");
        std::env::set_var("XDG_CACHE_HOME", &cache);

        let before = RealHomeSnapshot::take();
        let planted = cache.join("cosmic-bwarden-test-state-guard");
        std::fs::create_dir(&planted).expect("plant dir");
        let added = before.added_since();

        match prev {
            Some(v) => std::env::set_var("XDG_CACHE_HOME", v),
            None => std::env::remove_var("XDG_CACHE_HOME"),
        }

        assert_eq!(
            added,
            vec![planted],
            "the snapshot must report exactly the planted directory"
        );
    }

    /// An unrelated directory in the same root must not be reported.
    #[test]
    fn snapshot_ignores_unrelated_dirs() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = tempfile::tempdir().expect("tempdir");
        let cache = temp.path().join("cache");
        std::fs::create_dir_all(&cache).expect("cache dir");

        let prev = std::env::var_os("XDG_CACHE_HOME");
        std::env::set_var("XDG_CACHE_HOME", &cache);

        let before = RealHomeSnapshot::take();
        std::fs::create_dir(cache.join("some-other-app")).expect("plant dir");
        let added = before.added_since();

        match prev {
            Some(v) => std::env::set_var("XDG_CACHE_HOME", v),
            None => std::env::remove_var("XDG_CACHE_HOME"),
        }

        assert!(
            added.is_empty(),
            "unrelated dirs must be ignored, got {added:?}"
        );
    }
}
