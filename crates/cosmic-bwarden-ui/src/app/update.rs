use cosmic::app::Task;
use cosmic::iced::window;
use cosmic::Action;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response, EntryType};
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::db::{Entry, EntryData, Secret};
use crate::message::{Message, View, WindowState};
use crate::fl;
use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::{fetch_sidebar_entries, fetch_top_entries};
use tracing::{debug, error};

impl CosmicBWardenApp {
    pub fn update_app(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ConfigReceived(res) => {
                match res {
                    Ok((config, needs_login, is_locked)) => {
                        self.config = config;
                        if needs_login {
                            self.view = View::Setup;
                        } else if is_locked {
                            self.view = View::Unlock;
                            if let Some(email) = &self.config.email {
                                self.login_email = email.clone();
                            }
                        } else {
                            self.view = View::Vault;
                        }
                    }
                    Err(e) => {
                        self.error = Some(e);
                        self.view = View::Setup;
                    }
                }
                Task::none()
            }
            Message::EventReceived(event) => {
                match event {
                    cosmic_bwarden_core::protocol::Event::Locked => {
                        self.view = View::Unlock;
                        self.selected_entry = None;
                        self.editing_entry = None;
                        self.selected_entry_id = None;
                    }
                    cosmic_bwarden_core::protocol::Event::Unlocked => {
                        self.view = View::Vault;
                        return Task::perform(async {}, |_| Action::App(Message::RefreshStateInternal));
                    }
                    cosmic_bwarden_core::protocol::Event::VaultChanged => {
                        return Task::perform(async {}, |_| Action::App(Message::RefreshStateInternal));
                    }
                }
                Task::none()
            }
            Message::WindowOpened(id) => {
                debug!("Window opened: {:?}", id);
                Task::none()
            }
            Message::WindowClosed(id) => {
                debug!("Window closed: {:?}", id);
                if self.applet_popup == Some(id) {
                    self.applet_popup = None;
                }
                self.windows.remove(&id);
                Task::none()
            }
            Message::OpenMainWindow => {
                let mut tasks = Vec::new();
                
                // In standalone mode, the primary window is already the main window
                if std::env::var("COSMIC_PANEL_NAME").is_err() {
                    if let Some(id) = self.core.main_window_id() {
                        tasks.push(window::gain_focus(id).map(move |_: ()| Action::App(Message::WindowOpened(id))));
                    }
                    return Task::batch(tasks);
                }

                let is_auth_view = matches!(self.view, View::Loading | View::Setup | View::Unlock);

                if is_auth_view {
                    if let Some((&id, _)) = self.windows.iter().find(|(_, w)| matches!(w, WindowState::Auth)) {
                        tasks.push(window::gain_focus(id).map(move |_: ()| Action::App(Message::WindowOpened(id))));
                    } else {
                        let settings = window::Settings {
                            size: cosmic::iced::Size::new(400.0, 600.0),
                            decorations: true,
                            resizable: true,
                            ..window::Settings::default()
                        };
                        let (id, spawn) = window::open(settings);
                        self.windows.insert(id, WindowState::Auth);
                        let title = if self.view == View::Setup { "Login" } else { "Unlock Vault" };
                        tasks.push(self.core.set_title(Some(id), title.to_string()));
                        tasks.push(spawn.map(move |_| Action::App(Message::WindowOpened(id))));
                    }
                } else {
                    if let Some((&id, _)) = self.windows.iter().find(|(_, w)| matches!(w, WindowState::Main)) {
                        tasks.push(window::gain_focus(id).map(move |_: ()| Action::App(Message::WindowOpened(id))));
                    } else {
                        let settings = window::Settings {
                            size: cosmic::iced::Size::new(1280.0, 800.0),
                            decorations: true,
                            resizable: true,
                            ..window::Settings::default()
                        };
                        let (id, spawn) = window::open(settings);
                        self.windows.insert(id, WindowState::Main);
                        tasks.push(self.core.set_title(Some(id), fl!("app-title").to_string()));
                        tasks.push(spawn.map(move |_: window::Id| Action::App(Message::WindowOpened(id))));
                    }
                }
                Task::batch(tasks)
            }
            Message::SpawnApplication => {
                if let Some(exe) = std::env::current_exe().ok() {
                    let mut cmd = std::process::Command::new(exe);
                    cmd.env("COSMIC_BWARDEN_MODE", "application");
                    // We must clear COSMIC_PANEL_NAME to ensure the new process doesn't think it's an applet
                    cmd.env_remove("COSMIC_PANEL_NAME");
                    tokio::spawn(cosmic::process::spawn(cmd));
                }
                Task::none()
            }
            Message::RefreshStateInternal => {
                Task::perform(async {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::GetConfig).await {
                        Ok(Response::Config { config, needs_login, is_locked }) => Ok((config, needs_login, is_locked)),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                }, |res| Action::App(Message::ConfigReceived(res)))
            }
            Message::AppletIconClicked(offset, bounds) => {
                if let Some(id) = self.applet_popup {
                    return Task::done(Action::Cosmic(cosmic::app::Action::Surface(cosmic::surface::action::destroy_popup(id))));
                }

                let mut tasks = Vec::new();
                
                // 1. Refresh state
                tasks.push(Task::perform(async {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::GetConfig).await {
                        Ok(Response::Config { config, needs_login, is_locked }) => Ok((config, needs_login, is_locked)),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                }, |res| Action::App(Message::ConfigReceived(res))));

                // 2. Open popup
                let popup_task = Task::done(Action::Cosmic(cosmic::app::Action::Surface(cosmic::surface::action::app_popup::<CosmicBWardenApp>(
                    move |state: &mut CosmicBWardenApp| {
                        let new_id = window::Id::unique();
                        state.applet_popup = Some(new_id);
                        state.windows.insert(new_id, crate::message::WindowState::Popup);
                        let mut popup_settings = state.core.applet.get_popup_settings(
                            state.core.main_window_id().unwrap_or(window::Id::RESERVED),
                            new_id,
                            None,
                            None,
                            None,
                        );
                        popup_settings.positioner.anchor_rect = cosmic::iced::Rectangle {
                            x: (bounds.x - offset.x) as i32,
                            y: (bounds.y - offset.y) as i32,
                            width: bounds.width as i32,
                            height: bounds.height as i32,
                        };
                        popup_settings
                    },
                    None,
                ))));
                tasks.push(popup_task);

                Task::batch(tasks)
            }
            Message::EmailChanged(e) => {
                self.login_email = e;
                Task::none()
            }
            Message::PasswordChanged(p) => {
                self.login_password = p;
                Task::none()
            }
            Message::ServerChanged(s) => {
                self.login_server = s;
                Task::none()
            }
            Message::RememberChanged(r) => {
                self.login_remember = r;
                Task::none()
            }
            Message::VerificationCodeChanged(c) => {
                self.login_verification_code = c;
                Task::none()
            }
            Message::LoginSubmitted => {
                let email = self.login_email.clone();
                let password = self.login_password.clone();
                let server_url = if self.login_server.trim().is_empty() { None } else { Some(self.login_server.clone()) };
                let remember_me = self.login_remember;
                let device_verification_code = if self.login_verification_code.is_empty() { None } else { Some(self.login_verification_code.clone()) };
                
                self.view = View::Loading;
                Task::perform(async move {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::Login {
                        email,
                        password,
                        server_url,
                        remember_me,
                        two_factor_token: None,
                        two_factor_provider: None,
                        two_factor_code: None,
                        device_verification_code,
                    }).await {
                        Ok(Response::Ack) => Ok(()),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                }, |res| Action::App(Message::AuthResult(res)))
            }
            Message::UnlockPasswordChanged(p) => {
                self.unlock_password = p;
                Task::none()
            }
            Message::UnlockSubmitted => {
                let password = self.unlock_password.clone();
                self.view = View::Loading;
                Task::perform(async move {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::Unlock { password }).await {
                        Ok(Response::Ack) => Ok(()),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                }, |res| Action::App(Message::AuthResult(res)))
            }
            Message::AuthResult(res) => {
                match res {
                    Ok(()) => {
                        self.view = View::Vault;
                        self.error = None;
                        self.login_password = String::new();
                        self.unlock_password = String::new();
                        
                        let mut tasks = vec![
                           fetch_sidebar_entries(self.search_id, None, None),
                           fetch_top_entries(self.config.top_popular_count as usize, Some(self.config.top_popular_days)),
                        ];

                        if std::env::var("COSMIC_PANEL_NAME").is_ok() {
                            if let Some((&id, _)) = self.windows.iter().find(|(_, w)| matches!(w, WindowState::Auth)) {
                                tasks.push(window::close(id).map(move |_: ()| Action::App(Message::WindowClosed(id))));
                                let settings = window::Settings::default();
                                let (new_id, spawn) = window::open(settings);
                                self.windows.insert(new_id, WindowState::Main);
                                tasks.push(self.core.set_title(Some(new_id), fl!("app-title").to_string()));
                                tasks.push(spawn.map(move |_: window::Id| Action::App(Message::WindowOpened(new_id))));
                            } else if self.windows.iter().find(|(_, w)| matches!(w, WindowState::Main)).is_none() {
                                // If no windows are open (e.g. auth happened via applet popup or background), open main window
                                let settings = window::Settings::default();
                                let (new_id, spawn) = window::open(settings);
                                self.windows.insert(new_id, WindowState::Main);
                                tasks.push(self.core.set_title(Some(new_id), fl!("app-title").to_string()));
                                tasks.push(spawn.map(move |_: window::Id| Action::App(Message::WindowOpened(new_id))));
                            }
                        }
                        return Task::batch(tasks);
                    }
                    Err(e) => {
                        if e == "new_device_verification_required" {
                            self.show_verification_input = true;
                            self.error = None;
                            self.view = View::Setup;
                        } else {
                            self.error = Some(e);
                            self.view = if self.config.email.is_some() { View::Unlock } else { View::Setup };
                        }
                        if let Some(err) = &self.error {
                            error!("Auth failed: {}", err);
                        }
                    }
                }
                Task::none()
            }

            Message::SearchChanged(q) => {
                self.search_query = q;
                self.search_id += 1;
                fetch_sidebar_entries(self.search_id, Some(self.search_query.clone()), self.filter_type.clone())
            }
            Message::SearchSubmitted(q) => {
                self.search_id += 1;
                fetch_sidebar_entries(self.search_id, Some(q), self.filter_type.clone())
            }
            Message::FilterTypeChanged(t) => {
                self.filter_type = t;
                self.search_id += 1;
                fetch_sidebar_entries(self.search_id, Some(self.search_query.clone()), self.filter_type.clone())
            }
            Message::SelectEntry(id) => {
                self.selected_entry_id = Some(id.clone());
                self.selected_entry = None;
                self.editing_entry = None;
                self.view = View::Vault;
                Task::perform(async move {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::GetEntry { id, password: None }).await {
                        Ok(Response::Entry { entry }) => Ok(entry),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                }, |res| Action::App(Message::EntryReceived(res)))
            }
            Message::EntryReceived(res) => {
                match res {
                    Ok(entry) => {
                        self.notes_content = cosmic::widget::text_editor::Content::with_text(entry.notes.as_deref().unwrap_or(""));
                        self.selected_entry = Some(entry);
                        self.show_reprompt = None;
                        self.reprompt_password = String::new();
                    }
                    Err(e) if e == "reprompt_required" => {
                        self.show_reprompt = self.selected_entry_id.clone();
                    }
                    Err(e) => {
                        self.error = Some(e);
                    }
                }
                Task::none()
            }
            Message::AddEntryRequested => {
                let new_entry = Entry {
                    id: format!("new-{}", chrono::Utc::now().timestamp()),
                    org_id: None,
                    folder: None,
                    folder_id: None,
                    name: "New Entry".to_string(),
                    data: EntryData::Login {
                        username: Some(String::new()),
                        password: Some(String::new().into()),
                        totp: None,
                        uris: Vec::new(),
                    },
                    fields: Vec::new(),
                    notes: Some(Secret::from(String::new())),
                    history: Vec::new(),
                    key: None,
                    master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
                };
                self.selected_entry = Some(new_entry.clone());
                self.editing_entry = Some(new_entry);
                self.selected_entry_id = None;
                self.notes_content = cosmic::widget::text_editor::Content::new();
                self.edit_password_revealed = false;
                Task::none()
            }
            Message::EditEntry => {
                if let Some(entry) = &self.selected_entry {
                    self.editing_entry = Some(entry.clone());
                    self.notes_content = cosmic::widget::text_editor::Content::with_text(entry.notes.as_deref().unwrap_or(""));
                    self.edit_password_revealed = false;
                }
                Task::none()
            }
            Message::CancelEdit => {
                self.editing_entry = None;
                self.notes_content = cosmic::widget::text_editor::Content::new();
                Task::none()
            }
            Message::SaveEdit => {
                if let Some(mut entry) = self.editing_entry.take() {
                    if let Some(notes) = &entry.notes {
                        if notes.trim().is_empty() {
                            entry.notes = None;
                        }
                    }
                    Task::perform(async move {
                        let agent = AgentClient::new();
                        match agent.send(AgentAction::UpdateEntry { entry }).await {
                            Ok(Response::Ack) => Ok(()),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    }, |res| Action::App(Message::SaveEditResult(res)))
                } else {
                    Task::none()
                }
            }
            Message::SaveEditResult(res) => {
                match res {
                    Ok(()) => {
                        let id = self.selected_entry_id.clone();
                        self.editing_entry = None;
                        // Refresh sidebar
                        self.search_id += 1;
                        let sidebar_task = fetch_sidebar_entries(self.search_id, Some(self.search_query.clone()), self.filter_type.clone());
                        
                        // Re-fetch selected entry if any
                        if let Some(id) = id {
                            Task::batch(vec![
                                sidebar_task,
                                Task::done(Action::App(Message::SelectEntry(id))),
                            ])
                        } else {
                            sidebar_task
                        }
                    }
                    Err(e) => {
                        self.error = Some(e);
                        Task::none()
                    }
                }
            }
            Message::EditFieldChanged(field, value) => {
                if let Some(entry) = &mut self.editing_entry {
                    match &mut entry.data {
                        EntryData::Login { username, password, .. } => {
                            if field == "Username" {
                                *username = Some(value.clone());
                            } else if field == "Password" {
                                *password = Some(value.clone().into());
                            }
                        }
                        EntryData::SshKey { private_key, public_key, fingerprint } => {
                            if field == "Private Key" {
                                *private_key = Some(value.clone().into());
                            } else if field == "Public Key" {
                                *public_key = Some(value.clone());
                            } else if field == "Fingerprint" {
                                *fingerprint = Some(value.clone());
                            }
                        }
                        EntryData::Card { number, cardholder_name, brand, .. } => {
                            if field == "Card Number" {
                                *number = Some(value.clone().into());
                            } else if field == "Cardholder" {
                                *cardholder_name = Some(value.clone());
                            } else if field == "Brand" {
                                *brand = Some(value.clone());
                            }
                        }
                        EntryData::Identity { username, email, .. } => {
                            if field == "Username" {
                                *username = Some(value.clone());
                            } else if field == "Email" {
                                *email = Some(value.clone());
                            }
                        }
                        EntryData::SecureNote => {}
                    }
                    
                    // Also check custom fields
                    if let Some(f) = entry.fields.iter_mut().find(|f| f.name.as_deref() == Some(&field)) {
                        f.value = Some(value.into());
                    }
                }
                Task::none()
            }

            Message::EditNameChanged(name) => {
                if let Some(entry) = &mut self.editing_entry {
                    entry.name = name;
                }
                Task::none()
            }
            Message::NotesAction(action) => {
                self.notes_content.perform(action);
                if let Some(entry) = &mut self.editing_entry {
                    entry.notes = Some(self.notes_content.text().into());
                }
                Task::none()
            }
            Message::DeleteEntry(id) => {
                self.show_delete_confirm = Some(id);
                Task::none()
            }
            Message::ConfirmDelete => {
                if let Some(id) = self.show_delete_confirm.take() {
                    Task::perform(async move {
                        let agent = AgentClient::new();
                        match agent.send(AgentAction::DeleteEntry { id }).await {
                            Ok(Response::Ack) => Ok(()),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    }, |res| Action::App(Message::DeleteEntryResult(res)))
                } else {
                    Task::none()
                }
            }
            Message::CancelDelete => {
                self.show_delete_confirm = None;
                Task::none()
            }
            Message::DeleteEntryResult(res) => {
                match res {
                    Ok(()) => {
                        self.selected_entry_id = None;
                        self.selected_entry = None;
                        self.editing_entry = None;
                        self.search_id += 1;
                        fetch_sidebar_entries(self.search_id, Some(self.search_query.clone()), self.filter_type.clone())
                    }
                    Err(e) => {
                        self.error = Some(e);
                        Task::none()
                    }
                }
            }
            Message::EntriesReceived(id, res) => {
                if id == self.search_id {
                    match res {
                        Ok(entries) => {
                            self.entries = entries;
                            self.error = None;
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                Task::none()
            }
            Message::TopEntriesReceived(res) => {
                match res {
                    Ok(entries) => {
                        self.top_entries = entries;
                        self.error = None;
                    }
                    Err(e) => self.error = Some(e),
                }
                Task::none()
            }
            Message::CopyPassword(id) => {
                Task::perform(async move {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::GetPassword { id, password: None }).await {
                        Ok(Response::Password { password }) => {
                           Ok(password)
                        }
                        _ => Err("failed to get password".to_string()),
                    }
                }, |res| Action::App(res)).then(|res| match res {
                    Action::App(Ok(p)) => cosmic::iced::clipboard::write(p).map(|_: ()| Action::None),
                    _ => Task::done(Action::None),
                })
            }
            Message::CopyToClipboard(text) => {
                cosmic::iced::clipboard::write(text).map(|_: ()| Action::None)
            }
            Message::PopupClosed(id) => {
                if self.applet_popup == Some(id) {
                    self.applet_popup = None;
                }
                self.windows.remove(&id);
                Task::none()
            }
            Message::Surface(action) => Task::done(Action::Cosmic(cosmic::app::Action::Surface(action))),
            Message::Exit => {
                debug!("Exit requested. Shutting down.");
                std::process::exit(0)
            }
            Message::ConfigChanged(config) => {
                self.config = config;
                Task::none()
            }
            Message::ToggleRevealField(id, field) => {
                let key = (id, field);
                if self.revealed_fields.contains(&key) {
                    self.revealed_fields.remove(&key);
                } else {
                    self.revealed_fields.insert(key);
                }
                Task::none()
            }
            Message::ToggleMasterPasswordReveal => {
                self.master_password_revealed = !self.master_password_revealed;
                Task::none()
            }
            Message::ToggleEditPasswordReveal => {
                self.edit_password_revealed = !self.edit_password_revealed;
                Task::none()
            }
            Message::SettingsViewClicked => {
                self.view = View::Settings;
                self.selected_entry_id = None;
                self.selected_entry = None;
                self.editing_entry = None;
                Task::none()
            }
            Message::VaultViewClicked => {
                self.view = View::Vault;
                Task::none()
            }
            Message::LockClicked => {
                Task::perform(async {
                    let agent = AgentClient::new();
                    let _ = agent.send(AgentAction::Lock).await;
                    ()
                }, |_| Action::App(Message::LockResult))
            }
            Message::LockResult => {
                self.view = View::Unlock;
                self.entries.clear();
                self.top_entries.clear();
                self.selected_entry_id = None;
                self.selected_entry = None;
                self.editing_entry = None;
                self.revealed_fields.clear();
                Task::none()
            }
            Message::LogoutClicked => {
                Task::perform(async {
                    let agent = AgentClient::new();
                    let _ = agent.send(AgentAction::Logout).await;
                    ()
                }, |_| Action::App(Message::LogoutResult))
            }
            Message::LogoutResult => {
                self.view = View::Setup;
                self.entries.clear();
                self.top_entries.clear();
                self.selected_entry_id = None;
                self.selected_entry = None;
                self.editing_entry = None;
                self.revealed_fields.clear();
                Task::none()
            }
            Message::SyncClicked => {
                Task::perform(async {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::Sync).await {
                        Ok(Response::Ack) => Ok(()),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                }, |res| Action::App(Message::SyncResult(res)))
            }
            Message::SyncResult(res) => {
                match res {
                    Ok(()) => {
                        self.search_id += 1;
                        Task::batch(vec![
                            fetch_sidebar_entries(self.search_id, Some(self.search_query.clone()), self.filter_type.clone()),
                            fetch_top_entries(self.config.top_popular_count as usize, Some(self.config.top_popular_days)),
                        ])
                    }
                    Err(e) => {
                        self.error = Some(e);
                        Task::none()
                    }
                }
            }
            Message::TogglePin(id) => {
                let is_pinned = self.entries.iter().find(|e| e.id == id).map(|e| e.is_pinned).unwrap_or(false);
                let action = if is_pinned {
                    AgentAction::UnpinEntry { id: id.clone() }
                } else {
                    AgentAction::PinEntry { id: id.clone() }
                };
                
                Task::perform(async move {
                    let agent = AgentClient::new();
                    let _ = agent.send(action).await;
                    ()
                }, move |_| {
                    Action::App(Message::SyncResult(Ok(()))) // Reuse SyncResult to refresh
                })
            }
            Message::ToggleSearchPinned => {
                self.search_only_pinned = !self.search_only_pinned;
                self.search_id += 1;
                fetch_sidebar_entries(self.search_id, Some(self.search_query.clone()), self.filter_type.clone())
            }
            Message::SettingsEditClicked => {
                self.editing_config = Some(self.config.clone());
                self.settings_lock_timeout = format!("{}", self.config.lock_timeout / 60);
                self.settings_popular_count = format!("{}", self.config.top_popular_count);
                self.settings_popular_days = format!("{}", self.config.top_popular_days);
                Task::none()
            }
            Message::SettingsSaveClicked => {
                if let Some(mut config) = self.editing_config.take() {
                    if let Ok(minutes) = self.settings_lock_timeout.parse::<u64>() {
                        config.lock_timeout = minutes * 60;
                    }
                    if let Ok(count) = self.settings_popular_count.parse::<u32>() {
                        config.top_popular_count = count;
                    }
                    if let Ok(days) = self.settings_popular_days.parse::<u32>() {
                        config.top_popular_days = days;
                    }
                    self.config = config.clone();
                    Task::done(Action::App(Message::VaultViewClicked))
                } else {
                    Task::none()
                }
            }
            Message::SettingsCancelClicked => {
                self.editing_config = None;
                Task::none()
            }
            Message::SettingsEmailChanged(e) => {
                if let Some(config) = &mut self.editing_config {
                    config.email = Some(e);
                }
                Task::none()
            }
            Message::SettingsServerChanged(s) => {
                if let Some(config) = &mut self.editing_config {
                    config.base_url = Some(s);
                }
                Task::none()
            }
            Message::SettingsLockTimeoutChanged(v) => {
                self.settings_lock_timeout = v;
                Task::none()
            }
            Message::SettingsPopularCountChanged(v) => {
                self.settings_popular_count = v;
                Task::none()
            }
            Message::SettingsPopularDaysChanged(v) => {
                self.settings_popular_days = v;
                Task::none()
            }
            Message::RepromptPasswordChanged(p) => {
                self.reprompt_password = p;
                Task::none()
            }
            Message::SubmitReprompt => {
                if let Some(id) = self.show_reprompt.clone() {
                    let password = self.reprompt_password.clone();
                    Task::perform(async move {
                        let agent = AgentClient::new();
                        match agent.send(AgentAction::GetEntry { id, password: Some(password) }).await {
                            Ok(Response::Entry { entry }) => Ok(entry),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    }, |res| Action::App(Message::EntryReceived(res)))
                } else {
                    Task::none()
                }
            }
            Message::CancelReprompt => {
                self.show_reprompt = None;
                self.reprompt_password = String::new();
                Task::none()
            }
            Message::NewEntryTypeChanged(ty) => {
                if let Some(entry) = &mut self.editing_entry {
                    match ty {
                        EntryType::Login => {
                           entry.data = EntryData::Login {
                               username: Some(String::new()),
                               password: Some(String::new().into()),
                               totp: None,
                               uris: Vec::new(),
                           };
                        }

                        EntryType::SecureNote => {
                           entry.data = EntryData::SecureNote;
                        }
                        EntryType::SshKey => {
                           entry.data = EntryData::SshKey {
                               private_key: Some(String::new().into()),
                               public_key: Some(String::new()),
                               fingerprint: None,
                           };
                        }

                        _ => {}
                    }
                }
                Task::none()
            }
            Message::ToggleAdvanced => {
                self.show_advanced = !self.show_advanced;
                Task::none()
            }
        }
    }
}
