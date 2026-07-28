mod debug_impls;
pub mod entry_save;
#[cfg(test)]
mod tests;

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

/// TPM dictionary-attack (lockout) status. All counters are TPM-global (shared by
/// every DA-protected object), not specific to our PIN blob, and self-heal over
/// time (`recovery_interval_secs` per decrement). A successful PIN unlock resets
/// the counter to zero. Fields are `Option` because a TPM may not report a given
/// property.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct TpmDaStatus {
    /// True when the TPM is reachable and the values below are meaningful.
    pub available: bool,
    /// Max authorization failures before lockout (`TPM_PT_MAX_AUTH_FAIL`).
    pub max_tries: Option<u32>,
    /// Current failure count (`TPM_PT_LOCKOUT_COUNTER`).
    pub lockout_counter: Option<u32>,
    /// `max_tries - lockout_counter`, saturating at 0.
    pub remaining: Option<u32>,
    /// True when the TPM is currently in DA lockout (`TPMA_PERMANENT.inLockout`).
    pub in_lockout: bool,
    /// Seconds after which one failure is forgiven (`TPM_PT_LOCKOUT_INTERVAL`).
    pub recovery_interval_secs: Option<u32>,
}

/// Password-generator charset/length preferences. Shared, device-global (not
/// per-account) "last-used settings" — every surface (UI pane, applet quick-gen,
/// CLI, browser extension) reads/writes the same persisted value via the agent.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratorSettings {
    pub uppercase: bool,
    pub lowercase: bool,
    pub numbers: bool,
    pub special: bool,
    /// 8..=32, enforced by the agent regardless of what a client sends.
    pub length: u8,
}

impl Default for GeneratorSettings {
    fn default() -> Self {
        Self {
            uppercase: true,
            lowercase: true,
            numbers: true,
            special: true,
            length: 14,
        }
    }
}

/// One entry in the device-local, 7-day password-generation history. Not tied
/// to any vault entry or account — every generated password lands here
/// regardless of which surface (UI/applet/CLI/browser) requested it.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct GeneratorHistoryEntry {
    pub password: String,
    /// Unix epoch seconds.
    pub created_at: u64,
}

// IPC request message: transient (one per request, immediately consumed), so
// the size spread between small variants (Lock) and payload-carrying ones
// (AddEntry) doesn't justify boxing and the match-ergonomics churn it brings.
#[allow(clippy::large_enum_variant)]
#[derive(serde::Serialize, serde::Deserialize)]
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
        /// Current tab's full host (lowercased, no scheme). When set and
        /// `query` is None, entries are matched via `domain::hosts_match`
        /// against their cached URI hosts (exact / boundary-subdomain /
        /// eTLD+1) instead of the substring name search. A set `query` wins
        /// over `domain`. (serde default: JSON clients may omit it.)
        #[serde(default)]
        domain: Option<String>,
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
        /// Master password for reprompt-gated entries. Absent for entries that
        /// don't require reprompt. (serde treats a missing Option field as None,
        /// so JSON clients may omit it.)
        #[serde(default)]
        password: Option<String>,
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
        // `default` keeps older JSON clients (extension popup) that omit these
        // keys parsing in the browser host.
        #[serde(default)]
        totp: Option<crate::db::Secret>,
        #[serde(default)]
        uris: Vec<crate::db::Uri>,
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
    /// Returns the TPM dictionary-attack lockout status (attempts remaining, etc).
    GetTpmDaStatus,
    /// Update the autolock timer duration live, without restarting the agent.
    /// seconds=0 disables autolock.
    UpdateLockTimeout {
        seconds: u64,
    },
    /// Browser save-prompt support: does a Login entry exist for this
    /// domain+username, and does its stored password differ from the
    /// just-submitted one? The comparison happens inside the agent; the stored
    /// password is never returned. `password` is the freshly submitted value
    /// the client already holds.
    CheckLoginMatch {
        domain: String,
        username: String,
        password: String,
    },
    /// Set a new password on an existing Login entry (browser "Update" flow).
    /// The agent decrypts the stored entry itself, so no other secret (TOTP,
    /// notes) ever transits to the client, and redaction/merge pitfalls of
    /// echoing a `GetEntryMeta` result through `UpdateEntry` are avoided.
    UpdateLoginPassword {
        id: String,
        password: String,
    },

    /// Generate a password. `Some(settings)` persists the given settings as the
    /// new device-wide "last used" and generates with them (the UI pane's
    /// Generate button, and any CLI/browser caller fully specifying options).
    /// `None` reuses whatever is currently persisted (falling back to
    /// `GeneratorSettings::default()` on first use) — the applet quick-gen
    /// button, browser context menu, and inline field icon all send this.
    /// Every call, regardless of `Some`/`None`, appends the result to the
    /// 7-day local history. Does not require the vault to be unlocked, or
    /// even an account to be configured.
    GeneratePassword {
        settings: Option<GeneratorSettings>,
    },
    /// Fetch the currently persisted "last used" generator settings (for the
    /// UI pane to populate on open, and for Reset-button semantics).
    GetGeneratorSettings,
    /// Fetch the local password-generation history, pruned to the last 7 days,
    /// newest first.
    GetPasswordHistory,
    /// Delete one entry from the local password-generation history, identified
    /// by its `created_at` timestamp (history entries have no separate id).
    /// If more than one entry shares the same second, all matching entries
    /// are removed — an accepted, low-stakes edge case for a 7-day cache.
    DeleteGeneratedPassword {
        created_at: u64,
    },
}

