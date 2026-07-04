pub mod applet;
pub mod auth;
pub mod lifecycle;
pub mod vault;

use crate::app::state::CosmicBWardenApp;
use crate::message::Message;
use cosmic::app::Task;
use zeroize::Zeroize as _;

/// How long a copied value stays in the clipboard before the auto-clear
/// fires (`[P1-9]`). Matches upstream Bitwarden clients' default.
pub(crate) const CLIPBOARD_CLEAR_SECS: u64 = 30;

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
            Message::EmailChanged(_)
            | Message::PasswordChanged(_)
            | Message::ServerChanged(_)
            | Message::RememberChanged(_)
            | Message::VerificationCodeChanged(_)
            | Message::LoginSubmitted
            | Message::UnlockPasswordChanged(_)
            | Message::UnlockSubmitted
            | Message::UnlockPinChanged(_)
            | Message::UnlockPinRevealToggled
            | Message::AuthResult(_)
            | Message::LockClicked
            | Message::LockResult
            | Message::LogoutClicked
            | Message::LogoutResult
            | Message::SearchChanged(_)
            | Message::FilterTypeChanged(_)
            | Message::SelectEntry(_)
            | Message::EntryReceived(_)
            | Message::AddEntryRequested
            | Message::EditEntry
            | Message::CancelEdit
            | Message::SaveEdit
            | Message::SaveEditResult(_)
            | Message::EditFieldChanged(_, _)
            | Message::EditNameChanged(_)
            | Message::NotesAction(_)
            | Message::DeleteEntry(_)
            | Message::ConfirmDelete
            | Message::CancelDelete
            | Message::DeleteEntryResult(_)
            | Message::EntriesReceived(_, _)
            | Message::SyncClicked
            | Message::SyncResult(_)
            | Message::TogglePin(_)
            | Message::ToggleSearchPinned
            | Message::RepromptPasswordChanged(_)
            | Message::SubmitReprompt
            | Message::CancelReprompt
            | Message::NewEntryTypeChanged(_)
            | Message::ConfigReceived(_)
            | Message::EventReceived(_)
            | Message::WindowClosed(_)
            | Message::RefreshStateInternal
            | Message::AppletIconClicked(_, _)
            | Message::Surface(_)
            | Message::Exit
            | Message::LockAndQuit
            | Message::LogoutAndQuit
            | Message::OpenVaultRequested
            | Message::Token(_)
            | Message::AppletUnlockPasswordChanged(_)
            | Message::AppletUnlockSubmitted
            | Message::AppletUnlockResult(_)
            | Message::AppletSearchChanged(_)
            | Message::AppletToggleFavouritesFilter
            | Message::AppletSearchResultsReceived(_, _)
            | Message::AppletCopyPrimary(_)
            | Message::AppletCopySecret(_)
            | Message::AppletSecretReceived(_)
            | Message::AppletRepromptPasswordChanged(_)
            | Message::AppletRepromptSubmitted
            | Message::AppletRepromptCancelled
            | Message::AppletToggleUnlockPasswordReveal
            | Message::AppletToggleRepromptPasswordReveal
            | Message::AppletOpenInVault(_)
            | Message::AppletOpenLink(_)
            | Message::AppletQuitMenuToggle
            | Message::CloseToast(_)
            | Message::ProtocolVersionCheck(_)
            | Message::AppletPinChanged(_)
            | Message::AppletPinSubmitted
            | Message::AppletPinResult(_)
            | Message::AppletTogglePinReveal
            | Message::AppletUseMasterPasswordInstead
            | Message::MainWindowPinChanged(_)
            | Message::MainWindowPinSubmitted
            | Message::MainWindowPinResult(_)
            | Message::TpmStatusReceived(_)
            | Message::TpmDaStatusReceived(_)
            | Message::TpmDiagnosticsReceived(_)
            | Message::TpmSetupFormToggle
            | Message::TpmDisableFormToggle
            | Message::TpmSetupPinChanged(_)
            | Message::TpmSetupPinRevealToggled
            | Message::TpmSetupSubmitted
            | Message::TpmSetupResult(_)
            | Message::TpmDisableSubmitted
            | Message::TpmDisableResult(_)
            | Message::TpmServerCredentialsToggled(_)
            | Message::TpmServerCredentialsResult(_)
            | Message::LoginPinEnabledToggled(_)
            | Message::LoginPinChanged(_)
            | Message::LoginPinRevealToggled => Task::none(),

            Message::CopyToClipboard(text) => self.copy_to_clipboard_with_autoclear(text),
            Message::ClipboardClearElapsed(generation) => {
                if generation == self.clipboard_clear_generation
                    && self.clipboard_pending_clear.is_some()
                {
                    cosmic::iced::clipboard::read().map(move |contents| {
                        cosmic::Action::App(Message::ClipboardClearReadback(generation, contents))
                    })
                } else {
                    // A newer copy re-armed the timer; let that one decide.
                    Task::none()
                }
            }
            Message::ClipboardClearReadback(generation, contents) => {
                if generation != self.clipboard_clear_generation {
                    return Task::none();
                }
                let Some(mut ours) = self.clipboard_pending_clear.take() else {
                    return Task::none();
                };
                // Only wipe if the clipboard still holds our value — never
                // content the user copied from somewhere else since.
                let still_ours = contents.as_deref() == Some(ours.as_str());
                ours.zeroize();
                if still_ours {
                    cosmic::iced::clipboard::write(String::new()).map(|_: ()| cosmic::Action::None)
                } else {
                    Task::none()
                }
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
            Message::ToggleRepromptPasswordReveal => {
                self.reprompt_password_revealed = !self.reprompt_password_revealed;
                Task::none()
            }
            Message::SettingsViewClicked => {
                self.view = crate::message::View::Settings;
                self.selected_entry_id = None;
                self.selected_entry = None;
                self.editing_entry = None;
                // Refresh TPM status every time settings opens — hardware
                // state or group membership may have changed since startup.
                // The dictionary-attack counter is fetched by TpmStatusReceived,
                // but only once availability is confirmed (see lifecycle.rs).
                lifecycle::check_tpm_task()
            }
            Message::SettingsEditClicked => {
                self.editing_config = Some(self.config.clone());
                Task::none()
            }
            Message::SettingsSaveClicked => {
                if let Some(config) = self.editing_config.take() {
                    self.config = config.clone();
                    let lock_timeout = config.lock_timeout;
                    // Persist to disk so the agent picks it up on next restart.
                    if let Err(e) = config.save_legacy() {
                        tracing::error!("failed to save config: {}", e);
                    }
                    // Stay on the Settings pane after save.
                    self.view = crate::message::View::Settings;
                    // Notify the running agent to update its live timer.
                    Task::perform(
                        async move {
                            let agent = cosmic_bwarden_core::agent_client::AgentClient::new();
                            let _ = agent
                                .send(cosmic_bwarden_core::protocol::Action::UpdateLockTimeout {
                                    seconds: lock_timeout,
                                })
                                .await;
                        },
                        |_| cosmic::Action::None,
                    )
                } else {
                    Task::none()
                }
            }
            Message::SettingsCancelClicked => {
                self.editing_config = None;
                Task::none()
            }
            Message::SettingsServerChanged(s) => {
                if let Some(config) = &mut self.editing_config {
                    config.base_url = Some(s);
                }
                Task::none()
            }
            Message::SettingsLockTimeoutChanged(minutes) => {
                if let Some(config) = &mut self.editing_config {
                    config.lock_timeout = minutes as u64 * 60;
                }
                Task::none()
            }
            Message::ToggleAdvanced => {
                self.show_advanced = !self.show_advanced;
                Task::none()
            }
        }
    }

    /// Copy `value` to the clipboard and arm the auto-clear timer. Used by
    /// both the main-window and applet copy paths, so every copied credential
    /// is cleared after [`CLIPBOARD_CLEAR_SECS`] unless the user (or a newer
    /// copy) has replaced the clipboard contents by then.
    pub(crate) fn copy_to_clipboard_with_autoclear(&mut self, value: String) -> Task<Message> {
        self.clipboard_clear_generation = self.clipboard_clear_generation.wrapping_add(1);
        let generation = self.clipboard_clear_generation;
        if let Some(mut stale) = self.clipboard_pending_clear.replace(value.clone()) {
            stale.zeroize();
        }
        let write_task = cosmic::iced::clipboard::write(value).map(|_: ()| cosmic::Action::None);
        let timer_task = Task::perform(
            async move {
                tokio::time::sleep(std::time::Duration::from_secs(CLIPBOARD_CLEAR_SECS)).await;
                generation
            },
            |generation| cosmic::Action::App(Message::ClipboardClearElapsed(generation)),
        );
        Task::batch(vec![write_task, timer_task])
    }
}
