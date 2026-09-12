//! On-demand secret fetch after a `GetEntryMeta` selection (R-1).

use super::vault_actions;
use crate::app::state::CosmardenApp;
use crate::message::{Message, OnDemandPayload, RepromptIntent};
use cosmarden_core::agent_client::AgentClient;
use cosmarden_core::db::{EntryData, Secret};
use cosmic::app::Task;
use cosmic::Action;

impl CosmardenApp {
    pub fn update_vault_secrets(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::ToggleRevealField(id, field) => Some(self.toggle_reveal_field(id, field)),
            Message::CopySecretField(id, field) => Some(self.copy_secret_field(id, field)),
            Message::OnDemandSecretLoaded {
                field,
                copy,
                result,
            } => Some(self.on_demand_secret_loaded(field, copy, result)),
            _ => None,
        }
    }

    fn toggle_reveal_field(&mut self, id: String, field: String) -> Task<Message> {
        let key = (id.clone(), field.clone());
        if self.revealed_fields.contains(&key) {
            self.revealed_fields.remove(&key);
            if field == "TOTP" {
                if let Some(mut code) = self.totp_code.take() {
                    use zeroize::Zeroize as _;
                    code.zeroize();
                }
            }
            return Task::none();
        }
        let loaded = self
            .selected_entry
            .as_ref()
            .is_some_and(|e| vault_actions::secret_is_loaded(e, &field));
        if loaded {
            self.revealed_fields.insert(key);
            return Task::none();
        }
        self.pending_secret_field = Some(field.clone());
        dispatch_on_demand(id, field, false, None)
    }

    fn copy_secret_field(&mut self, id: String, field: String) -> Task<Message> {
        if field == "TOTP" {
            if let Some(code) = self.totp_code.clone() {
                return self.copy_to_clipboard_with_autoclear(code.expose().to_string());
            }
        } else if let Some(entry) = &self.selected_entry {
            if let Some(plain) = vault_actions::field_plaintext(entry, &field) {
                return self.copy_to_clipboard_with_autoclear(plain);
            }
        }
        self.pending_secret_field = Some(field.clone());
        dispatch_on_demand(id, field, true, None)
    }

    fn on_demand_secret_loaded(
        &mut self,
        field: String,
        copy: bool,
        result: Result<OnDemandPayload, String>,
    ) -> Task<Message> {
        match result {
            Err(e) if e == "reprompt_required" => {
                self.show_reprompt = self.selected_entry_id.clone();
                self.reprompt_intent = Some(if copy {
                    RepromptIntent::Copy { field }
                } else {
                    RepromptIntent::Reveal { field }
                });
                Task::none()
            }
            Err(e) => {
                self.error = Some(e);
                Task::none()
            }
            Ok(payload) => {
                let to_copy;
                match payload {
                    OnDemandPayload::Password(p) => {
                        if let Some(entry) = &mut self.selected_entry {
                            if let EntryData::Login { password, .. } = &mut entry.data {
                                *password = Some(p.clone());
                            }
                        }
                        to_copy = Some(p);
                    }
                    OnDemandPayload::Totp(code) => {
                        self.totp_code = Some(code.clone());
                        to_copy = Some(code);
                    }
                    OnDemandPayload::Entry(entry) => {
                        to_copy = vault_actions::field_plaintext(&entry, &field).map(Secret::from);
                        self.selected_entry = Some(*entry);
                    }
                }
                if let Some(id) = self.selected_entry_id.clone() {
                    self.revealed_fields.insert((id, field));
                }
                self.pending_secret_field = None;
                self.show_reprompt = None;
                self.reprompt_intent = None;
                if copy {
                    if let Some(plain) = to_copy {
                        return self.copy_to_clipboard_with_autoclear(plain.expose().to_string());
                    }
                }
                Task::none()
            }
        }
    }
}

pub(super) fn dispatch_on_demand(
    id: String,
    field: String,
    copy: bool,
    reprompt: Option<String>,
) -> Task<Message> {
    let action = vault_actions::on_demand_secret(&field, id, reprompt);
    let field_for_parse = field.clone();
    Task::perform(
        async move {
            let agent = AgentClient::new();
            match agent.send(action).await {
                Ok(resp) => vault_actions::parse_on_demand_response(&field_for_parse, resp),
                Err(e) => Err(e.to_string()),
            }
        },
        move |result| {
            Action::App(Message::OnDemandSecretLoaded {
                field,
                copy,
                result,
            })
        },
    )
}
