use cosmic::app::Task;
use cosmic::iced::window;
use cosmic::widget::text_input;
use cosmic::Action;
use cosmic::widget::Toast;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response};
use cosmic_bwarden_core::agent_client::AgentClient;
use crate::message::{Message, View};
use crate::app::state::CosmicBWardenApp;
use crate::app::applet_search;
use crate::app::tasks::{fetch_applet_search, fetch_applet_secret};
use crate::view::applet::{search, unlock};
use crate::fl;
use zeroize::Zeroize;

impl CosmicBWardenApp {
    pub fn update_applet(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::AppletIconClicked(offset, bounds) => {
                if let Some(id) = self.applet_popup {
                    return Some(Task::done(cosmic::Action::Cosmic(cosmic::app::Action::Surface(cosmic::surface::action::destroy_popup(id)))));
                }

                // Reset transient popup state for a fresh open.
                self.applet_search_query.clear();
                self.applet_search_only_favourites = false;
                self.applet_unlock_password.zeroize();
                self.applet_unlock_password_revealed = false;
                self.applet_reprompt_id = None;
                self.applet_reprompt_password.zeroize();
                self.applet_reprompt_password_revealed = false;
                self.applet_error = None;
                self.applet_search_id += 1;

                let mut tasks = Vec::new();
                tasks.push(text_input::focus(unlock::password_input_id()));
                tasks.push(Task::perform(async {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::GetConfig).await {
                        Ok(Response::Config { config, needs_login, has_account, is_locked }) => Ok((config, needs_login, has_account, is_locked)),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                }, |res| cosmic::Action::App(Message::ConfigReceived(res))));

                tasks.push(fetch_applet_search(self.applet_search_id, None, true));

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
                Some(Task::batch(tasks))
            }
            Message::CopyPassword(id) => {
                Some(Task::perform(async move {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::GetPassword { id, password: None }).await {
                        Ok(Response::Password { password }) => Ok(password),
                        _ => Err("failed to get password".to_string()),
                    }
                }, |res| cosmic::Action::App(res)).then(|res| match res {
                    cosmic::Action::App(Ok(p)) => cosmic::iced::clipboard::write(p).map(|_: ()| cosmic::Action::None),
                    _ => Task::done(cosmic::Action::None),
                }))
            }
            Message::PopupClosed(id) => {
                if self.applet_popup == Some(id) {
                    self.applet_popup = None;
                }
                self.windows.remove(&id);
                Some(Task::none())
            }
            Message::Surface(action) => Some(Task::done(cosmic::Action::Cosmic(cosmic::app::Action::Surface(action)))),
            Message::Exit => {
                std::process::exit(0)
            }
            Message::OpenVaultRequested => {
                let mut tasks = Vec::new();

                if let Some(popup_id) = self.applet_popup.take() {
                    self.windows.remove(&popup_id);
                    tasks.push(Task::done(cosmic::Action::Cosmic(cosmic::app::Action::Surface(cosmic::surface::action::destroy_popup(popup_id)))));
                }

                if let Some(tx) = self.token_tx.as_ref() {
                    let _ = tx.send(cosmic::applet::token::subscription::TokenRequest {
                        app_id: crate::app::state::APP_ID.to_string(),
                        exec: "open-vault".to_string(),
                    });
                }

                Some(Task::batch(tasks))
            }
            Message::Token(update) => {
                match update {
                    cosmic::applet::token::subscription::TokenUpdate::Init(tx) => {
                        self.token_tx = Some(tx);
                    }
                    cosmic::applet::token::subscription::TokenUpdate::Finished => {
                        self.token_tx = None;
                    }
                    cosmic::applet::token::subscription::TokenUpdate::ActivationToken { token, .. } => {
                        if let Ok(exe) = std::env::current_exe() {
                            let mut cmd = std::process::Command::new(exe);
                            cmd.env("COSMIC_BWARDEN_MODE", "application");
                            cmd.env_remove("COSMIC_PANEL_NAME");
                            if let Some(token) = token {
                                cmd.env("XDG_ACTIVATION_TOKEN", &token);
                                cmd.env("DESKTOP_STARTUP_ID", &token);
                            }
                            tokio::spawn(cosmic::process::spawn(cmd));
                        }
                    }
                }
                Some(Task::none())
            }

