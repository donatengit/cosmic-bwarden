use cosmic::app::Task;
use cosmic::iced::window;
use cosmic::Action;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response};
use cosmic_bwarden_core::agent_client::AgentClient;
use crate::message::{Message, View, WindowState};
use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::{fetch_sidebar_entries, fetch_top_entries};
use crate::fl;
use tracing::{error};

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
            Message::LoginSubmitted => {
                let email = self.login_email.clone();
                let password = self.login_password.clone();
                let server_url = if self.login_server.trim().is_empty() { None } else { Some(self.login_server.clone()) };
                let remember_me = self.login_remember;
                let device_verification_code = if self.login_verification_code.is_empty() { None } else { Some(self.login_verification_code.clone()) };
                
                self.view = View::Loading;
                Some(Task::perform(async move {
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
                }, |res| Action::App(Message::AuthResult(res))))
            }
            Message::UnlockPasswordChanged(p) => {
                self.unlock_password = p;
                Some(Task::none())
            }
            Message::UnlockSubmitted => {
                let password = self.unlock_password.clone();
                self.view = View::Loading;
                Some(Task::perform(async move {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::Unlock { password }).await {
                        Ok(Response::Ack) => Ok(()),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                }, |res| Action::App(Message::AuthResult(res))))
            }
            Message::AuthResult(res) => {
                match res {
                    Ok(()) => {
                        self.view = View::Vault;
                        self.error = None;
                        self.login_password = String::new();
                        self.unlock_password = String::new();
                        
                        let mut tasks = vec![
                           fetch_sidebar_entries(self.search_id, None, None, false),
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
                        return Some(Task::batch(tasks));
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
                Some(Task::none())
            }
            Message::LockClicked => {
                Some(Task::perform(async {
                    let agent = AgentClient::new();
                    let _ = agent.send(AgentAction::Lock).await;
                    ()
                }, |_| Action::App(Message::LockResult)))
            }
            Message::LockResult => {
                self.view = View::Unlock;
                self.entries.clear();
                self.top_entries.clear();
                self.selected_entry_id = None;
                self.selected_entry = None;
                self.editing_entry = None;
                self.revealed_fields.clear();
                Some(Task::none())
            }
            Message::LogoutClicked => {
                Some(Task::perform(async {
                    let agent = AgentClient::new();
                    let _ = agent.send(AgentAction::Logout).await;
                    ()
                }, |_| Action::App(Message::LogoutResult)))
            }
            Message::LogoutResult => {
                self.view = View::Setup;
                self.entries.clear();
                self.top_entries.clear();
                self.selected_entry_id = None;
                self.selected_entry = None;
                self.editing_entry = None;
                self.revealed_fields.clear();
                Some(Task::none())
            }
            _ => None,
        }
    }
}
