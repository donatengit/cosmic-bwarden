use std::collections::{HashMap, HashSet};
use cosmic::app::Core;
use cosmic::iced::window;
use cosmic::widget;
use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};
use cosmic_bwarden_core::db::Entry;
use cosmic_bwarden_core::config::CosmicBWardenConfig;

use crate::message::{View, WindowState};

pub const APP_ID: &str = "com.system76.CosmicBWarden";

pub struct CosmicBWardenApp {
    pub core: Core,
    pub config: CosmicBWardenConfig,
    pub windows: HashMap<window::Id, WindowState>,

    // Global State
    pub view: View,
    pub search_query: String,
    pub entries: Vec<SidebarEntry>,
    pub top_entries: Vec<SidebarEntry>,
    pub error: Option<String>,
    pub selected_entry_id: Option<String>,
    pub selected_entry: Option<Entry>,
    pub editing_entry: Option<Entry>,
    pub filter_type: Option<EntryType>,
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

    // Applet State
    pub token_tx: Option<cosmic::cctk::sctk::reexports::calloop::channel::Sender<cosmic::applet::token::subscription::TokenRequest>>,
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

    // Settings editing state
    pub editing_config: Option<CosmicBWardenConfig>,
    pub settings_lock_timeout: String,
    pub settings_popular_count: String,
    pub settings_popular_days: String,
    pub master_password_revealed: bool,
    pub show_advanced: bool,
    pub notes_content: widget::text_editor::Content,
    pub search_id: u32,
    pub show_delete_confirm: Option<String>,
    pub show_reprompt: Option<String>,
    pub reprompt_password: String,
    pub edit_password_revealed: bool,
    pub reprompt_password_revealed: bool,
}

impl Default for CosmicBWardenApp {
    fn default() -> Self {
        Self {
            core: Core::default(),
            config: CosmicBWardenConfig::default(),
            windows: HashMap::new(),
            view: View::Loading,
            search_query: String::new(),
            entries: Vec::new(),
            top_entries: Vec::new(),
            error: None,
            selected_entry_id: None,
            selected_entry: None,
            editing_entry: None,
            filter_type: None,
            search_only_pinned: false,
            revealed_fields: HashSet::new(),
            login_email: String::new(),
            login_password: String::new(),
            login_server: String::new(),
            login_remember: true,
            login_verification_code: String::new(),
            show_verification_input: false,
            unlock_password: String::new(),
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
            editing_config: None,
            settings_lock_timeout: String::new(),
            settings_popular_count: String::new(),
            settings_popular_days: String::new(),
            master_password_revealed: false,
            show_advanced: false,
            notes_content: widget::text_editor::Content::new(),
            search_id: 0,
            show_delete_confirm: None,
            show_reprompt: None,
            reprompt_password: String::new(),
            edit_password_revealed: false,
            reprompt_password_revealed: false,
        }
    }
}

#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AppFlags;

impl cosmic::app::CosmicFlags for AppFlags {
    type SubCommand = String;
    type Args = Vec<String>;

    fn action(&self) -> Option<&String> {
        None
    }
}