impl Action {
    /// Variant name only — used by the redacting `Debug` impl and for logging.
    /// Never includes field values, which may hold passwords, PINs, or secrets.
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::Register { .. } => "Register",
            Self::Login { .. } => "Login",
            Self::Unlock { .. } => "Unlock",
            Self::Lock => "Lock",
            Self::Sync => "Sync",
            Self::GetConfig => "GetConfig",
            Self::GetEntries { .. } => "GetEntries",
            Self::GetSidebarEntries { .. } => "GetSidebarEntries",
            Self::GetEntry { .. } => "GetEntry",
            Self::GetPassword { .. } => "GetPassword",
            Self::GetTotp { .. } => "GetTotp",
            Self::CopyToClipboard { .. } => "CopyToClipboard",
            Self::PinEntry { .. } => "PinEntry",
            Self::UnpinEntry { .. } => "UnpinEntry",
            Self::AddEntry { .. } => "AddEntry",
            Self::AddSecureNote { .. } => "AddSecureNote",
            Self::AddCard { .. } => "AddCard",
            Self::AddIdentity { .. } => "AddIdentity",
            Self::AddSshKey { .. } => "AddSshKey",
            Self::SetPendingEntry { .. } => "SetPendingEntry",
            Self::RequestUnlock => "RequestUnlock",
            Self::GetEntryMeta { .. } => "GetEntryMeta",
            Self::Quit => "Quit",
            Self::Version => "Version",
            Self::DeleteEntry { .. } => "DeleteEntry",
            Self::UpdateEntry { .. } => "UpdateEntry",
            Self::Logout => "Logout",
            Self::Subscribe => "Subscribe",
            Self::SetupTpmPin { .. } => "SetupTpmPin",
            Self::SetupTpmPinFromUnlocked { .. } => "SetupTpmPinFromUnlocked",
            Self::UnlockWithPin { .. } => "UnlockWithPin",
            Self::DisableTpmPin => "DisableTpmPin",
            Self::EnableTpmServerCredentials => "EnableTpmServerCredentials",
            Self::DisableTpmServerCredentials => "DisableTpmServerCredentials",
            Self::CheckTpm => "CheckTpm",
            Self::CheckTpmDiagnostics => "CheckTpmDiagnostics",
            Self::GetTpmDaStatus => "GetTpmDaStatus",
            Self::UpdateLockTimeout { .. } => "UpdateLockTimeout",
            Self::CheckLoginMatch { .. } => "CheckLoginMatch",
            Self::UpdateLoginPassword { .. } => "UpdateLoginPassword",
            Self::GeneratePassword { .. } => "GeneratePassword",
            Self::GetGeneratorSettings => "GetGeneratorSettings",
            Self::GetPasswordHistory => "GetPasswordHistory",
            Self::DeleteGeneratedPassword { .. } => "DeleteGeneratedPassword",
        }
    }
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
    OpenEntry {
        id: String,
    },
}

/// Stable message carried by `Response::Error` when the TPM refuses to unseal
/// (wrong PIN, changed PCRs, or DA lockout). Clients compare against this exact
/// string to show their own short feedback; the full error chain is log-only.
pub const ERR_TPM_UNSEAL_FAILED: &str = "TPM unseal failed";

// IPC response message: transient like `Action` — see the comment there.
#[allow(clippy::large_enum_variant)]
#[derive(serde::Serialize, serde::Deserialize)]
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
    /// TPM dictionary-attack lockout status.
    TpmDaStatus {
        status: TpmDaStatus,
    },
    /// Answer to `CheckLoginMatch`. Carries no secrets: `password_matches`
    /// only confirms equality with a value the client already possesses.
    LoginMatch {
        /// Id of the matched Login entry, if any exists for domain+username.
        entry_id: Option<String>,
        /// Display name of the matched entry (for the save-bar text).
        name: Option<String>,
        /// True when the matched entry's stored password equals the submitted one.
        password_matches: bool,
    },
    /// A freshly generated password. Never logged verbatim (see `debug_impls`).
    GeneratedPassword {
        password: String,
    },
    /// Answer to `GetGeneratorSettings`.
    GeneratorSettings {
        settings: GeneratorSettings,
    },
    /// Answer to `GetPasswordHistory`: entries from the last 7 days, newest first.
    PasswordHistory {
        entries: Vec<GeneratorHistoryEntry>,
    },
}
