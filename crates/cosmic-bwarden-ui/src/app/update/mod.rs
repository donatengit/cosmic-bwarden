pub mod lifecycle;
pub mod auth;
pub mod vault;

use cosmic::app::Task;
use crate::message::Message;
use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::fetch_top_entries;

impl CosmicBWardenApp {
    pub fn update_app(&mut self, message: Message) -> Task<Message> {
        if let Some(task) = self.update_lifecycle(message.clone()) {
            return task;
        }

        if let Some(task) = self.update_auth(message.clone()) {
            return task;
        }

        if let Some(task) = self.update_vault(message.clone()) {
            return task;
        }

        // Handle remaining messages
        match message {
            Message::EmailChanged(_) | Message::PasswordChanged(_) | Message::ServerChanged(_) | 
            Message::RememberChanged(_) | Message::VerificationCodeChanged(_) | Message::LoginSubmitted |
            Message::UnlockPasswordChanged(_) | Message::UnlockSubmitted | Message::AuthResult(_) |
            Message::LockClicked | Message::LockResult | Message::LogoutClicked | Message::LogoutResult |
            Message::SearchChanged(_) | Message::SearchSubmitted(_) | Message::FilterTypeChanged(_) |
            Message::SelectEntry(_) | Message::EntryReceived(_) | Message::AddEntryRequested |
            Message::EditEntry | Message::CancelEdit | Message::SaveEdit | Message::SaveEditResult(_) |
            Message::EditFieldChanged(_, _) | Message::EditNameChanged(_) | Message::NotesAction(_) |
            Message::DeleteEntry(_) | Message::ConfirmDelete | Message::CancelDelete | Message::DeleteEntryResult(_) |
            Message::EntriesReceived(_, _) | Message::TopEntriesReceived(_) | Message::SyncClicked | Message::SyncResult(_) |
            Message::TogglePin(_) | Message::ToggleSearchPinned | Message::RepromptPasswordChanged(_) | 
            Message::SubmitReprompt | Message::CancelReprompt | Message::NewEntryTypeChanged(_) |
            Message::ConfigReceived(_) | Message::EventReceived(_) | Message::WindowOpened(_) | 
            Message::WindowClosed(_) | Message::OpenMainWindow | Message::RefreshStateInternal => Task::none(),

            Message::SpawnApplication => {
                if let Some(exe) = std::env::current_exe().ok() {
                    let mut cmd = std::process::Command::new(exe);
                    cmd.env("COSMIC_BWARDEN_MODE", "application");
                    cmd.env_remove("COSMIC_PANEL_NAME");
                    tokio::spawn(cosmic::process::spawn(cmd));
                }
                Task::none()
            }
            Message::AppletIconClicked(offset, bounds) => {
                use cosmic::iced::window;
                use cosmic_bwarden_core::protocol::Action as AgentAction;
                use cosmic_bwarden_core::protocol::Response;
                use cosmic_bwarden_core::agent_client::AgentClient;

                if let Some(id) = self.applet_popup {
                    return Task::done(cosmic::Action::Cosmic(cosmic::app::Action::Surface(cosmic::surface::action::destroy_popup(id))));
                }

                let mut tasks = Vec::new();
                tasks.push(Task::perform(async {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::GetConfig).await {
                        Ok(Response::Config { config, needs_login, is_locked }) => Ok((config, needs_login, is_locked)),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                }, |res| cosmic::Action::App(Message::ConfigReceived(res))));

                tasks.push(fetch_top_entries(self.config.top_popular_count as usize, Some(self.config.top_popular_days)));

                let popup_task = Task::done(cosmic::Action::Cosmic(cosmic::app::Action::Surface(cosmic::surface::action::app_popup::<CosmicBWardenApp>(
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
            Message::CopyPassword(id) => {
                use cosmic_bwarden_core::protocol::Action as AgentAction;
                use cosmic_bwarden_core::protocol::Response;
                use cosmic_bwarden_core::agent_client::AgentClient;

                Task::perform(async move {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::GetPassword { id, password: None }).await {
                        Ok(Response::Password { password }) => Ok(password),
                        _ => Err("failed to get password".to_string()),
                    }
                }, |res| cosmic::Action::App(res)).then(|res| match res {
                    cosmic::Action::App(Ok(p)) => cosmic::iced::clipboard::write(p).map(|_: ()| cosmic::Action::None),
                    _ => Task::done(cosmic::Action::None),
                })
            }
            Message::CopyToClipboard(text) => {
                cosmic::iced::clipboard::write(text).map(|_: ()| cosmic::Action::None)
            }
            Message::PopupClosed(id) => {
                if self.applet_popup == Some(id) {
                    self.applet_popup = None;
                }
                self.windows.remove(&id);
                Task::none()
            }
            Message::Surface(action) => Task::done(cosmic::Action::Cosmic(cosmic::app::Action::Surface(action))),
            Message::Exit => {
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
                self.view = crate::message::View::Settings;
                self.selected_entry_id = None;
                self.selected_entry = None;
                self.editing_entry = None;
                Task::none()
            }
            Message::VaultViewClicked => {
                self.view = crate::message::View::Vault;
                Task::none()
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
                    Task::done(cosmic::Action::App(Message::VaultViewClicked))
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
            Message::ToggleAdvanced => {
                self.show_advanced = !self.show_advanced;
                Task::none()
            }
        }
    }
}
