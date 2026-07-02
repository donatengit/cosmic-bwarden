use cosmic_bwarden_core::db::EntryData;
use cosmic_bwarden_core::locked;
use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;
use zeroize::Zeroize as _;

pub struct State {
    pub keys: Option<locked::Keys>,
    pub org_keys: Option<HashMap<String, locked::Keys>>,
    pub master_password_hash: Option<locked::PasswordHash>,
    pub db: Option<cosmic_bwarden_core::db::Db>,
    pub pinned_ids: HashSet<String>,
    /// Pre-built list of decrypted sidebar entries. Rebuilt on unlock and after
    /// every mutation (via sync). GetSidebarEntries just filters this in-memory.
    pub sidebar_cache: Vec<SidebarEntry>,
    pub pending_entry_id: Option<String>,
    pub subscribers: Vec<mpsc::UnboundedSender<cosmic_bwarden_core::protocol::Event>>,
    pub shutdown_tx: Option<mpsc::UnboundedSender<()>>,
    pub unlock_requested_notified: bool,
    /// Set when any server mutation (add/update/delete/sync) fails due to a
    /// network or backend error. Cleared by a successful sync.
    pub sync_failed: bool,
    pub last_sync_error: Option<String>,
    /// True when a TPM sealed blob exists for the current account.
    /// Set on startup and updated by the tpm_pin handler.
    pub tpm_configured: bool,
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
            tpm_configured: false,
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
                let effective_keys = entry.org_id.as_ref()
                    .and_then(|id| org_keys.get(id))
                    .unwrap_or(keys);

                let name = match cosmic_bwarden_core::vault::decrypt(
                    &entry.name, effective_keys, entry.key.as_deref(),
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!("sidebar cache: failed to decrypt name for entry {}: {}", entry.id, e);
                        entry.name.clone()
                    }
                };

                let (username, public_key, entry_type) = match &entry.data {
                    EntryData::Login { username, .. } => {
                        let u = username.as_ref().and_then(|u| {
                            match cosmic_bwarden_core::vault::decrypt(u, effective_keys, entry.key.as_deref()) {
                                Ok(v) => Some(v),
                                Err(e) => {
                                    log::warn!("sidebar cache: failed to decrypt username for entry {}: {}", entry.id, e);
                                    None
                                }
                            }
                        });
                        (u, None, EntryType::Login)
                    }
                    EntryData::SshKey { public_key, .. } => {
                        // Try the native sshKey.publicKey field first; fall back to a
                        // full decrypt (which handles the custom-field fallback path).
                        let pk = public_key
                            .as_ref()
                            .and_then(|pk| {
                                match cosmic_bwarden_core::vault::decrypt(pk, effective_keys, entry.key.as_deref()) {
                                    Ok(v) => Some(v),
                                    Err(_) => None,
                                }
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
                        (None, pk, EntryType::SshKey)
                    }
                    EntryData::Card { .. } => (None, None, EntryType::Card),
                    EntryData::Identity { .. } => (None, None, EntryType::Identity),
                    EntryData::SecureNote => (None, None, EntryType::SecureNote),
                };

                cache.push(SidebarEntry {
                    id: entry.id.clone(),
                    name,
                    username,
                    public_key,
                    entry_type,
                    is_pinned: entry.favorite,
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
        self.sync_failed = false;
        self.last_sync_error = None;
        // Zeroize and drop session tokens so they can't be used while locked.
        // The rest of `db` (encrypted entries, protected keys) stays in memory
        // so the vault can be unlocked offline without a re-sync.
        if let Some(db) = &mut self.db {
            if let Some(mut t) = db.access_token.take() { t.zeroize(); }
            if let Some(mut t) = db.refresh_token.take() { t.zeroize(); }
        }
        self.broadcast(cosmic_bwarden_core::protocol::Event::Locked);
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
