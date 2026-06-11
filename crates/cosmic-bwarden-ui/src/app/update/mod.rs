pub mod lifecycle;
pub mod auth;
pub mod vault;
pub mod applet;

use cosmic::app::Task;
use crate::message::Message;
use crate::app::state::CosmicBWardenApp;

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

        if let Some(task) = self.update_applet(message.clone()) {
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
            Message::WindowClosed(_) | Message::OpenMainWindow | Message::RefreshStateInternal |
            Message::SpawnApplication | Message::AppletIconClicked(_, _) | Message::CopyPassword(_) |
            Message::PopupClosed(_) | Message::Surface(_) | Message::Exit |
            Message::AppletUnlockPasswordChanged(_) | Message::AppletUnlockSubmitted | Message::AppletUnlockResult(_) |
            Message::AppletSearchChanged(_) | Message::AppletToggleFavouritesFilter | Message::AppletSearchResultsReceived(_, _) |
            Message::AppletCopyPrimary(_) | Message::AppletCopySecret(_) | Message::AppletSecretReceived(_) |
            Message::AppletRepromptPasswordChanged(_) | Message::AppletRepromptSubmitted | Message::AppletRepromptCancelled |
            Message::AppletToggleUnlockPasswordReveal | Message::AppletToggleRepromptPasswordReveal |
            Message::CloseToast(_) | Message::LockAndQuit | Message::LogoutAndQuit => Task::none(),

            Message::CopyToClipboard(text) => {
                cosmic::iced::clipboard::write(text).map(|_: ()| cosmic::Action::None)
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
