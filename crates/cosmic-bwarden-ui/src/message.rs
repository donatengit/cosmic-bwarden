use cosmic::iced::window;
use cosmic::widget;
use cosmic::widget::ToastId;
use cosmic_bwarden_core::config::CosmicBWardenConfig;
use cosmic_bwarden_core::db::Entry;
use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};

#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Loading,
    Setup,
    Unlock,
    Vault,
    Settings,
}

#[derive(Debug, Clone)]
pub enum WindowState {
    Popup,
}

#[derive(Debug, Clone)]
pub enum Message {
    ConfigReceived(Result<(CosmicBWardenConfig, bool, bool, bool, bool), String>),

    // Window management
    WindowClosed(window::Id),
    RefreshStateInternal,
    AppletIconClicked(cosmic::iced::Vector, cosmic::iced::Rectangle),
    Exit,
    LockAndQuit,
    LogoutAndQuit,
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

    // Vault actions
    SearchChanged(String),
    FilterTypeChanged(Option<EntryType>),
    SelectEntry(String),
    EntryReceived(Result<Entry, String>),
    AddEntryRequested,
    EditEntry,
    CancelEdit,
    SaveEdit,
    EditFieldChanged(String, String), // field name, value
    EditNameChanged(String),
    EntriesReceived(u32, Result<Vec<SidebarEntry>, String>),
    CopyToClipboard(String),
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
    Surface(cosmic::surface::Action),
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
    AppletSecretReceived(Result<String, (String, String)>),
    AppletOpenInVault(String),
    AppletOpenLink(String),
    AppletQuitMenuToggle,

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
    VaultViewClicked,

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
    TpmStatusReceived(Result<(bool, bool, bool), String>),
    TpmDiagnosticsReceived(Vec<(String, bool, String)>),
    TpmSetupFormToggle,
    TpmDisableFormToggle,
    TpmSetupPinChanged(String),
    TpmSetupPinRevealToggled,
    TpmSetupSubmitted,
    TpmSetupResult(Result<(), String>),
    TpmDisableSubmitted,
    TpmDisableResult(Result<(), String>),
    TpmServerCredentialsToggled(bool),
    TpmServerCredentialsResult(Result<(), String>),
}
