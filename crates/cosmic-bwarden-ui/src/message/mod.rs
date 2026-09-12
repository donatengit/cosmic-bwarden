use cosmic::iced::window;
use cosmic::widget;
use cosmic::widget::ToastId;
use cosmic_bwarden_core::config::CosmicBWardenConfig;
use cosmic_bwarden_core::db::{Entry, Secret};
use cosmic_bwarden_core::protocol::{
    EntryType, GeneratorHistoryEntry, GeneratorSettings, SidebarEntry,
};

#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Loading,
    Setup,
    Unlock,
    Vault,
    Settings,
    PasswordGenerator,
}

impl View {
    /// Whether the vault is considered unlocked in this view. Single source
    /// of truth shared by the applet menu (`header_row`, `quit_footer`) and
    /// the applet popup content routing — previously duplicated as two
    /// `matches!` blocks plus a popup `match` that disagreed on
    /// `View::PasswordGenerator`.
    pub fn is_unlocked(&self) -> bool {
        matches!(self, View::Vault | View::Settings | View::PasswordGenerator)
    }
}

#[derive(Debug, Clone)]
pub enum WindowState {
    Popup,
}

/// Which unlock credential the unlock views should ask for. Derived from
/// agent events (PinRequested/UnlockRequested) and the TPM status — see
/// `CosmicBWardenApp::unlock_mode`. The applet and main window share this so
/// both surfaces always offer the same form for the same state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnlockMode {
    #[default]
    Password,
    Pin,
}

/// Why the master-password reprompt dialog is showing. Submit must resume
/// this intent (`GetEntry` for edit, `GetPassword`/`GetTotp`/`GetEntry` for
/// reveal/copy) — never a bare detail `GetEntryMeta`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepromptIntent {
    Edit,
    Reveal { field: String },
    Copy { field: String },
}

/// Plaintext (or full entry) from an on-demand secret fetch. Debug-redacted.
/// `Entry` is boxed: the variant is ~600 bytes larger than the string
/// variants, which trips `clippy::large-enum-variant` (workspace denies
/// warnings) — boxing is free here because the payload is transient.
#[derive(Clone)]
pub enum OnDemandPayload {
    Password(Secret),
    Totp(Secret),
    Entry(Box<Entry>),
}

/// Agent config/state snapshot carried by `Message::ConfigReceived` — exactly
/// `Response::Config`'s payload: (config, needs_login, has_account, is_locked,
/// sync_failed, session_id, lock_epoch). Named to keep the message enum
/// readable (and `clippy::type_complexity` quiet).
pub type ConfigSnapshot = (CosmicBWardenConfig, bool, bool, bool, bool, u64, u64);

mod debug_impls;

