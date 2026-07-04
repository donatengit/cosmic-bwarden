use crate::app::applet_search;
use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::{
    check_protocol_version, fetch_applet_search, fetch_applet_secret, fetch_sidebar_entries,
};
use crate::fl;
use crate::message::{Message, View};
use crate::view::applet::{search, unlock};
use crate::MIN_PIN_LEN;
use cosmic::app::Task;
use cosmic::iced::window;
use cosmic::widget::text_input;
use cosmic::widget::Toast;
use cosmic::Action;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response};
use zeroize::Zeroize;

impl CosmicBWardenApp {
    pub fn update_applet(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::AppletIconClicked(offset, bounds) => {
                if let Some(id) = self.applet_popup {
                    return Some(Task::done(cosmic::Action::Cosmic(
                        cosmic::app::Action::Surface(cosmic::surface::action::destroy_popup(id)),
                    )));
                }

                Some(self.open_applet_popup_task(Some((offset, bounds))))
            }
            Message::Surface(action) => Some(Task::done(cosmic::Action::Cosmic(
                cosmic::app::Action::Surface(action),
            ))),
            Message::Exit => {
                let mut tasks = Vec::new();

                if let Some(popup_id) = self.applet_popup.take() {
                    self.windows.remove(&popup_id);
                    tasks.push(Task::done(cosmic::Action::Cosmic(
                        cosmic::app::Action::Surface(cosmic::surface::action::destroy_popup(
                            popup_id,
                        )),
                    )));
                }

                tasks.push(Task::done(cosmic::Action::Cosmic(
                    cosmic::app::Action::Close,
                )));

                Some(Task::batch(tasks))
            }
            Message::LockAndQuit => Some(Task::perform(
                async {
                    let agent = AgentClient::new();
                    let _ = agent.send(AgentAction::Lock).await;
                },
                |_| Action::App(Message::Exit),
            )),
            Message::LogoutAndQuit => Some(Task::perform(
                async {
                    let agent = AgentClient::new();
                    let _ = agent.send(AgentAction::Logout).await;
                },
                |_| Action::App(Message::Exit),
            )),
            Message::OpenVaultRequested => {
                let mut tasks = Vec::new();

                if let Some(popup_id) = self.applet_popup.take() {
                    self.windows.remove(&popup_id);
                    tasks.push(Task::done(cosmic::Action::Cosmic(
                        cosmic::app::Action::Surface(cosmic::surface::action::destroy_popup(
                            popup_id,
                        )),
                    )));
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
                    cosmic::applet::token::subscription::TokenUpdate::ActivationToken {
                        token,
                        ..
                    } => {
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
                if !self.unlock_pin.is_empty() && self.unlock_pin.chars().count() < MIN_PIN_LEN {
                    self.applet_error = Some(fl!("pin-too-short", count = MIN_PIN_LEN));
                    return Some(Task::none());
                }
                let password = self.applet_unlock_password.clone();
                Some(Task::perform(
                    async move {
                        let agent = AgentClient::new();
                        match agent.send(AgentAction::Unlock { password }).await {
                            Ok(Response::Ack) => Ok(()),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    },
                    |res| Action::App(Message::AppletUnlockResult(res)),
                ))
            }
            Message::AppletUnlockResult(res) => {
                self.applet_unlock_password.zeroize();
                match res {
                    Ok(()) => {
                        self.applet_error = None;
                        self.view = View::Vault;
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
                        let mut tasks = vec![fetch_applet_search(
                            self.applet_search_id,
                            query,
                            only_pinned,
                        )];
                        // Apply the master-password-unlock PIN field: reseal against
                        // the current TPM state (non-empty) or clear a stale PIN
                        // blob (empty). AppletUnlockResult is reached only from a
                        // master-password unlock, so no extra guard is needed.
                        if let Some(task) = self.apply_unlock_pin_task() {
                            tasks.push(task);
                        }
                        return Some(Task::batch(tasks));
                    }
                    Err(e) => self.applet_error = Some(e),
                }
                Some(Task::none())
            }

            // Search
            Message::AppletSearchChanged(q) => {
                self.applet_search_query = q.clone();
                self.applet_search_id += 1;
                let only_pinned =
                    applet_search::effective_only_pinned(&q, self.applet_search_only_favourites);
                let query = if q.trim().is_empty() { None } else { Some(q) };
                Some(fetch_applet_search(
                    self.applet_search_id,
                    query,
                    only_pinned,
                ))
            }
            Message::AppletToggleFavouritesFilter => {
                self.applet_search_only_favourites = !self.applet_search_only_favourites;
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
                Some(fetch_applet_search(
                    self.applet_search_id,
                    query,
                    only_pinned,
                ))
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
                if let Some(entry) = self.applet_search_results.iter().find(|e| e.id == id) {
                    if let Some(username) = entry.username.clone() {
                        return Some(self.applet_copy_to_clipboard(username));
                    }
                }
                Some(Task::none())
            }
            Message::AppletCopySecret(id) => Some(fetch_applet_secret(id, None)),
            Message::AppletOpenInVault(id) => Some(Task::perform(
                async move {
                    let agent = AgentClient::new();
                    let _ = agent.send(AgentAction::SetPendingEntry { id }).await;
                },
                |_| Action::App(Message::OpenVaultRequested),
            )),
            Message::AppletOpenLink(uri) => {
                let _ = std::process::Command::new("xdg-open").arg(&uri).spawn();
                Some(Task::none())
            }
            Message::AppletQuitMenuToggle => {
                self.applet_quit_expanded = !self.applet_quit_expanded;
                Some(Task::none())
            }
            Message::AppletSecretReceived(res) => match res {
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
            },

            // Inline reprompt
            Message::AppletRepromptPasswordChanged(p) => {
                self.applet_reprompt_password = p;
                Some(Task::none())
            }
            Message::AppletRepromptSubmitted => {
                if let Some(id) = self.applet_reprompt_id.clone() {
                    Some(fetch_applet_secret(
                        id,
                        Some(self.applet_reprompt_password.clone()),
                    ))
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
            Message::AppletTogglePinReveal => {
                self.applet_pin_revealed = !self.applet_pin_revealed;
                Some(Task::none())
            }

            // Toasts
            Message::CloseToast(id) => {
                self.applet_toasts.remove(id);
                Some(Task::none())
            }

            // TPM / PIN unlock
            Message::AppletPinChanged(p) => {
                self.applet_pin = p;
                Some(Task::none())
            }
            Message::AppletPinSubmitted => {
                let pin = self.applet_pin.clone();
                Some(Task::perform(
                    async move {
                        let agent = AgentClient::new();
                        match agent.send(AgentAction::UnlockWithPin { pin }).await {
                            Ok(Response::Ack) => Ok(()),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    },
                    |res| Action::App(Message::AppletPinResult(res)),
                ))
            }
            Message::AppletPinResult(res) => {
                self.applet_pin.zeroize();
                self.applet_pin_revealed = false;
                match res {
                    Ok(()) => {
                        self.applet_error = None;
                        self.error = None;
                        self.show_pin_unlock = false;
                        self.pin_incorrect = false;
                        self.view = View::Vault;
                        self.search_id += 1;
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
                        Some(Task::batch(vec![
                            fetch_sidebar_entries(self.search_id, None, None, false),
                            fetch_applet_search(self.applet_search_id, query, only_pinned),
                        ]))
                    }
                    Err(e) => {
                        tracing::error!("PIN unlock failed: {}", e);
                        self.view = View::Unlock;
                        if e == cosmic_bwarden_core::protocol::ERR_TPM_UNSEAL_FAILED {
                            // Wrong PIN / changed PCRs / DA lockout: the raw
                            // message is log-only; the incorrect-PIN/attempts
                            // caption is the feedback. A wrong PIN consumed a
                            // DA attempt — reveal and refresh the counter.
                            self.applet_error = None;
                            self.error = None;
                            self.pin_incorrect = true;
                            Some(super::lifecycle::check_tpm_da_task())
                        } else {
                            // Environmental failure (agent/config/account) —
                            // show it, don't mislabel it as a wrong PIN.
                            self.applet_error = Some(e.clone());
                            self.error = Some(e);
                            Some(Task::none())
                        }
                    }
                }
            }
            Message::AppletUseMasterPasswordInstead => {
                self.show_pin_unlock = false;
                self.pin_incorrect = false;
                Some(Task::none())
            }

            // TPM settings
            Message::TpmSetupFormToggle => {
                self.show_tpm_setup_form = !self.show_tpm_setup_form;
                self.tpm_setup_pin.zeroize();
                self.tpm_setup_pin_revealed = false;
                self.tpm_error = None;
                Some(Task::none())
            }
            Message::TpmDisableFormToggle => {
                self.show_tpm_disable_form = !self.show_tpm_disable_form;
                self.tpm_error = None;
                Some(Task::none())
            }
            Message::TpmSetupPinChanged(p) => {
                self.tpm_setup_pin = p;
                Some(Task::none())
            }
            Message::TpmSetupPinRevealToggled => {
                self.tpm_setup_pin_revealed = !self.tpm_setup_pin_revealed;
                Some(Task::none())
            }
            Message::TpmSetupSubmitted => {
                if self.tpm_setup_pin.chars().count() < MIN_PIN_LEN {
                    self.applet_error = Some(fl!("pin-too-short", count = MIN_PIN_LEN));
                    return Some(Task::none());
                }
                let pin = self.tpm_setup_pin.clone();
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
            }
            Message::TpmSetupResult(res) => {
                self.tpm_setup_pin.zeroize();
                self.tpm_setup_pin_revealed = false;
                self.show_tpm_setup_form = false;
                match res {
                    Ok(()) => {
                        self.tpm_configured = true;
                        // Enabling PIN resets all TPM stores server-side, so server
                        // credentials start disabled again — reflect that here.
                        self.tpm_server_credentials = false;
                        self.applet_error = None;
                        self.tpm_error = None;
                        // Refresh the lockout status shown in the settings pane.
                        return Some(super::lifecycle::check_tpm_da_task());
                    }
                    Err(e) => {
                        tracing::error!("TPM PIN setup failed: {}", e);
                        self.tpm_error = Some(e.clone());
                        self.applet_error = Some(e);
                    }
                }
                Some(Task::none())
            }
            Message::TpmDisableSubmitted => Some(Task::perform(
                async {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::DisableTpmPin).await {
                        Ok(Response::Ack) => Ok(()),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                },
                |res| Action::App(Message::TpmDisableResult(res)),
            )),
            Message::TpmDisableResult(res) => {
                self.show_tpm_disable_form = false;
                match res {
                    Ok(()) => {
                        self.tpm_configured = false;
                        self.show_pin_unlock = false;
                        self.applet_error = None;
                        self.tpm_error = None;
                        // Refresh the lockout status shown in the settings pane.
                        return Some(super::lifecycle::check_tpm_da_task());
                    }
                    Err(e) => {
                        tracing::error!("TPM PIN disable failed: {}", e);
                        self.tpm_error = Some(e.clone());
                        self.applet_error = Some(e);
                    }
                }
                Some(Task::none())
            }

            _ => None,
        }
    }

    /// Resets transient popup state and builds the task that opens the
    /// applet popup, focused on the unlock password field.
    ///
    /// `anchor`, when `Some((offset, bounds))`, positions the popup next to
    /// the clicked applet icon (as `AppletIconClicked` does). When `None`
    /// (e.g. opened in response to `Event::UnlockRequested`),
    /// `get_popup_settings`'s default `anchor_rect` is used, which anchors
    /// next to the applet's own panel icon.
    pub(crate) fn open_applet_popup_task(
        &mut self,
        anchor: Option<(cosmic::iced::Vector, cosmic::iced::Rectangle)>,
    ) -> Task<Message> {
        // Reset only truly transient state; preserve search query and
        // favourites-filter so the popup re-opens with the last search intact.
        self.applet_unlock_password.zeroize();
        self.applet_unlock_password_revealed = false;
        self.applet_reprompt_id = None;
        self.applet_reprompt_password.zeroize();
        self.applet_reprompt_password_revealed = false;
        self.applet_error = None;
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

        let mut tasks = Vec::new();
        tasks.push(check_protocol_version());
        tasks.push(text_input::focus(unlock::password_input_id()));
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
            |res| cosmic::Action::App(Message::ConfigReceived(res)),
        ));

        tasks.push(fetch_applet_search(
            self.applet_search_id,
            query,
            only_pinned,
        ));

        let popup_task = Task::done(cosmic::Action::Cosmic(cosmic::app::Action::Surface(
            cosmic::surface::action::app_popup::<CosmicBWardenApp>(
                |_| Default::default(),
                move |state: &mut CosmicBWardenApp| {
                    let new_id = window::Id::unique();
                    state.applet_popup = Some(new_id);
                    state
                        .windows
                        .insert(new_id, crate::message::WindowState::Popup);
                    let mut popup_settings = state.core.applet.get_popup_settings(
                        state.core.main_window_id().unwrap_or(window::Id::RESERVED),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    if let Some((offset, bounds)) = anchor {
                        popup_settings.positioner.anchor_rect = cosmic::iced::Rectangle {
                            x: (bounds.x - offset.x) as i32,
                            y: (bounds.y - offset.y) as i32,
                            width: bounds.width as i32,
                            height: bounds.height as i32,
                        };
                    }
                    popup_settings
                },
                None,
            ),
        )));
        tasks.push(popup_task);
        Task::batch(tasks)
    }

    fn applet_copy_to_clipboard(&mut self, value: String) -> Task<Message> {
        let clipboard_task =
            cosmic::iced::clipboard::write(value).map(|_: ()| cosmic::Action::None);
        let toast_task = self
            .applet_toasts
            .push(Toast::new(fl!("copied-to-clipboard")))
            .map(cosmic::Action::App);
        Task::batch(vec![clipboard_task, toast_task])
    }
}
