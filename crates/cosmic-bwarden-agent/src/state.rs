use cosmic_bwarden_core::locked;
use std::collections::HashMap;
use tokio::sync::mpsc;

pub struct State {
    pub keys: Option<locked::Keys>,
    pub org_keys: Option<HashMap<String, locked::Keys>>,
    pub master_password_hash: Option<locked::PasswordHash>,
    pub db: Option<cosmic_bwarden_core::db::Db>,
    pub pinned_ids: std::collections::HashSet<String>,
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
            pinned_ids: std::collections::HashSet::new(),
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

    pub fn lock(&mut self) {
        self.keys = None;
        self.org_keys = None;
        self.master_password_hash = None;
        self.pinned_ids.clear();
        self.name_cache.clear();
        self.username_cache.clear();
        self.broadcast(cosmic_bwarden_core::protocol::Event::Locked);
    }
}
