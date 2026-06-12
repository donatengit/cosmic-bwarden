use cosmic::app::Task;
use cosmic::Action;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response, EntryType};
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::db::{Entry, EntryData, Secret};
use crate::message::{Message, View};
use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::{fetch_sidebar_entries, fetch_top_entries};
use zeroize::Zeroize;

impl CosmicBWardenApp {
    pub fn update_vault(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::SearchChanged(q) => {
                self.search_query = q;
                self.search_id += 1;
                Some(fetch_sidebar_entries(self.search_id, Some(self.search_query.clone()), self.filter_type.clone(), self.search_only_pinned))
            }
            Message::SearchSubmitted(q) => {
                self.search_id += 1;
                Some(fetch_sidebar_entries(self.search_id, Some(q), self.filter_type.clone(), self.search_only_pinned))
            }
            Message::FilterTypeChanged(t) => {
                self.filter_type = t;
                self.search_id += 1;
                Some(fetch_sidebar_entries(self.search_id, Some(self.search_query.clone()), self.filter_type.clone(), self.search_only_pinned))
            }
            Message::SelectEntry(id) => {
                self.selected_entry_id = Some(id.clone());
                self.selected_entry = None;
                self.editing_entry = None;
                self.view = View::Vault;
                Some(Task::perform(async move {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::GetEntry { id, password: None }).await {
                        Ok(Response::Entry { entry }) => Ok(entry),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                }, |res| Action::App(Message::EntryReceived(res))))
            }
            Message::EntryReceived(res) => {
                match res {
                    Ok(entry) => {
                        self.notes_content = cosmic::widget::text_editor::Content::with_text(entry.notes.as_deref().unwrap_or(""));
                        self.selected_entry = Some(entry);
                        self.show_reprompt = None;
                        self.reprompt_password.zeroize();
                        self.reprompt_password_revealed = false;
                    }
                    Err(e) if e == "reprompt_required" => {
                        self.show_reprompt = self.selected_entry_id.clone();
                    }
                    Err(e) => {
                        self.error = Some(e);
                    }
                }
                Some(Task::none())
            }
            Message::AddEntryRequested => {
                let new_entry = Entry {
                    id: format!("new-{}", chrono::Utc::now().timestamp()),
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
                    self.notes_content = cosmic::widget::text_editor::Content::with_text(entry.notes.as_deref().unwrap_or(""));
                    self.edit_password_revealed = false;
                }
                Some(Task::none())
            }
            Message::CancelEdit => {
                self.editing_entry = None;
                self.notes_content = cosmic::widget::text_editor::Content::with_text(
                    self.selected_entry.as_ref().and_then(|e| e.notes.as_deref()).unwrap_or("")
                );
                Some(Task::none())
            }
            Message::SaveEdit => {
                if let Some(mut entry) = self.editing_entry.take() {
                    if let Some(notes) = &entry.notes {
                        if notes.trim().is_empty() {
                            entry.notes = None;
                        }
                    }
                    Some(Task::perform(async move {
                        let agent = AgentClient::new();
                        match agent.send(AgentAction::UpdateEntry { entry }).await {
                            Ok(Response::Ack) => Ok(()),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    }, |res| Action::App(Message::SaveEditResult(res))))
                } else {
                    Some(Task::none())
                }
            }
            Message::SaveEditResult(res) => {
                match res {
                    Ok(()) => {
                        let id = self.selected_entry_id.clone();
                        self.editing_entry = None;
                        // Refresh sidebar
                        self.search_id += 1;
                        let sidebar_task = fetch_sidebar_entries(self.search_id, Some(self.search_query.clone()), self.filter_type.clone(), self.search_only_pinned);
                        
                        // Re-fetch selected entry if any
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
                        self.error = Some(e);
                        Some(Task::none())
                    }
                }
            }
            Message::EditFieldChanged(field, value) => {
                if let Some(entry) = &mut self.editing_entry {
                    match &mut entry.data {
                        EntryData::Login { username, password, .. } => {
                            if field == "Username" {
                                *username = Some(value.clone());
                            } else if field == "Password" {
                                *password = Some(value.clone().into());
                            }
                        }
                        EntryData::SshKey { private_key, public_key, fingerprint } => {
                            if field == "Private Key" {
                                *private_key = Some(value.clone().into());
                            } else if field == "Public Key" {
                                *public_key = Some(value.clone());
                            } else if field == "Fingerprint" {
                                *fingerprint = Some(value.clone());
                            }
                        }
                        EntryData::Card { number, cardholder_name, brand, .. } => {
                            if field == "Card Number" {
                                *number = Some(value.clone().into());
                            } else if field == "Cardholder" {
                                *cardholder_name = Some(value.clone());
                            } else if field == "Brand" {
                                *brand = Some(value.clone());
                            }
                        }
                        EntryData::Identity { username, email, .. } => {
                            if field == "Username" {
                                *username = Some(value.clone());
                            } else if field == "Email" {
                                *email = Some(value.clone());
                            }
                        }
                        EntryData::SecureNote => {}
                    }
                    
                    // Also check custom fields
                    if let Some(f) = entry.fields.iter_mut().find(|f| f.name.as_deref() == Some(&field)) {
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
            Message::DeleteEntry(id) => {
                self.show_delete_confirm = Some(id);
                Some(Task::none())
            }
            Message::ConfirmDelete => {
                if let Some(id) = self.show_delete_confirm.take() {
                    Some(Task::perform(async move {
                        let agent = AgentClient::new();
                        match agent.send(AgentAction::DeleteEntry { id }).await {
                            Ok(Response::Ack) => Ok(()),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    }, |res| Action::App(Message::DeleteEntryResult(res))))
                } else {
                    Some(Task::none())
                }
            }
            Message::CancelDelete => {
                self.show_delete_confirm = None;
                Some(Task::none())
            }
            Message::DeleteEntryResult(res) => {
                match res {
                    Ok(()) => {
                        self.selected_entry_id = None;
                        self.selected_entry = None;
                        self.editing_entry = None;
                        self.search_id += 1;
                        Some(fetch_sidebar_entries(self.search_id, Some(self.search_query.clone()), self.filter_type.clone(), self.search_only_pinned))
                    }
                    Err(e) => {
                        self.error = Some(e);
                        Some(Task::none())
                    }
                }
            }
            Message::EntriesReceived(id, res) => {
                if id == self.search_id {
                    match res {
                        Ok(entries) => {
                            self.entries = entries;
                            self.error = None;
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                Some(Task::none())
            }
            Message::TopEntriesReceived(res) => {
                match res {
                    Ok(entries) => {
                        self.top_entries = entries;
                        self.error = None;
                    }
                    Err(e) => self.error = Some(e),
                }
                Some(Task::none())
            }
            Message::SyncClicked => {
                Some(Task::perform(async {
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::Sync).await {
                        Ok(Response::Ack) => Ok(()),
                        Ok(Response::Error { message }) => Err(message),
                        _ => Err("unexpected response".to_string()),
                    }
                }, |res| Action::App(Message::SyncResult(res))))
            }
            Message::SyncResult(res) => {
                match res {
                    Ok(()) => {
                        self.search_id += 1;
                        Some(Task::batch(vec![
                            fetch_sidebar_entries(self.search_id, Some(self.search_query.clone()), self.filter_type.clone(), self.search_only_pinned),
                            fetch_top_entries(self.config.top_popular_count as usize, Some(self.config.top_popular_days)),
                        ]))
                    }
                    Err(e) => {
                        self.error = Some(e);
                        Some(Task::none())
                    }
                }
            }
            Message::TogglePin(id) => {
                let mut is_pinned = false;
                if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
                    entry.is_pinned = !entry.is_pinned;
                    is_pinned = entry.is_pinned;
                }
                
                // Also update top_entries for consistency if it's there
                if let Some(entry) = self.top_entries.iter_mut().find(|e| e.id == id) {
                    entry.is_pinned = is_pinned;
                }
                
                // If it's the selected entry, update it too
                if let Some(entry) = &mut self.selected_entry {
                    if entry.id == id {
                        entry.favorite = is_pinned;
                    }
                }

                let action = if is_pinned {
                    AgentAction::PinEntry { id: id.clone() }
                } else {
                    AgentAction::UnpinEntry { id: id.clone() }
                };
                
                Some(Task::perform(async move {
                    let agent = AgentClient::new();
                    let _ = agent.send(action).await;
                }, |_| Action::None))
            }
            Message::ToggleSearchPinned => {
                self.search_only_pinned = !self.search_only_pinned;
                self.search_id += 1;
                Some(fetch_sidebar_entries(self.search_id, Some(self.search_query.clone()), self.filter_type.clone(), self.search_only_pinned))
            }
            Message::RepromptPasswordChanged(p) => {
                self.reprompt_password = p;
                Some(Task::none())
            }
            Message::SubmitReprompt => {
                if let Some(id) = self.show_reprompt.clone() {
                    let password = self.reprompt_password.clone();
                    Some(Task::perform(async move {
                        let agent = AgentClient::new();
                        match agent.send(AgentAction::GetEntry { id, password: Some(password) }).await {
                            Ok(Response::Entry { entry }) => Ok(entry),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    }, |res| Action::App(Message::EntryReceived(res))))
                } else {
                    Some(Task::none())
                }
            }
            Message::CancelReprompt => {
                self.show_reprompt = None;
                self.reprompt_password.zeroize();
                self.reprompt_password_revealed = false;
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
