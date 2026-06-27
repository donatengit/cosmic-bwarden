use crate::app::applet_search;
use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::{fetch_applet_search, fetch_sidebar_entries, fetch_top_entries};
use crate::message::{Message, View};
use cosmic::app::Task;
use cosmic::Action;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response};
use tracing::debug;

impl CosmicBWardenApp {
    pub fn update_lifecycle(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::ConfigReceived(res) => {
                match res {
                    Ok((config, _needs_login, has_account, is_locked)) => {
                        self.config = config;
                        if !has_account {
                            self.view = View::Setup;
                            if let Some(email) = &self.config.email {
                                self.login_email = email.clone();
                            }
                            if let Some(server) = &self.config.base_url {
                                self.login_server = server.clone();
                            }
                        } else if is_locked {
                            self.view = View::Unlock;
                            if let Some(email) = &self.config.email {
                                self.login_email = email.clone();
                            }
                            if let Some(server) = &self.config.base_url {
                                self.login_server = server.clone();
                            }
                        } else {
                            self.view = View::Vault;
                            // Fetch entries on initial vault open (startup while already
                            // unlocked). RefreshStateInternal handles the Unlocked-event
                            // path; this covers the direct-startup path.
                            self.search_id += 1;
                            let mut tasks = vec![
                                fetch_sidebar_entries(self.search_id, None, self.filter_type.clone(), self.search_only_pinned),
                                fetch_top_entries(self.config.top_popular_count as usize, Some(self.config.top_popular_days)),
                            ];
                            if let Some(id) = self.pending_vault_entry.take() {
                                tasks.push(Task::done(Action::App(Message::SelectEntry(id))));
                            }
                            return Some(Task::batch(tasks));
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
                        self.entries.clear();
                        self.error = None;
                        // Re-query config: if this was a logout rather than a
                        // plain lock, ConfigReceived will set view=Setup.
                        return Some(Task::perform(
                            async {
                                let agent = AgentClient::new();
                                match agent.send(AgentAction::GetConfig).await {
                                    Ok(Response::Config { config, needs_login, has_account, is_locked }) =>
                                        Ok((config, needs_login, has_account, is_locked)),
                                    Ok(Response::Error { message }) => Err(message),
                                    _ => Err("unexpected response".to_string()),
                                }
                            },
                            |res| Action::App(Message::ConfigReceived(res)),
                        ));
                    }
                    cosmic_bwarden_core::protocol::Event::UnlockRequested => {
                        self.view = View::Unlock;
                        self.selected_entry = None;
                        self.editing_entry = None;
                        self.selected_entry_id = None;
                        if crate::detect_run_mode() == crate::RunMode::Applet
                            && self.applet_popup.is_none()
                        {
                            return Some(self.open_applet_popup_task(None));
                        }
                    }
                    cosmic_bwarden_core::protocol::Event::Unlocked => {
                        self.view = View::Vault;
                        return Some(Task::perform(async {}, |_| {
                            Action::App(Message::RefreshStateInternal)
                        }));
                    }
                    cosmic_bwarden_core::protocol::Event::VaultChanged => {
                        return Some(Task::perform(async {}, |_| {
                            Action::App(Message::RefreshStateInternal)
                        }));
                    }
                    cosmic_bwarden_core::protocol::Event::OpenEntry { id } => {
                        if matches!(self.view, View::Vault) {
                            return Some(Task::done(Action::App(Message::SelectEntry(id))));
                        } else {
                            // View not ready yet (Loading → ConfigReceived not processed).
                            // Store and apply when the vault view becomes active.
                            self.pending_vault_entry = Some(id);
                        }
                    }
                }
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
            Message::ProtocolVersionCheck(res) => {
                match res {
                    Ok(mismatch) => {
                        self.protocol_mismatch = mismatch;
                    }
                    Err(e) => {
                        tracing::error!("Version check failed: {}", e);
                        // If we can't reach the agent, don't block — let the
                        // normal error handling surface the issue.
                    }
                }
                Some(Task::none())
            }
            Message::RefreshStateInternal => {
                let mut tasks = Vec::new();
                tasks.push(Task::perform(
                    async {
                        let agent = AgentClient::new();
                        match agent.send(AgentAction::GetConfig).await {
                            Ok(Response::Config {
                                config,
                                needs_login,
                                has_account,
                                is_locked,
                            }) => Ok((config, needs_login, has_account, is_locked)),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    },
                    |res| Action::App(Message::ConfigReceived(res)),
                ));

                if !matches!(self.view, View::Loading | View::Setup | View::Unlock) {
                    self.search_id += 1;
                    tasks.push(fetch_sidebar_entries(
                        self.search_id,
                        Some(self.search_query.clone()),
                        self.filter_type.clone(),
                        self.search_only_pinned,
                    ));
                    tasks.push(fetch_top_entries(
                        self.config.top_popular_count as usize,
                        Some(self.config.top_popular_days),
                    ));

                    if self.applet_popup.is_some() {
                        self.applet_search_id += 1;
                        let only_pinned = applet_search::effective_only_pinned(
                            &self.applet_search_query,
                            self.applet_search_only_favourites,
                        );
                        let query = if self.applet_search_query.trim().is_empty() {
                            None
                        } else {
                            Some(self.applet_search_query.clone())
                        };
                        tasks.push(fetch_applet_search(
                            self.applet_search_id,
                            query,
                            only_pinned,
                        ));
                    }
                }

                Some(Task::batch(tasks))
            }
            _ => None,
        }
    }
}
