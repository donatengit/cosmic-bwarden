use cosmic_bwarden_core::db::EntryData;
use cosmic_bwarden_core::locked;
use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex as AsyncMutex;

/// Fresh, random-enough per-process session id for ordering `Config`
/// snapshots. Time since epoch (nanos) folded with the process id is
/// sufficient: it only needs to differ across agent restarts.
fn session_id() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let pid = std::process::id() as u64;
    // SplitMix64-style finalizer so consecutive restarts don't produce
    // near-identical ids.
    let mut x = nanos ^ (pid << 32);
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

/// A sidebar entry plus the normalized hosts it is bound to, for domain
/// matching without per-query decryption. Hosts come from the login's URIs
/// (skipping match type `Never`), with a hostname-shaped name as fallback for
/// legacy entries created without URIs. Plaintext hosts sit in unlocked-agent
/// memory alongside the plaintext names/usernames already cached here — same
/// sensitivity class, documented in CONTEXT.md.
pub struct CachedSidebarEntry {
    pub entry: SidebarEntry,
    pub hosts: Vec<String>,
}

pub struct State {
    pub keys: Option<locked::Keys>,
    pub org_keys: Option<HashMap<String, locked::Keys>>,
    pub master_password_hash: Option<locked::PasswordHash>,
    pub db: Option<cosmic_bwarden_core::db::Db>,
    pub pinned_ids: HashSet<String>,
    /// Pre-built list of decrypted sidebar entries. Rebuilt on unlock and after
    /// every mutation (via sync). GetSidebarEntries just filters this in-memory.
    pub sidebar_cache: Vec<CachedSidebarEntry>,
    pub pending_entry_id: Option<String>,
    pub subscribers: Vec<mpsc::UnboundedSender<cosmic_bwarden_core::protocol::Event>>,
    pub shutdown_tx: Option<mpsc::UnboundedSender<()>>,
    pub unlock_requested_notified: bool,
    /// Set when any server mutation (add/update/delete/sync) fails due to a
    /// network or backend error. Cleared by a successful sync. Deliberately
    /// NOT cleared on lock: a vault that failed to sync stays failed to sync
    /// across a lock/unlock cycle (an unlock re-auths and then syncs, which
    /// clears it truthfully).
    pub sync_failed: bool,
    pub last_sync_error: Option<String>,
    /// Random per-agent-session id. Reported in `Response::Config` so clients
    /// can distinguish "older response in this session" from "agent restarted"
    /// (see `lock_epoch`).
    pub session_id: u64,
    /// Number of lock-state transitions this session (lock, unlock, login,
    /// logout). Reported in `Response::Config`; clients drop stale snapshots
    /// by comparing (session_id, lock_epoch).
    pub lock_epoch: u64,
    /// True when a TPM sealed blob exists for the current account.
    /// Set on startup and updated by the tpm_pin handler.
    pub tpm_configured: bool,
    /// Serializes `server::auth::with_refresh`'s refresh-token exchange.
    /// Held only around the check-and-refresh step (not whole requests), so
    /// concurrent 401s don't both spend the same single-use refresh token —
    /// Vaultwarden rotates it on use, so the loser of an unsynchronized race
    /// would persist a stale/already-consumed token. See `[F2-4]` in
    /// `docs/roadmap.md`.
    pub refresh_lock: Arc<AsyncMutex<()>>,
}

impl State {
    pub fn new() -> Self {
        Self {
            keys: None,
            org_keys: None,
            master_password_hash: None,
            db: None,
            pinned_ids: HashSet::new(),
            sidebar_cache: Vec::new(),
            pending_entry_id: None,
            subscribers: Vec::new(),
            shutdown_tx: None,
            unlock_requested_notified: false,
            sync_failed: false,
            last_sync_error: None,
            session_id: session_id(),
            lock_epoch: 0,
            tpm_configured: false,
            refresh_lock: Arc::new(AsyncMutex::new(())),
        }
    }

