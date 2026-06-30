use cosmic::app::Task;
use cosmic::Action;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response};
use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::fetch_sidebar_entries;
use crate::message::{Message, View};
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
                if self.login_pin_enabled && !self.login_pin.is_empty() && self.login_pin.len() < 6 {
                    self.error = Some("PIN must be at least 6 characters".to_string());
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
            Message::UnlockSubmitted => {
                let password = self.unlock_password.clone();
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
                match res {
                    Ok(()) => {
                        self.view = View::Vault;
                        self.error = None;

                        let mut tasks = vec![fetch_sidebar_entries(self.search_id, None, None, false)];

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
                self.main_window_pin.zeroize();
                self.unlock_password.zeroize();
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
                self.main_window_pin.zeroize();
                Some(Task::none())
            }
            _ => None,
        }
    }
}
