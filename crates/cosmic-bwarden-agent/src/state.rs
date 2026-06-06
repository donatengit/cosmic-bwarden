use cosmic_bwarden_core::locked;
use std::collections::HashMap;
use tokio::sync::mpsc;

pub struct State {
    pub keys: Option<locked::Keys>,
    pub org_keys: Option<HashMap<String, locked::Keys>>,
    pub master_password_hash: Option<locked::PasswordHash>,
    pub db: Option<cosmic_bwarden_core::db::Db>,
    pub clipboard: Option<arboard::Clipboard>,
    pub clipboard_gen: u32,
    pub name_cache: HashMap<String, String>, // id -> decrypted name
    pub username_cache: HashMap<String, String>, // id -> decrypted username
    pub subscribers: Vec<mpsc::UnboundedSender<cosmic_bwarden_core::protocol::Event>>,
}

impl State {
    pub fn new() -> Self {
        Self {
            keys: None,
            org_keys: None,
            master_password_hash: None,
            db: None,
            clipboard: arboard::Clipboard::new().ok(),
            clipboard_gen: 0,
            name_cache: HashMap::new(),
            username_cache: HashMap::new(),
            subscribers: Vec::new(),
        }
    }

    pub fn broadcast(&mut self, event: cosmic_bwarden_core::protocol::Event) {
        self.subscribers.retain(|tx| {
            tx.send(event.clone()).is_ok()
        });
    }

    pub fn record_copy(&mut self, id: &str) {
        if let Some(db) = &mut self.db {
            if db.pinned_ids.contains(id) {
                let count = db.usage_counts.entry(id.to_string()).or_insert(0);
                *count += 1;
                // Note: main.rs should handle saving the DB
            }
        }
    }

    pub fn pin_entry(&mut self, id: &str) {
        if let Some(db) = &mut self.db {
            db.pinned_ids.insert(id.to_string());
        }
    }

    pub fn unpin_entry(&mut self, id: &str) {
        if let Some(db) = &mut self.db {
            db.pinned_ids.remove(id);
            db.usage_counts.remove(id);
        }
    }

    pub fn top_pinned(&self, limit: usize) -> Vec<String> {
        if let Some(db) = &self.db {
            let mut pinned: Vec<_> = db.pinned_ids.iter().collect();
            pinned.sort_by(|a, b| {
                let count_a = db.usage_counts.get(*a).unwrap_or(&0);
                let count_b = db.usage_counts.get(*b).unwrap_or(&0);
                count_b.cmp(count_a).then((*a).cmp(*b))
            });
            pinned.into_iter().take(limit).cloned().collect()
        } else {
            Vec::new()
        }
    }

    pub fn lock(&mut self) {
        self.keys = None;
        self.org_keys = None;
        self.master_password_hash = None;
        self.name_cache.clear();
        self.username_cache.clear();
        self.broadcast(cosmic_bwarden_core::protocol::Event::Locked);
    }
}
