use cosmic::app::Core;
use cosmic::iced::window;
use cosmic::widget;
use cosmic_bwarden_core::config::CosmicBWardenConfig;
use cosmic_bwarden_core::db::Entry;
use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};
use std::collections::{HashMap, HashSet};

use crate::message::{View, WindowState};

/// libcosmic's `run_single_instance`/`dbus_activation` derive a D-Bus object
/// path directly from this by replacing `.` with `/` — each dot-separated
/// segment must therefore only contain `[A-Za-z0-9_]` (no hyphens; D-Bus
/// object paths reject them). A hyphenated segment here made
/// `conn.object_server().at(path, ...)` fail and `dbus_activation::subscription`
/// call `std::process::exit(1)`, silently killing "Open Vault Window" — see
/// `tests` below for the regression guard.
pub const APP_ID: &str = "com.enikeev.cosmic_bwarden";

#[cfg(test)]
mod app_id_tests {
    use super::APP_ID;

    #[test]
    fn app_id_is_a_valid_dbus_object_path_when_dot_replaced_with_slash() {
        for segment in APP_ID.split('.') {
            assert!(
                !segment.is_empty()
                    && segment
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "APP_ID segment {segment:?} is not a valid D-Bus object path \
                 segment (only [A-Za-z0-9_] allowed) — this breaks \
                 run_single_instance/dbus_activation for the Application-mode \
                 window (\"Open Vault Window\")"
            );
        }
    }
}

/// The two panes of the main vault window's resizable split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultPane {
    Sidebar,
    Content,
}

/// Minimum — and initial — width of the vault sidebar, in logical pixels.
pub const SIDEBAR_MIN_WIDTH: f32 = 340.0;
/// Upper drag limit for the sidebar split, as a share of the window width.
pub const SIDEBAR_MAX_RATIO: f32 = 0.60;
/// Sidebar share used only until the first window-size report arrives, at
/// which point the split snaps to exactly [`SIDEBAR_MIN_WIDTH`].
pub const SIDEBAR_DEFAULT_RATIO: f32 = 0.35;

/// Lowest allowed sidebar ratio for the given window width: the ratio that
/// keeps the sidebar at [`SIDEBAR_MIN_WIDTH`], capped so the clamp range
/// stays valid on windows too narrow to honor the pixel minimum.
pub fn sidebar_min_ratio(window_width: f32) -> f32 {
    (SIDEBAR_MIN_WIDTH / window_width).min(SIDEBAR_MAX_RATIO)
}

pub struct CosmicBWardenApp {
    pub core: Core,
    pub config: CosmicBWardenConfig,
    pub windows: HashMap<window::Id, WindowState>,

    // Global State
    pub view: View,
    /// True while a login/unlock/pin IPC request is in flight.
    /// Disables the submit button without switching away from the form view.
    pub auth_loading: bool,
    pub search_query: String,
    pub entries: Vec<SidebarEntry>,
    pub all_entries: Vec<SidebarEntry>,
    pub error: Option<String>,
    pub selected_entry_id: Option<String>,
    pub selected_entry: Option<Entry>,
    pub editing_entry: Option<Entry>,
    pub filter_type: Option<EntryType>,
    /// Selection model backing the sidebar's type-filter segmented control.
    /// Item order matches `view::vault::sidebar::filter_to_idx`; kept in sync
    /// with `filter_type` by the `FilterTabActivated`/`FilterTypeChanged`
    /// handlers.
    pub filter_model: widget::segmented_button::SingleSelectModel,
    /// Pane layout of the vault window (sidebar | content), resizable by
    /// dragging the split.
    pub vault_panes: widget::pane_grid::State<VaultPane>,
    /// Current sidebar share of the window width, mirrored from `vault_panes`
    /// on every resize so it can be re-clamped when the window itself resizes.
    pub sidebar_ratio: f32,
    /// Last reported width of the vault window, in logical pixels; `None`
    /// until the runtime's first `on_window_resize` callback.
    pub vault_window_width: Option<f32>,
    pub search_only_pinned: bool,
    pub revealed_fields: HashSet<(String, String)>,

    // Login form
    pub login_email: String,
    pub login_password: String,
    pub login_server: String,
    pub login_remember: bool,
    pub login_verification_code: String,
    pub show_verification_input: bool,

    // Unlock form
    pub unlock_password: String,
    /// PIN entered on the master-password unlock view to (re-)enable PIN unlock on
    /// this device. Empty means "disable PIN unlock" when a (possibly stale) blob
    /// exists. Shared by the main-window and applet master-password unlock views.
    pub unlock_pin: String,
    pub unlock_pin_revealed: bool,
    /// Set when a master-password unlock is submitted from a view that showed the
    /// PIN (re-)enable field, so `AuthResult` knows to apply `unlock_pin` (and not
    /// confuse it with a login or PIN-unlock success).
    pub unlock_pin_apply_pending: bool,

