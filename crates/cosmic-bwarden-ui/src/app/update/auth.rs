use cosmic::app::Task;
use cosmic::Action;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response};
use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::fetch_sidebar_entries;
use crate::fl;
use crate::message::{Message, View};
use crate::MIN_PIN_LEN;
use tracing::error;
use zeroize::Zeroize;

impl CosmicBWardenApp {
    pub fn update_auth(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::EmailChanged(e) => {
                self.login_email = e;
                Some(Task::none())
            }
            Message::PasswordChanged(p) => {
                self.login_password = p;
                Some(Task::none())
            }
            Message::ServerChanged(s) => {
                self.login_server = s;
                Some(Task::none())
            }
            Message::RememberChanged(r) => {
                self.login_remember = r;
                Some(Task::none())
            }
            Message::VerificationCodeChanged(c) => {
                self.login_verification_code = c;
                Some(Task::none())
            }
            Message::LoginPinEnabledToggled(v) => {
                self.login_pin_enabled = v;
                if !v {
                    self.login_pin.zeroize();
                    self.login_pin_revealed = false;
                }
                Some(Task::none())
            }
            Message::LoginPinChanged(v) => {
                self.login_pin = v;
                Some(Task::none())
            }
            Message::LoginPinRevealToggled => {
                self.login_pin_revealed = !self.login_pin_revealed;
                Some(Task::none())
            }
            Message::MainWindowPinChanged(p) => {
                self.main_window_pin = p;
                Some(Task::none())
            }
            Message::MainWindowPinSubmitted => {
                let pin = self.main_window_pin.clone();
                self.auth_loading = true;
                Some(Task::perform(
                    async move {
                        let agent = AgentClient::new();
                        match agent.send(AgentAction::UnlockWithPin { pin }).await {
                            Ok(Response::Ack) => Ok(()),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    },
                    |res| Action::App(Message::AuthResult(res)),
                ))
            }
            Message::LoginSubmitted => {
                // Validate PIN length before sending the login request.
                if self.login_pin_enabled && !self.login_pin.is_empty() && self.login_pin.chars().count() < MIN_PIN_LEN {
                    self.error = Some(fl!("pin-too-short", count = MIN_PIN_LEN));
                    return Some(Task::none());
                }

                let email = self.login_email.clone();
                let password = self.login_password.clone();
                let server_url = Some(self.login_server.clone());
                let remember_me = self.login_remember;
                let device_verification_code = if self.login_verification_code.is_empty() {
                    None
                } else {
                    Some(self.login_verification_code.clone())
                };

                self.auth_loading = true;
                Some(Task::perform(
                    async move {
                        let agent = AgentClient::new();
                        match agent
                            .send(AgentAction::Login {
                                email,
                                password,
                                server_url,
                                remember_me,
                                two_factor_token: None,
                                two_factor_provider: None,
                                two_factor_code: None,
                                device_verification_code,
                            })
                            .await
                        {
                            Ok(Response::Ack) => Ok(()),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    },
                    |res| Action::App(Message::AuthResult(res)),
                ))
            }
            Message::UnlockPasswordChanged(p) => {
                self.unlock_password = p;
                Some(Task::none())
            }
            Message::UnlockPinChanged(p) => {
                self.unlock_pin = p;
                Some(Task::none())
            }
            Message::UnlockPinRevealToggled => {
                self.unlock_pin_revealed = !self.unlock_pin_revealed;
                Some(Task::none())
            }
            Message::UnlockSubmitted => {
                // If the user is (re-)enabling PIN, validate its length before we
                // even unlock — matches the login/settings PIN rules.
                if !self.unlock_pin.is_empty() && self.unlock_pin.chars().count() < MIN_PIN_LEN {
                    self.error = Some(fl!("pin-too-short", count = MIN_PIN_LEN));
                    return Some(Task::none());
                }
                let password = self.unlock_password.clone();
                // Remember that this unlock should apply the PIN (re-)enable field.
                // AuthResult is shared with login and PIN-unlock, so gate on this.
                self.unlock_pin_apply_pending = true;
                self.auth_loading = true;
                Some(Task::perform(
                    async move {
                        let agent = AgentClient::new();
                        match agent.send(AgentAction::Unlock { password }).await {
                            Ok(Response::Ack) => Ok(()),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    },
                    |res| Action::App(Message::AuthResult(res)),
                ))
            }
            Message::AuthResult(res) => {
                self.auth_loading = false;
                let apply_unlock_pin = std::mem::take(&mut self.unlock_pin_apply_pending);
                match res {
                    Ok(()) => {
                        self.view = View::Vault;
                        self.error = None;
                        self.pin_incorrect = false;

                        let mut tasks = vec![fetch_sidebar_entries(self.search_id, None, None, false)];

                        // Master-password unlock that showed the PIN (re-)enable
                        // field: reseal the PIN (non-empty) or remove a stale PIN
                        // blob (empty). This is how the user recovers after a TPM
                        // mismatch forced them back to the master password.
                        if apply_unlock_pin {
                            if let Some(task) = self.apply_unlock_pin_task() {
                                tasks.push(task);
                            }
                        }

                        // If the user enabled PIN during login, set it up now.
                        // The vault is freshly unlocked, so we use SetupTpmPinFromUnlocked
                        // (no master password re-entry needed).
                        if self.login_pin_enabled && !self.login_pin.is_empty() {
                            let pin = std::mem::take(&mut self.login_pin);
                            self.login_pin_enabled = false;
                            self.login_pin_revealed = false;
                            tasks.push(Task::perform(
                                async move {
                                    let agent = AgentClient::new();
                                    match agent
                                        .send(AgentAction::SetupTpmPinFromUnlocked { pin })
                                        .await
                                    {
                                        Ok(Response::Ack) => Ok(()),
                                        Ok(Response::Error { message }) => Err(message),
                                        _ => Err("unexpected response".to_string()),
                                    }
                                },
                                |res| Action::App(Message::TpmSetupResult(res)),
                            ));
                        }

                        self.login_password.zeroize();
                        self.unlock_password.zeroize();

                        return Some(Task::batch(tasks));
                    }
                    Err(e) => {
                        if e == "new_device_verification_required" {
                            self.show_verification_input = true;
                            self.error = None;
                            self.view = View::Setup;
                        } else {
                            self.error = Some(e);
                            self.view = if self.config.email.is_some() {
                                View::Unlock
                            } else {
                                View::Setup
                            };
                        }
                        if let Some(err) = &self.error {
                            error!("Auth failed: {}", err);
                        }
                        // A failed PIN unlock consumes a DA attempt — reveal and
                        // refresh the counter shown on the PIN screen.
                        if self.show_pin_unlock && self.tpm_available {
                            self.pin_incorrect = true;
                            return Some(super::lifecycle::check_tpm_da_task());
                        }
                    }
                }
                Some(Task::none())
            }
            Message::LockClicked => Some(Task::perform(
                async {
                    let agent = AgentClient::new();
                    let _ = agent.send(AgentAction::Lock).await;
                },
                |_| Action::App(Message::LockResult),
            )),
            Message::LockResult => {
                self.view = View::Unlock;
                self.entries.clear();
                self.selected_entry_id = None;
                self.selected_entry = None;
                self.editing_entry = None;
                self.revealed_fields.clear();
                self.error = None;
                self.pin_incorrect = false;
                self.main_window_pin.zeroize();
                self.unlock_password.zeroize();
                self.unlock_pin.zeroize();
                self.unlock_pin_revealed = false;
                self.unlock_pin_apply_pending = false;
                self.login_password.zeroize();
                Some(Task::none())
            }
            Message::LogoutClicked => Some(Task::perform(
                async {
                    let agent = AgentClient::new();
                    let _ = agent.send(AgentAction::Logout).await;
                },
                |_| Action::App(Message::LogoutResult),
            )),
            Message::LogoutResult => {
                self.view = View::Setup;
                self.entries.clear();
                self.selected_entry_id = None;
                self.selected_entry = None;
                self.editing_entry = None;
                self.revealed_fields.clear();
                self.error = None;
                self.pin_incorrect = false;
                self.main_window_pin.zeroize();
                self.unlock_pin.zeroize();
                self.unlock_pin_revealed = false;
                self.unlock_pin_apply_pending = false;
                Some(Task::none())
            }
            _ => None,
        }
    }

    /// Build the task that applies the master-password-unlock PIN field: a
    /// non-empty PIN reseals it against the current TPM/PCR state (recovering from
    /// a mismatch); an empty PIN removes an existing (possibly stale) PIN blob.
    /// Consumes `self.unlock_pin`. Returns None when there is nothing to do.
    pub(crate) fn apply_unlock_pin_task(&mut self) -> Option<Task<Message>> {
        if !self.tpm_available {
            self.unlock_pin.zeroize();
            self.unlock_pin.clear();
            self.unlock_pin_revealed = false;
            return None;
        }
        let pin = std::mem::take(&mut self.unlock_pin);
        self.unlock_pin_revealed = false;

        if !pin.is_empty() {
            Some(Task::perform(
                async move {
                    let agent = AgentClient::new();
                    match agent
                        .send(AgentAction::SetupTpmPinFromUnlocked { pin })
                        .await
                    {
                        Ok(Response::Ack) => Ok(()),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                },
                |res| Action::App(Message::TpmSetupResult(res)),
            ))
        } else if self.tpm_configured {
            // Empty PIN + an existing blob → disable PIN unlock (clears a stale
            // blob left over after a TPM/PCR mismatch).
            Some(Task::perform(
                async {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::DisableTpmPin).await {
                        Ok(Response::Ack) => Ok(()),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                },
                |res| Action::App(Message::TpmDisableResult(res)),
            ))
        } else {
            None
        }
    }
}
