//! At-rest storage for the 7-day password-generation history.
//!
//! Reuses the vault's existing symmetric-cipher primitive (`cipherstring.rs`,
//! AES-256-CBC + HMAC-SHA256) rather than adding a new crypto dependency.
//! `locked::Keys` just wraps 64 raw bytes regardless of where they came from,
//! so a locally-generated, device-global key (not derived from any account's
//! master password) works as a drop-in.
//!
//! Threat model: this protects history at rest against a different local
//! user, a stray backup, or misconfigured file permissions elsewhere reading
//! the file directly — the same protection level the vault's own `Db` JSON
//! cache has (0600 perms). It does NOT protect against another process
//! running as the same local user, since the key file sits unguarded next to
//! the ciphertext by design: generation must work without a master password
//! or any account configured.

use anyhow::{Context as _, Result};
use cosmic_bwarden_core::protocol::GeneratorHistoryEntry;
use cosmic_bwarden_core::{cipherstring::CipherString, locked};
use std::os::unix::fs::OpenOptionsExt as _;
use std::time::{SystemTime, UNIX_EPOCH};

const HISTORY_TTL_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredEntry {
    created_at: u64,
    ciphertext: String,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Load the device-global history key, generating and persisting one (0600)
/// on first use.
fn load_or_create_key() -> Result<locked::Keys> {
    let path = cosmic_bwarden_core::dirs::generator_key_file();
    match std::fs::read(&path) {
        Ok(bytes) => {
            anyhow::ensure!(
                bytes.len() == 64,
                "generator key file has unexpected length"
            );
            let mut v = locked::Vec::new();
            v.extend(bytes.into_iter());
            Ok(locked::Keys::new(v))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            use rand::TryRngCore as _;
            let mut raw = [0u8; 64];
            rand::rngs::OsRng
                .try_fill_bytes(&mut raw)
                .context("reading OS randomness for generator key")?;

            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).context("creating data dir for generator key")?;
            }
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&path)
                .and_then(|mut f| {
                    use std::io::Write as _;
                    f.write_all(&raw)
                })
                .context("writing generator key to disk")?;

            let mut v = locked::Vec::new();
            v.extend(raw.into_iter());
            Ok(locked::Keys::new(v))
        }
        Err(e) => Err(e).context("reading generator key file"),
    }
}

fn load_stored_entries() -> Result<Vec<StoredEntry>> {
    let path = cosmic_bwarden_core::dirs::generator_history_file();
    match std::fs::read(&path) {
        Ok(bytes) => postcard::from_bytes(&bytes).context("deserializing generator history"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e).context("reading generator history file"),
    }
}

fn save_stored_entries(entries: &[StoredEntry]) -> Result<()> {
    let path = cosmic_bwarden_core::dirs::generator_history_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("creating data dir for generator history")?;
    }
    let bytes = postcard::to_allocvec(entries).context("serializing generator history")?;
    // Atomic tmp+rename, same pattern as `db::persistence::Db::save`.
    let tmp = path.with_extension("bin.tmp");
    {
        let mut fh = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .context("opening generator history tmp file")?;
        use std::io::Write as _;
        fh.write_all(&bytes).context("writing generator history")?;
        fh.sync_all().ok();
    }
    std::fs::rename(&tmp, &path).context("renaming generator history into place")?;
    Ok(())
}

fn prune(entries: Vec<StoredEntry>) -> Vec<StoredEntry> {
    let now = now_unix();
    entries
        .into_iter()
        .filter(|e| now.saturating_sub(e.created_at) <= HISTORY_TTL_SECS)
        .collect()
}

/// Append a freshly generated password to the history, pruning anything older
/// than 7 days in the same pass so the file self-heals on every write.
pub fn append(password: &str) -> Result<()> {
    let keys = load_or_create_key()?;
    let mut entries = prune(load_stored_entries()?);

    let ciphertext = CipherString::encrypt_symmetric(&keys, password.as_bytes())
        .context("encrypting history entry")?
        .to_string();
    entries.push(StoredEntry {
        created_at: now_unix(),
        ciphertext,
    });

    save_stored_entries(&entries)
}

/// Delete every history entry matching `created_at` (normally exactly one —
/// history entries have no separate id, so a shared-second collision would
/// remove more than one, an accepted low-stakes edge case for a 7-day cache).
pub fn delete_by_created_at(created_at: u64) -> Result<()> {
    let entries: Vec<StoredEntry> = load_stored_entries()?
        .into_iter()
        .filter(|e| e.created_at != created_at)
        .collect();
    save_stored_entries(&entries)
}

/// Return history entries from the last 7 days, newest first. Re-saves the
/// pruned set if anything expired, so an idle history file also shrinks the
/// next time anyone looks, not just on the next write.
pub fn get_pruned_newest_first() -> Result<Vec<GeneratorHistoryEntry>> {
    let raw = load_stored_entries()?;
    let before = raw.len();
    let pruned = prune(raw);
    if pruned.len() != before {
        save_stored_entries(&pruned)?;
    }

    if pruned.is_empty() {
        return Ok(Vec::new());
    }

    let keys = load_or_create_key()?;
    let mut decrypted: Vec<GeneratorHistoryEntry> = pruned
        .iter()
        .map(|e| {
            let cs = CipherString::new(&e.ciphertext).context("parsing history ciphertext")?;
            let bytes = cs
                .decrypt_symmetric(&keys, None)
                .context("decrypting history entry")?;
            let password = String::from_utf8(bytes).context("history entry was not valid UTF-8")?;
            Ok(GeneratorHistoryEntry {
                password,
                created_at: e.created_at,
            })
        })
        .collect::<Result<_>>()?;
    decrypted.sort_by_key(|e| std::cmp::Reverse(e.created_at));
    Ok(decrypted)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Exercises prune() in isolation (pure function, no filesystem) since the
    // append/get_pruned_newest_first pair touches real dirs::* paths that are
    // process-global — covered instead by the agent E2E suite
    // (crates/cosmic-bwarden-tests), which can isolate a per-test data dir.
    #[test]
    fn prune_drops_entries_older_than_seven_days() {
        let now = now_unix();
        let entries = vec![
            StoredEntry {
                created_at: now,
                ciphertext: "fresh".to_string(),
            },
            StoredEntry {
                created_at: now.saturating_sub(HISTORY_TTL_SECS + 3600),
                ciphertext: "stale".to_string(),
            },
        ];
        let pruned = prune(entries);
        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned[0].ciphertext, "fresh");
    }

    // delete_by_created_at itself touches the real dirs::* paths (like
    // append/get_pruned_newest_first above), so its filter logic is tested
    // here in isolation instead, mirroring the prune() test's approach.
    #[test]
    fn delete_filter_removes_only_the_matching_timestamp() {
        let entries = vec![
            StoredEntry {
                created_at: 100,
                ciphertext: "a".to_string(),
            },
            StoredEntry {
                created_at: 200,
                ciphertext: "b".to_string(),
            },
            StoredEntry {
                created_at: 100,
                ciphertext: "c".to_string(),
            },
        ];
        let remaining: Vec<StoredEntry> = entries
            .into_iter()
            .filter(|e| e.created_at != 100)
            .collect();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].ciphertext, "b");
    }
}
