use crate::app::applet_search;
use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::{check_protocol_version, fetch_applet_search, fetch_applet_secret};
use crate::app::update::{auth_actions, generator_actions};
use crate::fl;
use crate::message::{Message, UnlockMode, View};
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

/// Debounce window for the applet popup reopen race, in milliseconds. After an
/// icon click closes the popup, a second click within this window is treated as
/// part of the same physical press (Wayland can deliver a close and then a
/// follow-up event that sees the popup gone and would re-open it) and is
/// suppressed. Mirrors `cosmic-ext-applet-external-monitor-brightness`'s
/// 200 ms `last_quit` gate.
pub(crate) const APPLET_POPUP_REOPEN_DEBOUNCE_MS: u128 = 200;

/// True when a popup reopen should be suppressed because the popup was just
/// closed within [`APPLET_POPUP_REOPEN_DEBOUNCE_MS`]. Pure so the toggle
/// race logic is unit-testable without a live executor.
pub(crate) fn should_suppress_popup_reopen(
    last_closed_at: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    last_closed_at
        .map(|t| now.duration_since(t).as_millis() < APPLET_POPUP_REOPEN_DEBOUNCE_MS)
        .unwrap_or(false)
}

/// Compute the popup's `anchor_rect` from the icon-click `(offset, bounds)` and
/// clamp every side to at least 1px (as `cosmic-applet-time` does). A 0-width or
/// negative anchor from degenerate icon bounds would produce a zero-sized
/// positioning rect and a misplaced/never-mapped popup. `positioner.anchor_rect`
/// is `Rectangle<i32>`; `bounds`/`offset` are in logical floats. Pure so the
/// clamp is unit-testable without a live executor/popup surface.
pub(crate) fn clamped_anchor_rect(
    offset: cosmic::iced::Vector,
    bounds: cosmic::iced::Rectangle,
) -> cosmic::iced::Rectangle<i32> {
    cosmic::iced::Rectangle {
        x: (bounds.x - offset.x).max(1.0) as i32,
        y: (bounds.y - offset.y).max(1.0) as i32,
        width: bounds.width.max(1.0) as i32,
        height: bounds.height.max(1.0) as i32,
    }
}

/// Which unlock field the applet popup should autofocus: the PIN field when
/// TPM PIN unlock is active, the master password field otherwise. Pulled out
/// as a pure function so the decision is unit-testable independent of the
/// opaque `Task` returned by `text_input::focus`.
pub(crate) fn unlock_focus_id(mode: crate::message::UnlockMode) -> cosmic::widget::Id {
    if mode == crate::message::UnlockMode::Pin {
        unlock::pin_input_id()
    } else {
        unlock::password_input_id()
    }
}

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
                    return Some(Task::done(cosmic::Action::Cosmic(
                        cosmic::app::Action::Surface(cosmic::surface::action::destroy_popup(id)),
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
                    let _ = agent.send(auth_actions::lock()).await;
                },
                |_| Action::App(Message::Exit),
            )),
            Message::LogoutAndQuit => Some(Task::perform(
                async {
                    let agent = AgentClient::new();
                    let _ = agent.send(auth_actions::logout()).await;
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
            Message::AppletQuitMenuToggle => {
                self.applet_quit_expanded = !self.applet_quit_expanded;
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

    /// Resets transient popup state and builds the task that opens the
    /// applet popup, focused on whichever unlock field is actually shown
    /// (PIN when TPM PIN unlock is active, master password otherwise).
    ///
    /// `(offset, bounds)` come from the icon click and position the popup
    /// next to the clicked applet icon. Only ever called from a click:
    /// cosmic-panel drops popups created while no panel surface is
    /// hovered/focused (see the note in `update_lifecycle`'s `PinRequested`),
    /// so opening from anywhere else can never map a popup.
    pub(crate) fn open_applet_popup_task(
        &mut self,
        anchor: (cosmic::iced::Vector, cosmic::iced::Rectangle),
    ) -> Task<Message> {
        // Reset only truly transient state; preserve search query and
        // favourites-filter so the popup re-opens with the last search intact.
        self.applet_unlock_password.zeroize();
        self.applet_unlock_password_revealed = false;
        self.applet_reprompt_id = None;
        self.applet_reprompt_password.zeroize();
        self.applet_reprompt_password_revealed = false;
        self.applet_error = None;

        let mut tasks = Vec::new();
        tasks.push(check_protocol_version());
        tasks.push(text_input::focus(unlock_focus_id(self.unlock_mode)));
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
                        session_id,
                        lock_epoch,
                    }) => Ok((
                        config,
                        needs_login,
                        has_account,
                        is_locked,
                        sync_failed,
                        session_id,
                        lock_epoch,
                    )),
                    Ok(Response::Error { message }) => Err(message),
                    _ => Err("unexpected response".to_string()),
                }
            },
            |res| cosmic::Action::App(Message::ConfigReceived(res)),
        ));

        let popup_task = Task::done(cosmic::Action::Cosmic(cosmic::app::Action::Surface(
            cosmic::surface::action::app_popup::<CosmicBWardenApp>(
                |_| Default::default(),
                move |state: &mut CosmicBWardenApp| {
                    let new_id = window::Id::unique();
                    tracing::info!(?new_id, "creating applet popup surface");
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
                    let (offset, bounds) = anchor;
                    // Clamp the anchor rectangle to at least 1px on every side
                    // (as cosmic-applet-time does): a 0-width/0-height or negative
                    // anchor from a degenerate icon bounds would produce a
                    // zero-sized positioning rect and a misplaced/never-mapped
                    // popup. `bounds` is in logical floats; cast after clamping.
                    popup_settings.positioner.anchor_rect = clamped_anchor_rect(offset, bounds);
                    popup_settings.positioner.size_limits =
                        crate::view::applet::applet_popup_limits();
                    popup_settings
                },
                None,
            ),
        )));
        tasks.push(popup_task);
        Task::batch(tasks)
    }

    fn applet_copy_to_clipboard(&mut self, value: String) -> Task<Message> {
        let clipboard_task = self.copy_to_clipboard_with_autoclear(value);
        let toast_task = self
            .applet_toasts
            .push(Toast::new(fl!("copied-to-clipboard")))
            .map(cosmic::Action::App);
        Task::batch(vec![clipboard_task, toast_task])
    }
}
