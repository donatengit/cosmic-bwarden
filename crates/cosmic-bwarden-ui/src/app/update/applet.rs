use crate::app::applet_search;
use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::{fetch_applet_search, fetch_applet_secret};
use crate::app::update::{auth_actions, generator_actions};
use crate::fl;
use crate::message::{Message, UnlockMode, View};
use crate::view::applet::search;
use crate::MIN_PIN_LEN;
use cosmic::app::Task;
use cosmic::widget::text_input;
use cosmic::Action;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response};
use zeroize::Zeroize;

mod helpers;
mod popup;

// The message arms below call this one directly. The remaining helpers are
// reached only by `app/tests/applet.rs`, through the path they had before the
// split (`crate::app::update::applet::{…}`), so they are re-exported under
// `cfg(test)`: a plain `pub(crate) use` of items no non-test code calls trips
// `unused_imports`, which the workspace denies.
pub(crate) use helpers::should_suppress_popup_reopen;
#[cfg(test)]
pub(crate) use helpers::{clamped_anchor_rect, unlock_focus_id, APPLET_POPUP_REOPEN_DEBOUNCE_MS};

impl CosmicBWardenApp {
    pub fn update_applet(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::AppletIconClicked(offset, bounds) => {
                // `take()` eagerly instead of waiting for `WindowClosed`: if
                // the compositor dismissed the popup without a close event
                // reaching us, a stale `Some(id)` here would swallow every
                // subsequent click (each one "closing" a dead popup).
                if let Some(id) = self.applet_popup.take() {
                    self.windows.remove(&id);
                    tracing::info!(?id, "applet icon clicked: closing popup");
                    // Record the close so a follow-up event in the same
                    // physical press cannot immediately re-open the popup.
                    self.applet_last_popup_closed_at = Some(std::time::Instant::now());
                    return Some(Task::done(cosmic::Action::Surface(
                        cosmic::surface::action::destroy_popup(id),
                    )));
                }

                // Same-press reopen suppression: the popup was just closed and
                // this event is likely the remainder of that same press. Swallow
                // it once so the popup stays closed instead of flickering back
                // open. The timestamp is cleared on the way out so the *next*
                // deliberate click (after the window elapses) opens normally.
                if should_suppress_popup_reopen(
                    self.applet_last_popup_closed_at,
                    std::time::Instant::now(),
                ) {
                    tracing::info!("applet icon clicked: suppressing same-press popup reopen");
                    self.applet_last_popup_closed_at = None;
                    return Some(Task::none());
                }

                tracing::info!("applet icon clicked: opening popup");
                self.applet_last_popup_closed_at = None;
                Some(self.open_applet_popup_task((offset, bounds)))
            }
            // Surface actions come back as our own messages after the applet
            // popup is created (`app_popup` in `open_applet_popup_task`).
            // Forward them untouched: libcosmic intercepts `Action::Surface`
            // in its own update loop (see the surface module docs), and the
            // variant is typed on our message, so it cannot be handled here.
            Message::Surface(action) => Some(Task::done(cosmic::Action::Surface(action))),
            Message::Exit => {
                let mut tasks = Vec::new();

                if let Some(popup_id) = self.applet_popup.take() {
                    self.windows.remove(&popup_id);
                    tasks.push(Task::done(cosmic::Action::Surface(
                        cosmic::surface::action::destroy_popup(popup_id),
                    )));
                }

                tasks.push(Task::done(cosmic::Action::Cosmic(
                    cosmic::app::Action::Close,
                )));

                Some(Task::batch(tasks))
            }
            Message::OpenVaultRequested => {
                let mut tasks = Vec::new();

                if let Some(popup_id) = self.applet_popup.take() {
                    self.windows.remove(&popup_id);
                    tasks.push(Task::done(cosmic::Action::Surface(
                        cosmic::surface::action::destroy_popup(popup_id),
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
                let task = match update {
                    cosmic::applet::token::subscription::TokenUpdate::Init(tx) => {
                        self.token_tx = Some(tx);
                        Task::none()
                    }
                    cosmic::applet::token::subscription::TokenUpdate::Finished => {
                        self.token_tx = None;
                        Task::none()
                    }
                    cosmic::applet::token::subscription::TokenUpdate::ActivationToken {
                        token,
                        exec,
                    } => {
                        use crate::app::update::activation::ActivationAction;
                        use cosmic::app::Application as _;
                        match crate::app::update::activation::classify_activation(&exec) {
                            ActivationAction::SpawnVault => {
                                crate::app::update::activation::spawn_vault_app(token)
                            }
                            // Panel activation of applet quick actions runs
                            // the same handlers as the popup's icon buttons.
                            ActivationAction::Lock => self.update(Message::LockClicked),
                            ActivationAction::Generate => {
                                self.update(Message::AppletGeneratePasswordRequested)
                            }
                            ActivationAction::Ignore => Task::none(),
                        }
                    }
                };
                Some(task)
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
                        match agent.send(auth_actions::unlock(password)).await {
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
                // Detached double-fork via activation::open_link, not a bare
                // Command::spawn: the dropped-Child call this replaces left a
                // zombie xdg-open child of the applet for the whole panel
                // session once it exited.
                Some(crate::app::update::activation::open_link(uri))
            }
            Message::AppletSearchRowHoverChanged(id, hovered) => {
                if hovered {
                    self.applet_hovered_row_id = Some(id);
                } else if self.applet_hovered_row_id.as_deref() == Some(id.as_str()) {
                    self.applet_hovered_row_id = None;
                }
                Some(Task::none())
            }
            Message::AppletSecretReceived(res) => match res {
                Ok(secret) => {
                    self.applet_reprompt_id = None;
                    self.applet_reprompt_password.zeroize();
                    self.applet_error = None;
                    Some(self.applet_copy_to_clipboard(secret.expose().to_string()))
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
            Message::AppletGeneratePasswordRequested => Some(Task::perform(
                async move {
                    let agent = AgentClient::new();
                    // `settings: None` reuses whatever was last saved by any
                    // surface (desktop pane, CLI, browser extension) — the
                    // applet quick-gen entry never carries its own settings.
                    match agent.send(generator_actions::generate_with_stored()).await {
                        Ok(Response::GeneratedPassword { password }) => Ok(password),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                },
                |res| Action::App(Message::AppletGeneratePasswordReceived(res)),
            )),
            Message::AppletGeneratePasswordReceived(res) => match res {
                Ok(pw) => Some(self.applet_copy_to_clipboard(pw.expose().to_string())),
                Err(e) => {
                    self.applet_error = Some(e);
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
                        match agent.send(auth_actions::unlock_with_pin(pin)).await {
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
                        self.pin_incorrect = false;
                        self.view = View::Vault;
                        // Refresh agent state directly (epoch-stamped config)
                        // so a stale "still locked" response cannot bounce the
                        // popup back to the unlock view after a successful PIN.
                        Some(Task::perform(async {}, |_| {
                            Action::App(Message::RefreshStateInternal)
                        }))
                    }
                    Err(e) => {
                        tracing::error!("PIN unlock failed: {}", e);
                        self.view = View::Unlock;
                        if e == cosmic_bwarden_core::protocol::ERR_TPM_UNSEAL_FAILED {
                            // Wrong PIN / DA lockout: the raw message is
                            // log-only; the incorrect-PIN/attempts caption is
                            // the feedback. A wrong PIN consumed a DA attempt
                            // — reveal and refresh the counter.
                            self.applet_error = None;
                            self.error = None;
                            self.pin_incorrect = true;
                            Some(super::lifecycle::check_tpm_da_task())
                        } else if e == cosmic_bwarden_core::protocol::ERR_TPM_STATE_CHANGED {
                            // PCR state changed (BIOS/firmware update): the
                            // PIN is fine. Never call this a wrong PIN — show
                            // the recovery path.
                            self.applet_error = None;
                            self.error = Some(fl!("tpm-state-changed"));
                            self.pin_incorrect = false;
                            self.unlock_mode = UnlockMode::Password;
                            Some(Task::none())
                        } else if e == cosmic_bwarden_core::protocol::ERR_TPM_BLOB_MISSING {
                            self.applet_error = None;
                            self.error = Some(fl!("tpm-blob-missing"));
                            self.pin_incorrect = false;
                            self.unlock_mode = UnlockMode::Password;
                            Some(Task::none())
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
                self.unlock_mode = UnlockMode::Password;
                self.password_preferred = true;
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
                        match agent.send(auth_actions::setup_tpm_pin(pin)).await {
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
                    match agent.send(auth_actions::disable_tpm_pin()).await {
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
                        self.unlock_mode = UnlockMode::Password;
                        self.password_preferred = true;
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
}
