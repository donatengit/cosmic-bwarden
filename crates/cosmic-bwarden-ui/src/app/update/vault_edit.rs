//! The vault detail pane's *edit buffer* lifecycle: composing a new entry,
//! editing an existing one, mutating individual fields, and persisting the
//! result.
//!
//! Split out of `vault.rs` (which kept list, search, selection, and sync) so
//! each module has one responsibility and stays within the size limit.
//!
//! The save path is deliberately thin: it hands the draft to
//! `entry_save::save_action` in core and dispatches whatever that returns.
//! Choosing the action there rather than inline in the async block is what
//! makes the choice unit-testable — see that module's header for the HTTP 400
//! this arrangement exists to prevent.

use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::fetch_sidebar_entries;
use crate::message::Message;
use cosmic::app::Task;
use cosmic::Action;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::db::{Entry, EntryData, Secret};
use cosmic_bwarden_core::protocol::entry_save as save;
use cosmic_bwarden_core::protocol::{EntryType, Response};

impl CosmicBWardenApp {
    /// Handles the edit-buffer messages; returns `None` for anything else so
    /// the dispatch chain in `update/mod.rs` can keep looking.
    pub fn update_vault_edit(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::AddEntryRequested => {
                let new_entry = Entry {
                    id: save::new_placeholder_id(),
                    org_id: None,
                    folder: None,
                    folder_id: None,
                    name: "New Entry".to_string(),
                    favorite: false,
                    data: EntryData::Login {
                        username: Some(String::new()),
                        password: Some(String::new().into()),
                        totp: None,
                        uris: Vec::new(),
                    },
                    fields: Vec::new(),
                    notes: Some(Secret::from(String::new())),
                    history: Vec::new(),
                    key: None,
                    master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
                };
                self.selected_entry = Some(new_entry.clone());
                self.editing_entry = Some(new_entry);
                self.selected_entry_id = None;
                self.notes_content = cosmic::widget::text_editor::Content::new();
                self.edit_password_revealed = false;
                Some(Task::none())
            }
            Message::EditEntry => {
                if let Some(entry) = &self.selected_entry {
                    self.editing_entry = Some(entry.clone());
                    self.notes_content = cosmic::widget::text_editor::Content::with_text(
                        entry.notes.as_deref().unwrap_or(""),
                    );
                    self.edit_password_revealed = false;
                }
                Some(Task::none())
            }
            Message::CancelEdit => {
                self.editing_entry = None;
                self.notes_content = cosmic::widget::text_editor::Content::with_text(
                    self.selected_entry
                        .as_ref()
                        .and_then(|e| e.notes.as_deref())
                        .unwrap_or(""),
                );
                Some(Task::none())
            }
            Message::SaveEdit => {
                // Keep `editing_entry` until the agent confirms: taking it here
                // discarded everything the user typed whenever the save failed.
                if let Some(mut entry) = self.editing_entry.clone() {
                    if let Some(notes) = &entry.notes {
                        if notes.trim().is_empty() {
                            entry.notes = None;
                        }
                    }
                    let action = save::save_action(entry);
                    Some(Task::perform(
                        async move {
                            let agent = AgentClient::new();
                            match agent.send(action).await {
                                Ok(Response::Ack) => Ok(()),
                                Ok(Response::Error { message }) => Err(message),
                                _ => Err("unexpected response".to_string()),
                            }
                        },
                        |res| Action::App(Message::SaveEditResult(res)),
                    ))
                } else {
                    Some(Task::none())
                }
            }
            Message::SaveEditResult(res) => match res {
                Ok(()) => {
                    let id = self.selected_entry_id.clone();
                    // A just-created entry only ever existed client-side under
                    // its `new-` placeholder id; drop it so the detail pane
                    // doesn't keep showing a phantom the server never saw. The
                    // sidebar refresh below brings back the real one.
                    if self.selected_entry.as_ref().is_some_and(save::is_new) {
                        self.selected_entry = None;
                    }
                    self.editing_entry = None;
                    self.search_id += 1;
                    let sidebar_task = fetch_sidebar_entries(self.search_id, None, None, false);
                    if let Some(id) = id {
                        Some(Task::batch(vec![
                            sidebar_task,
                            Task::done(Action::App(Message::SelectEntry(id))),
                        ]))
                    } else {
                        Some(sidebar_task)
                    }
                }
                Err(e) => {
                    self.sync_failed = true;
                    self.error = Some(e);
                    Some(Task::none())
                }
            },
            Message::EditFieldChanged(field, value) => {
                if let Some(entry) = &mut self.editing_entry {
                    match &mut entry.data {
                        EntryData::Login {
                            username, password, ..
                        } => {
                            if field == "Username" {
                                *username = Some(value.clone());
                            } else if field == "Password" {
                                *password = Some(value.clone().into());
                            }
                        }
                        EntryData::SshKey {
                            private_key,
                            public_key,
                            fingerprint,
                        } => {
                            if field == "Private Key" {
                                *private_key = Some(value.clone().into());
                            } else if field == "Public Key" {
                                *public_key = Some(value.clone());
                            } else if field == "Fingerprint" {
                                *fingerprint = Some(value.clone());
                            }
                        }
                        EntryData::Card {
                            number,
                            cardholder_name,
                            brand,
                            ..
                        } => {
                            if field == "Card Number" {
                                *number = Some(value.clone().into());
                            } else if field == "Cardholder" {
                                *cardholder_name = Some(value.clone());
                            } else if field == "Brand" {
                                *brand = Some(value.clone());
                            }
                        }
                        EntryData::Identity {
                            username, email, ..
                        } => {
                            if field == "Username" {
                                *username = Some(value.clone());
                            } else if field == "Email" {
                                *email = Some(value.clone());
                            }
                        }
                        EntryData::SecureNote => {}
                    }

                    // Also check custom fields
                    if let Some(f) = entry
                        .fields
                        .iter_mut()
                        .find(|f| f.name.as_deref() == Some(&field))
                    {
                        f.value = Some(value.into());
                    }
                }
                Some(Task::none())
            }
            Message::EditNameChanged(name) => {
                if let Some(entry) = &mut self.editing_entry {
                    entry.name = name;
                }
                Some(Task::none())
            }
            Message::NotesAction(action) => {
                self.notes_content.perform(action);
                if let Some(entry) = &mut self.editing_entry {
                    entry.notes = Some(self.notes_content.text().into());
                }
                Some(Task::none())
            }
            Message::NewEntryTypeChanged(ty) => {
                if let Some(entry) = &mut self.editing_entry {
                    match ty {
                        EntryType::Login => {
                            entry.data = EntryData::Login {
                                username: Some(String::new()),
                                password: Some(String::new().into()),
                                totp: None,
                                uris: Vec::new(),
                            };
                        }

                        EntryType::SecureNote => {
                            entry.data = EntryData::SecureNote;
                        }
                        EntryType::SshKey => {
                            entry.data = EntryData::SshKey {
                                private_key: Some(String::new().into()),
                                public_key: Some(String::new()),
                                fingerprint: None,
                            };
                        }

                        _ => {}
                    }
                }
                Some(Task::none())
            }
            _ => None,
        }
    }
}
