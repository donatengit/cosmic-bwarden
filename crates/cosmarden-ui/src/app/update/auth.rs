use crate::app::state::CosmardenApp;
use crate::app::tasks::fetch_sidebar_entries;
use crate::app::update::auth_actions;
use crate::fl;
use crate::message::{Message, UnlockMode, View};
use crate::MIN_PIN_LEN;
use cosmarden_core::agent_client::AgentClient;
use cosmarden_core::protocol::Response;
use cosmic::app::Task;
use cosmic::Action;
use tracing::error;
use zeroize::Zeroize;

impl CosmardenApp {
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
                        match agent.send(auth_actions::unlock_with_pin(pin)).await {
                            Ok(Response::Ack) => Ok(()),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    },
                    |res| Action::App(Message::MainWindowPinResult(res)),
                ))
            }
            Message::MainWindowPinResult(res) => {
                self.auth_loading = false;
                self.main_window_pin.zeroize();
                match res {
                    Ok(()) => {
                        self.view = View::Vault;
                        self.error = None;
                        self.pin_incorrect = false;
                        // Refresh agent state right away (GetConfig + entries)
                        // instead of relying solely on the Event::Unlocked
                        // broadcast: if the event subscription is delayed or
                        // dropped, the epoch-stamped config still lets us drop
                        // any stale "still locked" response that would
                        // otherwise bounce the view back to Unlock.
                        Some(Task::perform(async {}, |_| {
                            Action::App(Message::RefreshStateInternal)
                        }))
                    }
                    Err(e) => {
                        error!("PIN unlock failed: {}", e);
                        if e == cosmarden_core::protocol::ERR_TPM_UNSEAL_FAILED {
                            // Wrong PIN / DA lockout: the raw message is
                            // log-only; the incorrect-PIN/attempts caption is
                            // the feedback. A wrong PIN consumed a DA attempt
                            // — reveal and refresh the counter.
                            self.error = None;
                            self.pin_incorrect = true;
                            Some(super::lifecycle::check_tpm_da_task())
                        } else if e == cosmarden_core::protocol::ERR_TPM_STATE_CHANGED {
                            // PCR state changed (BIOS/firmware update): the
                            // PIN is fine, the machine state moved. Never call
                            // this a wrong PIN — point at the recovery path.
                            self.error = Some(fl!("tpm-state-changed"));
                            self.pin_incorrect = false;
                            self.unlock_mode = UnlockMode::Password;
                            Some(Task::none())
                        } else if e == cosmarden_core::protocol::ERR_TPM_BLOB_MISSING {
                            // The sealed data is gone (server URL changed, TPM
                            // reset). Retrying only burns DA attempts against a
                            // blob that no longer exists.
                            self.error = Some(fl!("tpm-blob-missing"));
                            self.pin_incorrect = false;
                            self.unlock_mode = UnlockMode::Password;
                            Some(Task::none())
                        } else {
                            // Environmental failure (agent/config/account) —
                            // show it, don't mislabel it as a wrong PIN.
                            self.error = Some(e);
                            Some(Task::none())
                        }
                    }
                }
            }
            Message::LoginSubmitted => {
                // Toggle on means they want PIN: require a long enough one
                // rather than treating an empty box as "skip" (skip is the
                // toggle off, and that path deletes leftover TPM blobs).
                if self.login_pin_enabled && self.login_pin.chars().count() < MIN_PIN_LEN {
                    self.error = Some(fl!("pin-too-short", count = MIN_PIN_LEN));
                    return Some(Task::none());
                }

                let action = auth_actions::login(
                    self.login_email.clone(),
                    self.login_password.clone(),
                    self.login_server.clone(),
                    self.login_remember,
                    &self.login_verification_code,
                );

                self.auth_loading = true;
                Some(Task::perform(
                    async move {
                        let agent = AgentClient::new();
                        match agent.send(action).await {
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
            Message::SessionRestoreToggle => {
                self.show_session_restore = !self.show_session_restore;
                self.session_restore_password.zeroize();
                self.session_restore_password.clear();
                Some(Task::none())
            }
            Message::SessionRestorePasswordChanged(password) => {
                self.session_restore_password = password;
                Some(Task::none())
            }
            Message::SessionRestoreRevealToggled => {
                self.session_restore_revealed = !self.session_restore_revealed;
                Some(Task::none())
            }
            Message::SessionRestoreSubmitted => {
                // Plain `Unlock`: the agent re-derives the hash and re-auths
                // (handler::auth::reauth), which is all a dead session needs —
                // no logout, no re-download of the vault. Deliberately does not
                // route through UnlockSubmitted, whose PIN field would be empty
                // here and would clear the sealed blob.
                let password = self.session_restore_password.clone();
                self.auth_loading = true;
                Some(Task::perform(
                    async move {
                        let agent = AgentClient::new();
                        match agent.send(auth_actions::unlock(password)).await {
                            Ok(Response::Ack) => Ok(()),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    },
                    |res| Action::App(Message::AuthResult(res)),
                ))
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
                        match agent.send(auth_actions::unlock(password)).await {
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
                        // A successful unlock re-authenticates, so a session-restore
                        // prompt has served its purpose.
                        self.show_session_restore = false;
                        self.session_restore_password.zeroize();
                        self.session_restore_password.clear();

                        let mut tasks =
                            vec![fetch_sidebar_entries(self.search_id, None, None, false)];

                        // Same rationale as MainWindowPinResult: fetch the
                        // epoch-stamped config directly instead of relying on
                        // the Event::Unlocked broadcast alone.
                        tasks.push(Task::perform(async {}, |_| {
                            Action::App(Message::RefreshStateInternal)
                        }));

                        // Master-password unlock: reseal (non-empty PIN) or
                        // remove a stale blob (empty). Login uses the same
                        // decision via the PIN toggle — leaving it off after
                        // logout must delete leftover TPM blobs, not keep them.
                        if apply_unlock_pin {
                            if let Some(task) = self.apply_unlock_pin_task() {
                                tasks.push(task);
                            }
                        } else if let Some(task) = self.apply_login_pin_task() {
                            tasks.push(task);
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
                    let _ = agent.send(auth_actions::lock()).await;
                },
                |_| Action::App(Message::LockResult),
            )),
            Message::LockResult => {
                self.wipe_for_lock();
                Some(Task::none())
            }
            Message::LogoutClicked => Some(Task::perform(
                async {
                    let agent = AgentClient::new();
                    let _ = agent.send(auth_actions::logout()).await;
                },
                |_| Action::App(Message::LogoutResult),
            )),
            Message::LogoutResult => {
                self.view = View::Setup;
                self.wipe_session_secrets();
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
        // Consume the field either way — the PIN must not outlive this call,
        // including on the no-TPM path where nothing is sent.
        let pin = std::mem::take(&mut self.unlock_pin);
        self.unlock_pin_revealed = false;
        Self::pin_intent_task(auth_actions::apply_unlock_pin(
            self.tpm_available,
            self.tpm_configured,
            pin,
        ))
    }

    /// Same reseal/clear decision as the unlock form, driven by the login
    /// screen's PIN toggle. Consumes `self.login_pin`.
    pub(crate) fn apply_login_pin_task(&mut self) -> Option<Task<Message>> {
        let pin_enabled = self.login_pin_enabled;
        self.login_pin_enabled = false;
        self.login_pin_revealed = false;
        let pin = std::mem::take(&mut self.login_pin);
        Self::pin_intent_task(auth_actions::apply_login_pin(
            self.tpm_available,
            self.tpm_configured,
            pin_enabled,
            pin,
        ))
    }

    fn pin_intent_task(intent: auth_actions::UnlockPinIntent) -> Option<Task<Message>> {
        // Each arm routes to a different result message, so the mapping to a
        // task stays here while the *decision* stays unit-testable.
        match intent {
            auth_actions::UnlockPinIntent::Reseal(action) => Some(Task::perform(
                async move {
                    let agent = AgentClient::new();
                    match agent.send(action).await {
                        Ok(Response::Ack) => Ok(()),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                },
                |res| Action::App(Message::TpmSetupResult(res)),
            )),
            auth_actions::UnlockPinIntent::ClearStale(action) => Some(Task::perform(
                async move {
                    let agent = AgentClient::new();
                    match agent.send(action).await {
                        Ok(Response::Ack) => Ok(()),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                },
                |res| Action::App(Message::TpmDisableResult(res)),
            )),
            // Nothing was sent, so the PIN came back unused: wipe it rather
            // than letting a plain String drop leave it in freed memory.
            auth_actions::UnlockPinIntent::Nothing(mut unused) => {
                unused.zeroize();
                None
            }
        }
    }
}
