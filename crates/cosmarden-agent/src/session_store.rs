//! Persist the server refresh token across agent restarts, encrypted under the
//! vault keys (`cosmarden_core::session_envelope`).
//!
//! This is what lets a PIN unlock restore server sync after a reboot, and it is
//! the *only* credential this device keeps for that purpose. A refresh grant is
//! deliberately weak: revocable server-side (deleting the device, or any
//! master-password change, rotates it), self-expiring, unable to change the
//! account, and exempt from 2FA — so it also works on accounts a silent
//! password grant could never satisfy. No master-password hash is stored
//! anywhere; when this token stops working the user is asked for it instead.
//!
//! The envelope opens only with the vault keys, which for PIN unlock the TPM
//! releases under PolicyPCR(0,7) ∧ PolicyAuthValue. Losing the file costs a
//! master-password unlock, never data.
//!
//! Deliberately **not** gated on `persist_session` ("remember me"), which gates
//! the Secret Service keyring. That flag is about staying signed in without an
//! unlock; this file grants nothing without one, exactly like the vault cache
//! next to it, which also holds encrypted account data regardless of the flag.
//! Gating it would push the users who decline "remember me" onto the sealed
//! master-password hash — the *stronger* credential — which is backwards.
//! Logout deletes it either way.

use crate::state::State;
use cosmarden_core::locked;
use cosmarden_core::session_envelope::{self, MAX_ENVELOPE_BYTES};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use zeroize::Zeroizing;

/// Persist whatever refresh token `state` currently holds, if the vault is
/// unlocked. Call this on every path that mints or rotates a token (login,
/// silent re-auth, refresh) so the stored envelope tracks the live session and
/// its validity window keeps rolling forward instead of aging out.
///
/// Errors are logged, never propagated: every caller is on a success path that
/// must not fail because persistence did. They log at `error!` because a lost
/// envelope means the next PIN unlock silently drops to "no sync".
pub async fn persist_current(state: &Arc<Mutex<State>>, server: &str, email: &str) {
    let (keys, token) = {
        let g = state.lock().await;
        let Some(keys) = g.keys.clone() else {
            // Re-locked while the network call was in flight; the next unlock
            // persists. Nothing lost.
            return;
        };
        let token =
            g.db.as_ref()
                .and_then(|db| db.refresh_token.as_ref())
                .map(|t| Zeroizing::new(t.expose().to_string()));
        (keys, token)
    };

    let Some(token) = token else {
        log::error!(
            "no refresh token to persist for {email} — a PIN unlock after restart \
             will have no session to restore"
        );
        return;
    };
    if let Err(e) = save(&keys, server, email, &token) {
        log::error!("failed to persist the session envelope for {email}: {e:#}");
    }
}

/// Encrypt `refresh_token` and write it to the per-account session file.
///
/// Errors are the caller's to log: a lost session means the next PIN unlock
/// silently drops to "no sync", which is exactly the failure this module
/// exists to remove.
pub fn save(
    keys: &locked::Keys,
    server: &str,
    email: &str,
    refresh_token: &str,
) -> anyhow::Result<()> {
    save_to(
        &cosmarden_core::dirs::session_file(server, email),
        keys,
        server,
        email,
        refresh_token,
    )
}

/// Read and decrypt the stored refresh token, if one exists.
///
/// `Ok(None)` means "nothing stored" — the ordinary state before the first
/// login. Anything else (truncated, wrong account, wrong keys, tampered) is an
/// error, so a real problem is never mistaken for a fresh install.
pub fn load(
    keys: &locked::Keys,
    server: &str,
    email: &str,
) -> anyhow::Result<Option<locked::Token>> {
    load_from(
        &cosmarden_core::dirs::session_file(server, email),
        keys,
        server,
        email,
    )
}

/// Remove the stored session. Missing file is success (already gone); any
/// other unlink error is returned so logout cannot report a clean sweep while
/// a usable refresh token is still on disk.
pub fn clear(server: &str, email: &str) -> anyhow::Result<()> {
    remove_if_present(&cosmarden_core::dirs::session_file(server, email))
}

/// Write the envelope to `file` (0600, temp-file + rename so a crash mid-write
/// cannot truncate it).
fn save_to(
    file: &Path,
    keys: &locked::Keys,
    server: &str,
    email: &str,
    refresh_token: &str,
) -> anyhow::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;

    let aad = session_envelope::account_aad(server, email);
    let blob = session_envelope::seal(keys, refresh_token.as_bytes(), &aad)?;

    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = file.with_extension("enc.tmp");
    {
        let mut fh = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        fh.write_all(&blob)?;
        fh.sync_all()?;
    }
    std::fs::rename(&tmp, file)?;
    log::debug!("session envelope written to {}", file.display());
    Ok(())
}

