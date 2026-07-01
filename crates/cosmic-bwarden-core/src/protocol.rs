#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    Login,
    Card,
    Identity,
    SecureNote,
    SshKey,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SidebarEntry {
    pub id: String,
    pub name: String,
    pub username: Option<String>,
    pub public_key: Option<String>,
    pub entry_type: EntryType,
    pub is_pinned: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
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
        only_pinned: bool,
    },
    GetSidebarEntries {
        query: Option<String>,
        entry_type: Option<EntryType>,
        only_pinned: bool,
    },
    GetEntry {
        id: String,
        password: Option<String>,
    },
    GetPassword {
        id: String,
        password: Option<String>,
    },
    GetTotp {
        id: String,
    },
    CopyToClipboard {
        id: String,
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

    SetPendingEntry {
        id: String,
    },
    /// Ask the agent to broadcast Event::UnlockRequested so the applet opens its
    /// unlock screen. Sent by the browser extension when it detects the vault is locked.
    RequestUnlock,
    /// Like GetEntry but returns the entry with all secrets (password, TOTP,
    /// card numbers, private keys, notes) redacted. Use for detail/view UI so
    /// secrets are never eagerly fetched; request them on explicit user action.
    GetEntryMeta {
        id: String,
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

    // TPM PIN unlock actions (agent feature-gated; safe to send on non-TPM builds)
    SetupTpmPin {
        master_password: String,
        pin: String,
    },
    /// Like SetupTpmPin but uses the vault keys already in memory (vault must be unlocked).
    /// Does not require the master password to be re-entered.
    SetupTpmPinFromUnlocked {
        pin: String,
    },
    UnlockWithPin {
        pin: String,
    },
    /// Disable PIN unlock. The vault must be currently unlocked (checked in the agent).
    /// No master password needed — being authenticated in the vault is sufficient.
    DisableTpmPin,
    /// Seal the in-memory master_password_hash into a separate TPM blob (no PIN required
    /// for this blob — it is TPM-bound only). Enables silent server re-auth after PIN unlock.
    /// Fails if the vault was not unlocked with master password (hash not in memory).
    EnableTpmServerCredentials,
    /// Remove the TPM-sealed server-credentials blob, disabling silent re-auth.
    DisableTpmServerCredentials,
    CheckTpm,
    /// Returns system-level diagnostic checks explaining why TPM may be unavailable.
    CheckTpmDiagnostics,
    /// Update the autolock timer duration live, without restarting the agent.
    /// seconds=0 disables autolock.
    UpdateLockTimeout {
        seconds: u64,
    },
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum Event {
    Locked,
    Unlocked,
    VaultChanged,
    /// Something needed the vault unlocked but it is locked (e.g. an
    /// ssh-agent request). Broadcast at most once per lock period so
    /// subscribers can prompt the user to unlock without being spammed by
    /// repeated requests.
    UnlockRequested,
    /// Like UnlockRequested, but TPM PIN unlock is configured — the UI
    /// should show a PIN field instead of a full master-password prompt.
    PinRequested,
    /// Requests the vault window to open and navigate to a specific entry.
    OpenEntry { id: String },
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum Response {
    Ack,
    Error {
        message: String,
    },
    TwoFactorRequired {
        token: String,
        providers: Vec<u32>,
    },
    NewDeviceVerificationRequired,
    Config {
        config: crate::config::CosmicBWardenConfig,
        needs_login: bool,
        has_account: bool,
        is_locked: bool,
        /// True when the most recent server operation failed due to a network or
        /// backend error. Cleared by a successful sync. Visible in the UI as a
        /// red "Not synced" button so the user knows data may be out of date.
        sync_failed: bool,
    },
    Entries {
        entries: Vec<crate::db::Entry>,
    },
    SidebarEntries {
        entries: Vec<SidebarEntry>,
    },
    Entry {
        entry: crate::db::Entry,
    },
    Password {
        password: String,
    },
    Totp {
        code: String,
    },
    Version {
        version: String,
        protocol_version: String,
    },
    Event {
        event: Event,
    },
    /// Whether the TPM is reachable and a sealed blob is configured for this account.
    TpmStatus {
        available: bool,
        configured: bool,
        /// True when the master_password_hash is also sealed (enables silent server re-auth).
        server_credentials: bool,
    },
    /// System-level diagnostic checks: (label, passed, hint) triples.
    TpmDiagnostics {
        checks: Vec<(String, bool, String)>,
    },
}
