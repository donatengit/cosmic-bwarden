use cosmic::app::Task;
use cosmic::iced::window;
use cosmic::Action;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response};
use cosmic_bwarden_core::agent_client::AgentClient;
use crate::message::{Message, View, WindowState};
use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::{fetch_sidebar_entries, fetch_top_entries};
use crate::fl;
use tracing::{debug};

impl CosmicBWardenApp {
    pub fn update_lifecycle(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::ConfigReceived(res) => {
                match res {
                    Ok((config, _needs_login, has_account, is_locked)) => {
                        self.config = config;
                        if !has_account {
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
                Some(Task::none())
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
                        return Some(Task::perform(async {}, |_| Action::App(Message::RefreshStateInternal)));
                    }
                    cosmic_bwarden_core::protocol::Event::VaultChanged => {
                        return Some(Task::perform(async {}, |_| Action::App(Message::RefreshStateInternal)));
                    }
                }
                Some(Task::none())
            }
            Message::WindowOpened(id) => {
                debug!("Window opened: {:?}", id);
                Some(Task::none())
            }
            Message::WindowClosed(id) => {
                debug!("Window closed: {:?}", id);
                if self.applet_popup == Some(id) {
                    self.applet_popup = None;
                }
                self.windows.remove(&id);
                Some(Task::none())
            }
            Message::OpenMainWindow => {
                let mut tasks = Vec::new();
                
                // In standalone mode, the primary window is already the main window
                if std::env::var("COSMIC_PANEL_NAME").is_err() {
                    if let Some(id) = self.core.main_window_id() {
                        tasks.push(window::gain_focus(id).map(move |_: ()| Action::App(Message::WindowOpened(id))));
                    }
                    return Some(Task::batch(tasks));
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
                Some(Task::batch(tasks))
            }
            Message::RefreshStateInternal => {
                let mut tasks = Vec::new();
                tasks.push(Task::perform(async {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::GetConfig).await {
                        Ok(Response::Config { config, needs_login, has_account, is_locked }) => Ok((config, needs_login, has_account, is_locked)),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                }, |res| Action::App(Message::ConfigReceived(res))));

                if !matches!(self.view, View::Loading | View::Setup | View::Unlock) {
                    self.search_id += 1;
                    tasks.push(fetch_sidebar_entries(self.search_id, Some(self.search_query.clone()), self.filter_type.clone(), self.search_only_pinned));
                    tasks.push(fetch_top_entries(self.config.top_popular_count as usize, Some(self.config.top_popular_days)));
                }

                Some(Task::batch(tasks))
            }
            _ => None,
        }
    }
}
