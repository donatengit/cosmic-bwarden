use crate::app::applet_search;
use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::{fetch_applet_search, fetch_sidebar_entries};
use crate::message::{Message, View};
use cosmic::app::Task;
use cosmic::Action;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response};
use tracing::debug;

pub(super) fn check_tpm_task() -> Task<Message> {
    Task::perform(
        async {
            let agent = AgentClient::new();
            match agent.send(AgentAction::CheckTpm).await {
                Ok(Response::TpmStatus { available, configured, server_credentials }) => {
                    Ok((available, configured, server_credentials))
                }
                Ok(Response::Error { message }) => Err(message),
                _ => Err("unexpected response to CheckTpm".to_string()),
            }
        },
        |res| Action::App(Message::TpmStatusReceived(res)),
    )
}

impl CosmicBWardenApp {
    pub fn update_lifecycle(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::ConfigReceived(res) => {
                match res {
                    Ok((config, _needs_login, has_account, is_locked, sync_failed)) => {
                        self.config = config;
                        self.sync_failed = sync_failed;
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
                            // Fetch all entries on initial vault open to populate the
                            // local cache. Local filtering handles search from here on.
                            self.search_id += 1;
                            let mut tasks = vec![
                                fetch_sidebar_entries(self.search_id, None, None, false),
                                check_tpm_task(),
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
                Some(check_tpm_task())
            }
            Message::EventReceived(event) => {
                match event {
                    cosmic_bwarden_core::protocol::Event::Locked => {
                        self.view = View::Unlock;
                        self.selected_entry = None;
                        self.editing_entry = None;
                        self.selected_entry_id = None;
                        self.entries.clear();
                        self.all_entries.clear();
                        self.error = None;
                        // Re-query config: if this was a logout rather than a
                        // plain lock, ConfigReceived will set view=Setup.
                        return Some(Task::perform(
                            async {
                                let agent = AgentClient::new();
                                match agent.send(AgentAction::GetConfig).await {
                                    Ok(Response::Config { config, needs_login, has_account, is_locked, sync_failed }) =>
                                        Ok((config, needs_login, has_account, is_locked, sync_failed)),
                                    Ok(Response::Error { message }) => Err(message),
                                    _ => Err("unexpected response".to_string()),
                                }
                            },
                            |res| Action::App(Message::ConfigReceived(res)),
                        ));
                    }
                    cosmic_bwarden_core::protocol::Event::PinRequested => {
                        self.show_pin_unlock = true;
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
                    cosmic_bwarden_core::protocol::Event::UnlockRequested => {
                        self.show_pin_unlock = false;
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
            Message::TpmStatusReceived(res) => {
                match res {
                    Ok((available, configured, server_credentials)) => {
                        self.tpm_status_known = true;
                        self.tpm_available = available;
                        self.tpm_configured = configured;
                        self.tpm_server_credentials = server_credentials;
                        if configured {
                            self.show_pin_unlock = true;
                        }
                        if !available {
                            // Fetch diagnostics so the settings view can explain why.
                            return Some(Task::perform(
                                async {
                                    let agent = AgentClient::new();
                                    match agent.send(AgentAction::CheckTpmDiagnostics).await {
                                        Ok(Response::TpmDiagnostics { checks }) => checks,
                                        _ => vec![],
                                    }
                                },
                                |checks| Action::App(Message::TpmDiagnosticsReceived(checks)),
                            ));
                        }
                    }
                    Err(e) => {
                        tracing::warn!("TPM status check failed: {}", e);
                    }
                }
                return Some(Task::none());
            }
            Message::TpmDiagnosticsReceived(checks) => {
                self.tpm_diagnostics = checks;
                return Some(Task::none());
            }
            Message::TpmServerCredentialsToggled(on) => {
                self.tpm_error = None;
                let action = if on {
                    AgentAction::EnableTpmServerCredentials
                } else {
                    AgentAction::DisableTpmServerCredentials
                };
                return Some(Task::perform(
                    async move {
                        let agent = AgentClient::new();
                        match agent.send(action).await {
                            Ok(Response::Ack) => Ok(()),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    },
                    |res| Action::App(Message::TpmServerCredentialsResult(res)),
                ));
            }
            Message::TpmServerCredentialsResult(res) => {
                match res {
                    Ok(()) => {
                        self.tpm_error = None;
                        return Some(check_tpm_task());
                    }
                    Err(e) => {
                        tracing::error!("TPM server credentials toggle failed: {}", e);
                        self.tpm_error = Some(e);
                    }
                }
                return Some(Task::none());
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
                                sync_failed,
                            }) => Ok((config, needs_login, has_account, is_locked, sync_failed)),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    },
                    |res| Action::App(Message::ConfigReceived(res)),
                ));

                if !matches!(self.view, View::Loading | View::Setup | View::Unlock) {
                    self.search_id += 1;
                    tasks.push(fetch_sidebar_entries(self.search_id, None, None, false));

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
