#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    Login,
    Card,
    Identity,
    SecureNote,
    SshKey,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct SidebarEntry {
    pub id: String,
    pub name: String,
    pub entry_type: EntryType,
    pub is_pinned: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Action {
    Register {
        email: String,
        password: String,
        server_url: String,
    },
    Login {
        email: String,
        password: String,
        server_url: Option<String>,
        remember_me: bool,
        two_factor_token: Option<String>,
        two_factor_provider: Option<u32>,
        two_factor_code: Option<String>,
        device_verification_code: Option<String>,
    },
    Unlock {
        password: String,
    },
    Lock,
    Sync,
    GetConfig,
    GetEntries {
        query: Option<String>,
        entry_type: Option<EntryType>,
    },
    GetSidebarEntries {
        query: Option<String>,
        entry_type: Option<EntryType>,
    },
    GetEntry {
        id: String,
        password: Option<String>,
    },
    GetPassword {
        id: String,
        password: Option<String>,
    },
    CopyToClipboard {
        id: String,
    },
    RecordCopy {
        id: String,
    },
    GetTopFrequent {
        limit: usize,
        days: Option<u32>,
    },
    PinEntry {
        id: String,
    },
    UnpinEntry {
        id: String,
    },
    AddEntry {
        name: String,
        entry_type: EntryType,
        username: Option<String>,
        password: Option<crate::db::Secret>,
        notes: Option<crate::db::Secret>,
        fields: Vec<crate::db::Field>,
    },
    AddSecureNote {
        name: String,
        notes: crate::db::Secret,
        fields: Vec<crate::db::Field>,
    },
    AddCard {
        name: String,
        cardholder_name: Option<String>,
        number: Option<crate::db::Secret>,
        brand: Option<String>,
        exp_month: Option<String>,
        exp_year: Option<String>,
        code: Option<crate::db::Secret>,
        notes: Option<crate::db::Secret>,
        fields: Vec<crate::db::Field>,
    },
    AddIdentity {
        name: String,
        first_name: Option<String>,
        last_name: Option<String>,
        address1: Option<String>,
        city: Option<String>,
        state: Option<String>,
        postal_code: Option<String>,
        country: Option<String>,
        email: Option<String>,
        phone: Option<String>,
        notes: Option<crate::db::Secret>,
        fields: Vec<crate::db::Field>,
    },
    AddSshKey {
        name: String,
        private_key: crate::db::Secret,
        public_key: Option<String>,
        notes: Option<crate::db::Secret>,
        fields: Vec<crate::db::Field>,
    },

    Quit,
    Version,
    DeleteEntry {
        id: String,
    },
    UpdateEntry {
        entry: crate::db::Entry,
    },
    Logout,
    Subscribe,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Event {
    Locked,
    Unlocked,
    VaultChanged,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Response {
    Ack,
    Error { message: String },
    Config { config: crate::config::CosmicBWardenConfig, needs_login: bool, is_locked: bool },
    Entries { entries: Vec<crate::db::Entry> },
    SidebarEntries { entries: Vec<SidebarEntry> },
    Entry { entry: crate::db::Entry },
    Password { password: String },
    Version { version: String },
    Event { event: Event },
}
