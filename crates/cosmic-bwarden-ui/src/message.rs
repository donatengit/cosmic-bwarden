use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};
use cosmic_bwarden_core::db::Entry;
use cosmic_bwarden_core::config::CosmicBWardenConfig;
use cosmic::iced::window;
use cosmic::widget;

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
    Main,
    Auth,
    Popup,
}

#[derive(Debug, Clone)]
pub enum Message {
    ConfigReceived(Result<(CosmicBWardenConfig, bool, bool), String>),

    // Window management
    WindowOpened(window::Id),
    WindowClosed(window::Id),
    OpenMainWindow,
    SpawnApplication,
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

    // Vault actions
    SearchChanged(String),
    SearchSubmitted(String),
    FilterTypeChanged(Option<String>),
    SelectEntry(String),
    EntryReceived(Result<Entry, String>),
    AddEntryRequested,
    EditEntry,
    CancelEdit,
    SaveEdit,
    EditFieldChanged(String, String), // field name, value
    EditNameChanged(String),
    EntriesReceived(u32, Result<Vec<SidebarEntry>, String>),
    TopEntriesReceived(Result<Vec<SidebarEntry>, String>),
    CopyPassword(String),
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
    PopupClosed(window::Id),
    Surface(cosmic::surface::Action),

    // Config actions
    ConfigChanged(CosmicBWardenConfig),

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

    // Settings editing
    SettingsEditClicked,
    SettingsSaveClicked,
    SettingsCancelClicked,
    SettingsEmailChanged(String),
    SettingsServerChanged(String),
    SettingsLockTimeoutChanged(String),
    SettingsPopularCountChanged(String),
    SettingsPopularDaysChanged(String),
}