    // Applet State
    pub token_tx: Option<
        cosmic::cctk::sctk::reexports::calloop::channel::Sender<
            cosmic::applet::token::subscription::TokenRequest,
        >,
    >,
    pub applet_popup: Option<window::Id>,
    pub applet_unlock_password: String,
    pub applet_error: Option<String>,
    pub applet_search_query: String,
    pub applet_search_only_favourites: bool,
    pub applet_search_results: Vec<SidebarEntry>,
    pub applet_search_id: u32,
    pub applet_reprompt_id: Option<String>,
    pub applet_reprompt_password: String,
    pub applet_unlock_password_revealed: bool,
    pub applet_reprompt_password_revealed: bool,
    pub applet_toasts: widget::Toasts<crate::message::Message>,
    pub applet_quit_expanded: bool,

    // Settings editing state
    pub editing_config: Option<CosmicBWardenConfig>,
    /// Login-time PIN setup (shown when TPM is available but not yet configured)
    pub login_pin_enabled: bool,
    pub login_pin: String,
    pub login_pin_revealed: bool,
    pub master_password_revealed: bool,
    pub show_advanced: bool,
    pub notes_content: widget::text_editor::Content,
    pub search_id: u32,
    pub show_delete_confirm: Option<String>,
    pub show_reprompt: Option<String>,
    pub reprompt_password: String,
    pub edit_password_revealed: bool,
    pub reprompt_password_revealed: bool,
    pub protocol_mismatch: bool,
    /// Entry id received via `Event::OpenEntry` before the vault view was ready.
    /// Applied as `SelectEntry` on the next transition to `View::Vault`.
    pub pending_vault_entry: Option<String>,
    /// Mirrors `State::sync_failed` from the agent. True when the last server
    /// operation (add/update/delete/sync) failed. Shown as a red "Not synced"
    /// button so the user knows the vault may be out of date.
    pub sync_failed: bool,
    /// True while a Sync request is in flight; shows a spinner on the Sync button.
    pub syncing: bool,
    /// True while a DeleteEntry request is in flight; shows a spinner in place of
    /// the Delete button so the user knows the operation is running.
    pub deleting: bool,

    // TPM / PIN unlock state
    /// False until the first TpmStatusReceived arrives. Prevents showing
    /// "not accessible" before the agent has been queried.
    pub tpm_status_known: bool,
    pub tpm_available: bool,
    pub tpm_configured: bool,
    /// True when the master_password_hash is also sealed in the TPM.
    pub tpm_server_credentials: bool,
    /// When true, the applet unlock widget shows a PIN field instead of password.
    pub show_pin_unlock: bool,
    pub applet_pin: String,
    pub applet_pin_revealed: bool,
    /// PIN for the main-window unlock view (View::Unlock when tpm_configured).
    pub main_window_pin: String,
    // TPM settings dialog state
    pub show_tpm_setup_form: bool,
    pub tpm_setup_pin: String,
    pub tpm_setup_pin_revealed: bool,
    pub show_tpm_disable_form: bool,
    /// Diagnostic check results populated by CheckTpmDiagnostics (shown when !tpm_available).
    pub tpm_diagnostics: Vec<(String, bool, String)>,
    /// Last error from a TPM setup/disable operation — shown in the settings TPM section.
    pub tpm_error: Option<String>,
    /// TPM dictionary-attack lockout status (attempts remaining before lockout).
    /// Fetched on demand; `None` until first queried or if the TPM is unavailable.
    pub tpm_da: Option<cosmic_bwarden_core::protocol::TpmDaStatus>,
    /// True after a failed PIN unlock in the current lock session. Gates the
    /// "attempts remaining" line so it only appears once the user got the PIN
    /// wrong, not on every PIN prompt. Reset on a fresh prompt / success / lock.
    pub pin_incorrect: bool,
    /// True when the agent reports a configured account (`has_account`). Used to
    /// avoid prompting to unlock when there is nothing to unlock (no account/email).
    pub has_account: bool,

    // Password generator pane
    /// Current draft checkbox/slider state — may differ from the device's
    /// last-saved settings until Generate is pressed (Reset restores schema
    /// defaults locally, without touching what's persisted).
    pub generator_settings: cosmic_bwarden_core::protocol::GeneratorSettings,
    pub generator_result: Option<String>,
    pub generator_result_revealed: bool,
    pub generator_history: Vec<cosmic_bwarden_core::protocol::GeneratorHistoryEntry>,
    /// Per-row reveal state, keyed by index into `generator_history`. Safe to
    /// key by index because the pane always refetches and replaces the whole
    /// Vec together (on open and after each Generate), so indices never go
    /// stale mid-session.
    pub generator_history_revealed: HashSet<usize>,
    pub generator_error: Option<String>,
    /// Index into `generator_history` pending delete confirmation; shown via
    /// the same dialog widget as vault-entry deletion (`view_dialogs`).
    pub generator_history_delete_pending: Option<usize>,

