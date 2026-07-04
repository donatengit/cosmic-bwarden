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
        }
    }
}

// Manual `Debug` that never prints field values. Many `Action` variants carry
// master passwords, PINs, TOTP secrets, and card data; the derived `Debug` would
// leak them into logs (`handler.rs`, `main.rs`, `browser_host.rs`) at info/debug.
impl std::fmt::Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Non-secret scalar fields are safe and useful for debugging.
            Self::GetEntry { id, .. } => write!(f, "GetEntry {{ id: {id:?} }}"),
            Self::GetEntryMeta { id } => write!(f, "GetEntryMeta {{ id: {id:?} }}"),
            Self::GetPassword { id, .. } => write!(f, "GetPassword {{ id: {id:?} }}"),
            Self::GetTotp { id, .. } => write!(f, "GetTotp {{ id: {id:?} }}"),
            Self::DeleteEntry { id } => write!(f, "DeleteEntry {{ id: {id:?} }}"),
            Self::PinEntry { id } => write!(f, "PinEntry {{ id: {id:?} }}"),
            Self::UnpinEntry { id } => write!(f, "UnpinEntry {{ id: {id:?} }}"),
            Self::SetPendingEntry { id } => write!(f, "SetPendingEntry {{ id: {id:?} }}"),
            Self::UpdateLockTimeout { seconds } => {
                write!(f, "UpdateLockTimeout {{ seconds: {seconds} }}")
            }
            // Everything else: variant name only.
            other => f.write_str(other.variant_name()),
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
}

#[cfg(test)]
mod debug_redaction_tests {
    use super::*;

    #[test]
    fn action_debug_never_prints_secrets() {
        let a = Action::Unlock {
            password: "hunter2-secret".to_string(),
        };
        assert_eq!(format!("{a:?}"), "Unlock");

        let a = Action::UnlockWithPin {
            pin: "1234-secret".to_string(),
        };
        assert!(!format!("{a:?}").contains("1234-secret"));

        let a = Action::SetupTpmPin {
            master_password: "master-secret".to_string(),
            pin: "pin-secret".to_string(),
        };
        let s = format!("{a:?}");
        assert!(!s.contains("master-secret") && !s.contains("pin-secret"));

        // Non-secret scalar (entry id) is allowed for debuggability.
        let a = Action::GetPassword {
            id: "entry-123".to_string(),
            password: Some("x".into()),
        };
        let s = format!("{a:?}");
        assert!(s.contains("entry-123") && !s.contains("\"x\""));
    }

    #[test]
    fn response_debug_never_prints_secrets() {
        let r = Response::Password {
            password: "leaked-pw".to_string(),
        };
        assert!(!format!("{r:?}").contains("leaked-pw"));

        let r = Response::Totp {
            code: "123456".to_string(),
        };
        assert!(!format!("{r:?}").contains("123456"));

        // Error messages remain visible.
        let r = Response::Error {
            message: "boom".to_string(),
        };
        assert!(format!("{r:?}").contains("boom"));
    }

    // Deterministic mini-fuzz: the agent postcard-decodes `Action` from any
    // same-UID client, so the decoder is attacker-reachable (threat A2).
    // Arbitrary bytes must produce Ok/Err, never a panic or huge allocation.
    #[test]
    fn action_decode_from_arbitrary_bytes_never_panics() {
        use rand::{Rng as _, SeedableRng as _};
        let mut rng = rand::rngs::StdRng::seed_from_u64(0xDEC0DE);

        for _ in 0..10_000 {
            let len = rng.random_range(0..128);
            let bytes: Vec<u8> = (0..len).map(|_| rng.random()).collect();
            let _ = postcard::from_bytes::<Action>(&bytes);
        }

        // Truncations of a valid message — the classic framing edge case.
        let valid = postcard::to_allocvec(&Action::Lock).unwrap();
        for cut in 0..valid.len() {
            let _ = postcard::from_bytes::<Action>(&valid[..cut]);
        }
    }
}

// Manual `Debug` that never prints secret payloads. `Password`, `Totp`, `Entry`,
// and `Entries` carry decrypted secrets; the derived `Debug` would leak them into
// logs. Non-secret variants (errors, versions, flags) print their useful fields.
impl std::fmt::Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ack => f.write_str("Ack"),
            Self::Error { message } => write!(f, "Error {{ message: {message:?} }}"),
            Self::TwoFactorRequired { providers, .. } => {
                write!(
                    f,
                    "TwoFactorRequired {{ providers: {providers:?}, token: <redacted> }}"
                )
            }
            Self::NewDeviceVerificationRequired => f.write_str("NewDeviceVerificationRequired"),
            Self::Config {
                needs_login,
                has_account,
                is_locked,
                sync_failed,
                ..
            } => write!(
                f,
                "Config {{ needs_login: {needs_login}, has_account: {has_account}, \
                 is_locked: {is_locked}, sync_failed: {sync_failed} }}"
            ),
            Self::Entries { entries } => {
                write!(f, "Entries {{ count: {}, <redacted> }}", entries.len())
            }
            Self::SidebarEntries { entries } => {
                write!(f, "SidebarEntries {{ count: {} }}", entries.len())
            }
            Self::Entry { .. } => f.write_str("Entry { <redacted> }"),
            Self::Password { .. } => f.write_str("Password { <redacted> }"),
            Self::Totp { .. } => f.write_str("Totp { <redacted> }"),
            Self::Version {
                version,
                protocol_version,
            } => write!(
                f,
                "Version {{ version: {version:?}, protocol_version: {protocol_version:?} }}"
            ),
            Self::Event { event } => write!(f, "Event {{ event: {event:?} }}"),
            Self::TpmStatus {
                available,
                configured,
                server_credentials,
            } => write!(
                f,
                "TpmStatus {{ available: {available}, configured: {configured}, \
                 server_credentials: {server_credentials} }}"
            ),
            Self::TpmDiagnostics { checks } => {
                write!(f, "TpmDiagnostics {{ checks: {} }}", checks.len())
            }
            Self::TpmDaStatus { status } => write!(f, "TpmDaStatus {{ {status:?} }}"),
        }
    }
}