// MVU message enum: one short-lived value per event. Boxing the wide variants
// (entry payloads) would touch every `match` arm for a transient allocation win.
// `Debug` is handwritten in `debug_impls` so clipboard / password / PIN / entry
// payloads never appear in `{:?}`.
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum Message {
    /// (config, needs_login, has_account, is_locked, sync_failed,
    ///  session_id, lock_epoch)
    ConfigReceived(Result<ConfigSnapshot, String>),

    // Window management
    WindowClosed(window::Id),
    RefreshStateInternal,
    AppletIconClicked(cosmic::iced::Vector, cosmic::iced::Rectangle),
    Exit,
    // Setup actions
    EmailChanged(String),
    PasswordChanged(String),
    ServerChanged(String),
    RememberChanged(bool),
    VerificationCodeChanged(String),
    LoginSubmitted,

    // Unlock actions
    UnlockPasswordChanged(String),
    UnlockSubmitted,
    /// PIN (re-)enable field on the master-password unlock view.
    UnlockPinChanged(String),
    UnlockPinRevealToggled,

    // Vault actions
    SearchChanged(String),
    /// Segmented-control tab pressed; resolved to a filter via the tab's
    /// position in `filter_model` (see `view::vault::sidebar::idx_to_filter`).
    FilterTabActivated(widget::segmented_button::Entity),
    /// Sidebar/content split dragged in the vault window.
    PaneResized(widget::pane_grid::ResizeEvent),
    SelectEntry(String),
    EntryReceived(Result<Entry, String>),
    AddEntryRequested,
    EditEntry,
    /// Full `GetEntry` result for the edit form (selection used `GetEntryMeta`).
    EditEntryLoaded(Result<Entry, String>),
    CancelEdit,
    SaveEdit,
    EditFieldChanged(String, String), // field name, value
    EditNameChanged(String),
    EntriesReceived(u32, Result<Vec<SidebarEntry>, String>),
    CopyToClipboard(String),
    /// Copy a secret that may not yet be in `selected_entry` (GetEntryMeta).
    CopySecretField(String, String),
    /// Result of GetPassword / GetTotp / GetEntry for a detail-pane secret.
    OnDemandSecretLoaded {
        field: String,
        copy: bool,
        result: Result<OnDemandPayload, String>,
    },
    /// Auto-clear timer fired; payload is the copy generation it was armed
    /// for — stale generations (a newer copy happened since) are ignored.
    ClipboardClearElapsed(u32),
    /// Clipboard contents read back for the pending auto-clear: only wipe if
    /// it still holds what we copied, never content the user copied elsewhere.
    ClipboardClearReadback(u32, Option<String>),
    NotesAction(widget::text_editor::Action),
    DeleteEntry(String),
    DeleteEntryResult(Result<(), String>),
    SaveEditResult(Result<(), String>),
    ConfirmDelete,
    CancelDelete,
    RepromptPasswordChanged(String),
    SubmitReprompt,
    CancelReprompt,
    NewEntryTypeChanged(EntryType),
    ToggleAdvanced,

    // Applet actions
    Surface(cosmic::surface::Action<Message>),
    OpenVaultRequested,
    Token(cosmic::applet::token::subscription::TokenUpdate),

    // Applet popup: inline unlock
    AppletUnlockPasswordChanged(String),
    AppletUnlockSubmitted,
    AppletUnlockResult(Result<(), String>),

    // Applet popup: search
    AppletSearchChanged(String),
    AppletToggleFavouritesFilter,
    AppletSearchResultsReceived(u32, Result<Vec<SidebarEntry>, String>),

    // Applet popup: copy actions
    AppletCopyPrimary(String),
    AppletCopySecret(String),
    AppletSecretReceived(Result<Secret, (String, String)>),
    AppletOpenInVault(String),
    AppletOpenLink(String),
    /// Search-result row pointer enter (`true`) / exit (`false`). Exit of a
    /// row only clears hover when that row is still the hovered one, so
    /// moving to a neighbour cannot race-clear the new id.
    AppletSearchRowHoverChanged(String, bool),

    // Applet popup: inline reprompt for AppletCopySecret
    AppletRepromptPasswordChanged(String),
    AppletRepromptSubmitted,
    AppletRepromptCancelled,

    // Protocol version check
    ProtocolVersionCheck(Result<bool, String>),

    // Applet popup: password reveal toggles
    AppletToggleUnlockPasswordReveal,
    AppletToggleRepromptPasswordReveal,

    // Toast dismissal
    CloseToast(ToastId),

    // UI actions
    ToggleRevealField(String, String), // id, field
    ToggleMasterPasswordReveal,
    SettingsViewClicked,

    // Results
    AuthResult(Result<(), String>),
    LockResult,
    LogoutResult,
    LockClicked,
    LogoutClicked,
    SyncClicked,
    SyncResult(Result<(), String>),
    EventReceived(cosmic_bwarden_core::protocol::Event),
    TogglePin(String),
    ToggleSearchPinned,
    ToggleEditPasswordReveal,
    ToggleRepromptPasswordReveal,

    // Settings editing
    SettingsEditClicked,
    SettingsSaveClicked,
    SettingsCancelClicked,
    SettingsServerChanged(String),
    SettingsLockTimeoutChanged(u32),
    // TPM PIN setup during login
    LoginPinEnabledToggled(bool),
    LoginPinChanged(String),
    LoginPinRevealToggled,

    // TPM / PIN unlock
    AppletPinChanged(String),
    AppletPinSubmitted,
    AppletPinResult(Result<(), String>),
    AppletTogglePinReveal,
    AppletUseMasterPasswordInstead,
    // Main-window PIN unlock (shown in View::Unlock when tpm_configured)
    MainWindowPinChanged(String),
    MainWindowPinSubmitted,
    /// Result of a main-window PIN unlock attempt (mirror of `AppletPinResult`).
    MainWindowPinResult(Result<(), String>),
    TpmStatusReceived(Result<(bool, bool), String>),
    TpmDaStatusReceived(Option<cosmic_bwarden_core::protocol::TpmDaStatus>),
    TpmDiagnosticsReceived(Vec<(String, bool, String)>),
    TpmSetupFormToggle,
    TpmDisableFormToggle,
    TpmSetupPinChanged(String),
    TpmSetupPinRevealToggled,
    TpmSetupSubmitted,
    TpmSetupResult(Result<(), String>),
    TpmDisableSubmitted,
    TpmDisableResult(Result<(), String>),
    SessionRestoreToggle,
    SessionRestorePasswordChanged(String),
    SessionRestoreRevealToggled,
    SessionRestoreSubmitted,

    // Password generator pane
    GeneratorViewClicked,
    /// Tab switch in the generator pane (Settings | History); the entity is
    /// looked up in `state::generator_tabs` for the `GeneratorTab` data.
    GeneratorTabActivated(widget::segmented_button::Entity),
    GeneratorUppercaseToggled(bool),
    GeneratorLowercaseToggled(bool),
    GeneratorNumbersToggled(bool),
    GeneratorSpecialToggled(bool),
    GeneratorLengthChanged(u32),
    /// Local-only: restores the pane's draft checkboxes/slider to
    /// `GeneratorSettings::default()`. Does not call the agent or touch the
    /// persisted "last used" settings — that only happens on the next Generate.
    GeneratorResetClicked,
    GeneratorGenerateClicked,
    GeneratorGenerated(Result<Secret, String>),
    GeneratorRevealToggled,
    GeneratorSettingsReceived(Result<GeneratorSettings, String>),
    GeneratorHistoryReceived(Result<Vec<GeneratorHistoryEntry>, String>),
    GeneratorHistoryRevealToggled(usize),
    /// Shows the confirmation dialog for deleting one history entry (index
    /// into the currently displayed `generator_history`).
    GeneratorHistoryDeleteRequested(usize),
    GeneratorHistoryDeleteConfirmed,
    GeneratorHistoryDeleteCancelled,
    GeneratorHistoryDeleted(Result<(), String>),

    // Applet popup: quick-generate (last-saved settings, copies to clipboard)
    AppletGeneratePasswordRequested,
    AppletGeneratePasswordReceived(Result<Secret, String>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_unlocked_true_for_vault_surfaces() {
        assert!(View::Vault.is_unlocked());
        assert!(View::Settings.is_unlocked());
        assert!(View::PasswordGenerator.is_unlocked());
    }

    #[test]
    fn is_unlocked_false_for_pre_vault_views() {
        assert!(!View::Loading.is_unlocked());
        assert!(!View::Setup.is_unlocked());
        assert!(!View::Unlock.is_unlocked());
    }
}