            // Inline unlock
            Message::AppletUnlockPasswordChanged(p) => {
                self.applet_unlock_password = p;
                Some(Task::none())
            }
            Message::AppletUnlockSubmitted => {
                let password = self.applet_unlock_password.clone();
                Some(Task::perform(async move {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::Unlock { password }).await {
                        Ok(Response::Ack) => Ok(()),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                }, |res| Action::App(Message::AppletUnlockResult(res))))
            }
            Message::AppletUnlockResult(res) => {
                self.applet_unlock_password.zeroize();
                match res {
                    Ok(()) => {
                        self.applet_error = None;
                        self.view = View::Vault;
                        self.applet_search_id += 1;
                        let only_pinned = applet_search::effective_only_pinned(&self.applet_search_query, self.applet_search_only_favourites);
                        let query = if self.applet_search_query.trim().is_empty() { None } else { Some(self.applet_search_query.clone()) };
                        return Some(fetch_applet_search(self.applet_search_id, query, only_pinned));
                    }
                    Err(e) => self.applet_error = Some(e),
                }
                Some(Task::none())
            }

            // Search
            Message::AppletSearchChanged(q) => {
                self.applet_search_query = q.clone();
                self.applet_search_id += 1;
                let only_pinned = applet_search::effective_only_pinned(&q, self.applet_search_only_favourites);
                let query = if q.trim().is_empty() { None } else { Some(q) };
                Some(fetch_applet_search(self.applet_search_id, query, only_pinned))
            }
            Message::AppletToggleFavouritesFilter => {
                self.applet_search_only_favourites = !self.applet_search_only_favourites;
                self.applet_search_id += 1;
                let only_pinned = applet_search::effective_only_pinned(&self.applet_search_query, self.applet_search_only_favourites);
                let query = if self.applet_search_query.trim().is_empty() {
                    None
                } else {
                    Some(self.applet_search_query.clone())
                };
                Some(fetch_applet_search(self.applet_search_id, query, only_pinned))
            }
            Message::AppletSearchResultsReceived(id, res) => {
                if id == self.applet_search_id {
                    match res {
                        Ok(entries) => {
                            self.applet_search_results = entries;
                            self.applet_error = None;
                        }
                        // Expected while locked; the unlock UI is already shown.
                        Err(e) if e == "agent is locked" => {}
                        Err(e) => self.applet_error = Some(e),
                    }
                }
                Some(Task::none())
            }

            // Copy actions
            Message::AppletCopyPrimary(id) => {
                let rows = applet_search::build_applet_rows(&self.applet_search_results);
                if let Some(value) = rows.iter().find(|r| r.id == id).and_then(|r| r.primary_value.clone()) {
                    Some(self.applet_copy_to_clipboard(value))
                } else {
                    Some(Task::none())
                }
            }
            Message::AppletCopySecret(id) => Some(fetch_applet_secret(id, None)),
            Message::AppletSecretReceived(res) => {
                match res {
                    Ok(secret) => {
                        self.applet_reprompt_id = None;
                        self.applet_reprompt_password.zeroize();
                        self.applet_error = None;
                        Some(self.applet_copy_to_clipboard(secret))
                    }
                    Err((id, msg)) => {
                        if msg == "reprompt_required" {
                            self.applet_reprompt_id = Some(id);
                            self.applet_reprompt_password.zeroize();
                            return Some(text_input::focus(search::reprompt_input_id()));
                        } else {
                            self.applet_error = Some(msg);
                        }
                        Some(Task::none())
                    }
                }
            }

            // Inline reprompt
            Message::AppletRepromptPasswordChanged(p) => {
                self.applet_reprompt_password = p;
                Some(Task::none())
            }
            Message::AppletRepromptSubmitted => {
                if let Some(id) = self.applet_reprompt_id.clone() {
                    Some(fetch_applet_secret(id, Some(self.applet_reprompt_password.clone())))
                } else {
                    Some(Task::none())
                }
            }
            Message::AppletRepromptCancelled => {
                self.applet_reprompt_id = None;
                self.applet_reprompt_password.zeroize();
                Some(Task::none())
            }

            // Password reveal toggles
            Message::AppletToggleUnlockPasswordReveal => {
                self.applet_unlock_password_revealed = !self.applet_unlock_password_revealed;
                Some(Task::none())
            }
            Message::AppletToggleRepromptPasswordReveal => {
                self.applet_reprompt_password_revealed = !self.applet_reprompt_password_revealed;
                Some(Task::none())
            }

            // Toasts
            Message::CloseToast(id) => {
                self.applet_toasts.remove(id);
                Some(Task::none())
            }

            _ => None,
        }
    }

    fn applet_copy_to_clipboard(&mut self, value: String) -> Task<Message> {
        let clipboard_task = cosmic::iced::clipboard::write(value).map(|_: ()| cosmic::Action::None);
        let toast_task = self
            .applet_toasts
            .push(Toast::new(fl!("copied-to-clipboard")))
            .map(cosmic::Action::App);
        Task::batch(vec![clipboard_task, toast_task])
    }
}