fn load_from(
    file: &Path,
    keys: &locked::Keys,
    server: &str,
    email: &str,
) -> anyhow::Result<Option<locked::Token>> {
    let blob = match std::fs::read(file) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if blob.len() > MAX_ENVELOPE_BYTES {
        anyhow::bail!(
            "session envelope {} is {} bytes, past the {MAX_ENVELOPE_BYTES}-byte cap",
            file.display(),
            blob.len()
        );
    }

    let aad = session_envelope::account_aad(server, email);
    let plaintext = session_envelope::open(keys, &blob, &aad)?;
    let token = std::str::from_utf8(plaintext.data())
        .map_err(|e| anyhow::anyhow!("stored session is not valid UTF-8: {e}"))?;
    Ok(Some(locked::Token::from_string(token)))
}

fn remove_if_present(file: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(file) {
        Ok(()) => {
            log::info!("session envelope cleared: {}", file.display());
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "failed to remove session envelope {}: {e}",
            file.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(seed: u8) -> locked::Keys {
        let mut v = locked::Vec::new();
        v.extend(std::iter::repeat_n(seed, 64));
        locked::Keys::new(v)
    }

    /// A scratch file under the OS temp dir. Deliberately NOT a `dirs::*` path:
    /// those are process-global and a unit test that reaches one overwrites the
    /// developer's own account (AGENTS.md, "tests must never touch real user
    /// state"). Filesystem coverage of the real paths belongs to the E2E suite,
    /// which can isolate a data dir per test.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "cosmarden-session-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            std::fs::create_dir_all(&dir).expect("scratch dir");
            Self(dir.join("session.enc"))
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            if let Some(parent) = self.0.parent() {
                let _ = std::fs::remove_dir_all(parent);
            }
        }
    }

    const TOKEN: &str = "eyJhbGciOiJSUzI1NiJ9.cGF5bG9hZA.c2ln";

    #[test]
    fn save_then_load_roundtrips() {
        let f = Scratch::new("roundtrip");
        let k = keys(3);

        assert!(
            load_from(f.path(), &k, "s", "e")
                .expect("load on empty")
                .is_none(),
            "no file yet must be Ok(None), not an error"
        );

        save_to(f.path(), &k, "s", "e", TOKEN).expect("save");
        let got = load_from(f.path(), &k, "s", "e")
            .expect("load")
            .expect("some");
        assert_eq!(got.expose(), TOKEN);
    }

    #[test]
    fn stored_file_is_0600_and_holds_no_plaintext() {
        use std::os::unix::fs::PermissionsExt as _;
        let f = Scratch::new("perms");
        save_to(f.path(), &keys(3), "s", "e", TOKEN).expect("save");

        let mode = std::fs::metadata(f.path())
            .expect("stat")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "session envelope must be 0600");

        let raw = std::fs::read(f.path()).expect("read");
        assert!(
            !raw.windows(TOKEN.len()).any(|w| w == TOKEN.as_bytes()),
            "token must never be on disk in the clear"
        );
    }

    /// Wrong keys must be an error, not a silent `None` — otherwise a real
    /// problem reads as "no session stored" and the cause is never logged.
    #[test]
    fn wrong_keys_error_rather_than_returning_none() {
        let f = Scratch::new("wrongkeys");
        save_to(f.path(), &keys(3), "s", "e", TOKEN).expect("save");
        assert!(load_from(f.path(), &keys(4), "s", "e").is_err());
    }

    /// The AAD binding, end to end: an envelope moved between accounts must not
    /// open even when the vault keys happen to match.
    #[test]
    fn wrong_account_errors() {
        let f = Scratch::new("wrongaccount");
        let k = keys(3);
        save_to(f.path(), &k, "srv-a", "a@example.com", TOKEN).expect("save");
        assert!(load_from(f.path(), &k, "srv-b", "a@example.com").is_err());
        assert!(load_from(f.path(), &k, "srv-a", "b@example.com").is_err());
    }

    #[test]
    fn clear_removes_the_file_and_is_idempotent() {
        let f = Scratch::new("clear");
        let k = keys(3);
        save_to(f.path(), &k, "s", "e", TOKEN).expect("save");
        remove_if_present(f.path()).expect("clear");
        assert!(load_from(f.path(), &k, "s", "e").expect("load").is_none());
        remove_if_present(f.path()).expect("clearing a missing file is success");
    }

    /// A truncated or corrupt file must surface as an error, never a panic —
    /// this runs inside the secrets-holding daemon.
    #[test]
    fn corrupt_file_errors_without_panicking() {
        let f = Scratch::new("corrupt");
        std::fs::create_dir_all(f.path().parent().unwrap()).expect("dir");
        std::fs::write(f.path(), b"\x01 not an envelope").expect("write");
        assert!(load_from(f.path(), &keys(3), "s", "e").is_err());
    }
}