    /// Decrypt and cache the sidebar fields (name, username, public key) for
    /// every entry. Call after unlock and after any vault mutation.
    pub fn rebuild_sidebar_cache(&mut self) {
        let new_cache = if let (Some(db), Some(keys)) = (&self.db, &self.keys) {
            let empty_org_keys = HashMap::new();
            let org_keys = self.org_keys.as_ref().unwrap_or(&empty_org_keys);
            let mut cache = Vec::with_capacity(db.entries.len());
            for entry in &db.entries {
                let effective_keys = entry
                    .org_id
                    .as_ref()
                    .and_then(|id| org_keys.get(id))
                    .unwrap_or(keys);

                let name = match cosmic_bwarden_core::vault::decrypt(
                    &entry.name,
                    effective_keys,
                    entry.key.as_deref(),
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!(
                            "sidebar cache: failed to decrypt name for entry {}: {}",
                            entry.id,
                            e
                        );
                        entry.name.clone()
                    }
                };

                let (username, public_key, entry_type, hosts) = match &entry.data {
                    EntryData::Login { username, uris, .. } => {
                        let u = username.as_ref().and_then(|u| {
                            match cosmic_bwarden_core::vault::decrypt(u, effective_keys, entry.key.as_deref()) {
                                Ok(v) => Some(v),
                                Err(e) => {
                                    log::warn!("sidebar cache: failed to decrypt username for entry {}: {}", entry.id, e);
                                    None
                                }
                            }
                        });
                        // Hosts for domain matching. URIs with match type
                        // `Never` are excluded here so they can never surface
                        // an entry (docs/public_suffix_list.md).
                        let mut hosts: Vec<String> = uris
                            .iter()
                            .filter(|u| {
                                u.match_type != Some(cosmic_bwarden_core::api::UriMatchType::Never)
                            })
                            .filter_map(|u| {
                                match cosmic_bwarden_core::vault::decrypt(
                                    &u.uri,
                                    effective_keys,
                                    entry.key.as_deref(),
                                ) {
                                    Ok(plain) => cosmic_bwarden_core::domain::host_from_uri(&plain),
                                    Err(e) => {
                                        log::warn!(
                                            "sidebar cache: failed to decrypt uri for entry {}: {}",
                                            entry.id,
                                            e
                                        );
                                        None
                                    }
                                }
                            })
                            .collect();
                        if hosts.is_empty() {
                            // Legacy entries created without URIs: the save
                            // bar's convention is name = domain.
                            hosts.extend(cosmic_bwarden_core::domain::host_from_name(&name));
                        }
                        (u, None, EntryType::Login, hosts)
                    }
                    EntryData::SshKey { public_key, .. } => {
                        // Try the native sshKey.publicKey field first; fall back to a
                        // full decrypt (which handles the custom-field fallback path).
                        let pk = public_key
                            .as_ref()
                            .and_then(|pk| {
                                cosmic_bwarden_core::vault::decrypt(
                                    pk,
                                    effective_keys,
                                    entry.key.as_deref(),
                                )
                                .ok()
                            })
                            .or_else(|| {
                                let empty_org_keys = HashMap::new();
                                let org_keys = self.org_keys.as_ref().unwrap_or(&empty_org_keys);
                                let decrypted = entry.decrypt(effective_keys, org_keys);
                                if let EntryData::SshKey { public_key, .. } = decrypted.data {
                                    public_key
                                } else {
                                    None
                                }
                            });
                        (None, pk, EntryType::SshKey, Vec::new())
                    }
                    EntryData::Card { .. } => (None, None, EntryType::Card, Vec::new()),
                    EntryData::Identity { .. } => (None, None, EntryType::Identity, Vec::new()),
                    EntryData::SecureNote => (None, None, EntryType::SecureNote, Vec::new()),
                };

                cache.push(CachedSidebarEntry {
                    entry: SidebarEntry {
                        id: entry.id.clone(),
                        name,
                        username,
                        public_key,
                        entry_type,
                        is_pinned: entry.favorite,
                    },
                    hosts,
                });
            }
            cache
        } else {
            Vec::new()
        };
        self.sidebar_cache = new_cache;
    }

    pub fn broadcast(&mut self, event: cosmic_bwarden_core::protocol::Event) {
        self.subscribers.retain(|tx| tx.send(event.clone()).is_ok());
    }

    pub fn lock(&mut self) {
        self.keys = None;
        self.org_keys = None;
        self.master_password_hash = None;
        self.pinned_ids.clear();
        self.sidebar_cache.clear();
        self.pending_entry_id = None;
        self.unlock_requested_notified = false;
        // Deliberately do NOT clear sync_failed/last_sync_error here: a vault
        // that failed to sync stays out of sync across a lock/unlock cycle.
        // Unlock re-authenticates and then runs a sync, which clears the flag
        // truthfully (or sets it again if that sync also fails).
        self.bump_epoch();
        // Drop session tokens so they can't be used while locked — both
        // `locked::Token` representations zeroize on drop. The rest of `db`
        // (encrypted entries, protected keys) stays in memory so the vault
        // can be unlocked offline without a re-sync.
        if let Some(db) = &mut self.db {
            db.access_token = None;
            db.refresh_token = None;
        }
        self.broadcast(cosmic_bwarden_core::protocol::Event::Locked);
    }

    /// Record a lock-state transition (lock, unlock, login, logout) so
    /// `Response::Config` snapshots can be ordered by clients. Call this
    /// while holding the state lock, before broadcasting the matching event.
    pub fn bump_epoch(&mut self) {
        self.lock_epoch = self.lock_epoch.wrapping_add(1);
    }

    pub fn request_unlock(&mut self) {
        if !self.unlock_requested_notified {
            log::warn!("vault is locked but an operation requires it to be unlocked");
            let event = if self.tpm_configured {
                cosmic_bwarden_core::protocol::Event::PinRequested
            } else {
                cosmic_bwarden_core::protocol::Event::UnlockRequested
            };
            self.broadcast(event);
            self.unlock_requested_notified = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `lock()` bumps the lock epoch (so stale `Config` snapshots are
    /// detectable) and must NOT whitewash the out-of-sync state: a vault that
    /// failed to sync stays failed across a lock.
    #[test]
    fn lock_bumps_epoch_and_preserves_sync_failed() {
        let mut state = State::new();
        state.sync_failed = true;
        state.last_sync_error = Some("sync failed: network down".to_string());
        let epoch_before = state.lock_epoch;

        state.lock();

        assert_eq!(
            state.lock_epoch,
            epoch_before + 1,
            "lock must bump the epoch"
        );
        assert!(
            state.sync_failed,
            "lock must not clear the out-of-sync flag"
        );
        assert!(
            state.last_sync_error.is_some(),
            "lock must not clear the last sync error"
        );
    }

    /// A fresh state has a non-zero session id and epoch 0.
    #[test]
    fn fresh_state_session_and_epoch() {
        let state = State::new();
        assert_ne!(state.session_id, 0, "session id must identify the session");
        assert_eq!(state.lock_epoch, 0);
    }
}