    // Clipboard auto-clear (`[P1-9]`)
    /// Bumped on every copy; a pending clear only fires if its generation is
    /// still current (i.e. no newer copy has claimed the clipboard since).
    pub clipboard_clear_generation: u32,
    /// The value most recently copied, held only while a clear is scheduled so
    /// the readback can verify the clipboard still contains it before wiping.
    /// Zeroized when the clear resolves either way.
    pub clipboard_pending_clear: Option<String>,
}

impl Default for CosmicBWardenApp {
    fn default() -> Self {
        Self {
            core: Core::default(),
            config: CosmicBWardenConfig::default(),
            windows: HashMap::new(),
            view: View::Loading,
            auth_loading: false,
            search_query: String::new(),
            entries: Vec::new(),
            all_entries: Vec::new(),
            error: None,
            selected_entry_id: None,
            selected_entry: None,
            editing_entry: None,
            filter_type: None,
            filter_model: crate::view::vault::sidebar::filter_model(),
            vault_panes: widget::pane_grid::State::with_configuration(
                widget::pane_grid::Configuration::Split {
                    axis: widget::pane_grid::Axis::Vertical,
                    ratio: SIDEBAR_DEFAULT_RATIO,
                    a: Box::new(widget::pane_grid::Configuration::Pane(VaultPane::Sidebar)),
                    b: Box::new(widget::pane_grid::Configuration::Pane(VaultPane::Content)),
                },
            ),
            sidebar_ratio: SIDEBAR_DEFAULT_RATIO,
            vault_window_width: None,
            search_only_pinned: false,
            revealed_fields: HashSet::new(),
            login_email: String::new(),
            login_password: String::new(),
            login_server: String::new(),
            login_remember: true,
            login_verification_code: String::new(),
            show_verification_input: false,
            unlock_password: String::new(),
            unlock_pin: String::new(),
            unlock_pin_revealed: false,
            unlock_pin_apply_pending: false,
            token_tx: None,
            applet_popup: None,
            applet_unlock_password: String::new(),
            applet_error: None,
            applet_search_query: String::new(),
            applet_search_only_favourites: false,
            applet_search_results: Vec::new(),
            applet_search_id: 0,
            applet_reprompt_id: None,
            applet_reprompt_password: String::new(),
            applet_unlock_password_revealed: false,
            applet_reprompt_password_revealed: false,
            applet_toasts: widget::Toasts::new(crate::message::Message::CloseToast),
            applet_quit_expanded: false,
            editing_config: None,
            login_pin_enabled: false,
            login_pin: String::new(),
            login_pin_revealed: false,
            master_password_revealed: false,
            show_advanced: false,
            notes_content: widget::text_editor::Content::new(),
            search_id: 0,
            show_delete_confirm: None,
            show_reprompt: None,
            reprompt_password: String::new(),
            edit_password_revealed: false,
            reprompt_password_revealed: false,
            protocol_mismatch: false,
            pending_vault_entry: None,
            sync_failed: false,
            syncing: false,
            deleting: false,
            tpm_status_known: false,
            tpm_available: false,
            tpm_configured: false,
            tpm_server_credentials: false,
            show_pin_unlock: false,
            applet_pin: String::new(),
            applet_pin_revealed: false,
            main_window_pin: String::new(),
            show_tpm_setup_form: false,
            tpm_setup_pin: String::new(),
            tpm_setup_pin_revealed: false,
            show_tpm_disable_form: false,
            tpm_diagnostics: Vec::new(),
            tpm_error: None,
            tpm_da: None,
            pin_incorrect: false,
            has_account: false,
            clipboard_clear_generation: 0,
            clipboard_pending_clear: None,
            generator_settings: cosmic_bwarden_core::protocol::GeneratorSettings::default(),
            generator_result: None,
            generator_result_revealed: false,
            generator_history: Vec::new(),
            generator_history_revealed: HashSet::new(),
            generator_error: None,
            generator_history_delete_pending: None,
        }
    }
}

#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AppFlags {
    pub config: Option<std::path::PathBuf>,
    pub socket: Option<std::path::PathBuf>,
}

impl cosmic::app::CosmicFlags for AppFlags {
    type SubCommand = String;
    type Args = Vec<String>;

    fn action(&self) -> Option<&String> {
        None
    }
}
