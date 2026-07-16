use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::fetch_sidebar_entries;
use crate::message::{Message, View};
use crate::view::vault::sidebar;
use cosmic::app::Task;
use cosmic::Action;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::db::{Entry, EntryData, Secret};
use cosmic_bwarden_core::protocol::{Action as AgentAction, EntryType, Response, SidebarEntry};
use zeroize::Zeroize;

fn apply_local_filter(
    all: &[SidebarEntry],
    query: &str,
    entry_type: Option<&EntryType>,
    only_pinned: bool,
) -> Vec<SidebarEntry> {
    let q = query.trim().to_lowercase();
    all.iter()
        .filter(|e| {
            if only_pinned && !e.is_pinned {
                return false;
            }
            if let Some(et) = entry_type {
                if std::mem::discriminant(&e.entry_type) != std::mem::discriminant(et) {
                    return false;
                }
            }
            if q.is_empty() {
                return true;
            }
            if e.name.to_lowercase().contains(&q) {
                return true;
            }
            e.username
                .as_ref()
                .map(|u| u.to_lowercase().contains(&q))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

impl CosmicBWardenApp {
    pub fn update_vault(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::SearchChanged(q) => {
                self.search_query = q;
                self.entries = apply_local_filter(
                    &self.all_entries,
                    &self.search_query,
                    self.filter_type.as_ref(),
                    self.search_only_pinned,
                );
                Some(Task::none())
            }
            Message::PaneResized(event) => {
                // The window-resize hook fires before any drag can happen, so
                // the width is known here in practice; fall back to no minimum
                // rather than guessing one.
                let min_ratio = self
                    .vault_window_width
                    .map_or(0.0, crate::app::state::sidebar_min_ratio);
                let ratio = event
                    .ratio
                    .clamp(min_ratio, crate::app::state::SIDEBAR_MAX_RATIO);
                self.vault_panes.resize(event.split, ratio);
                self.sidebar_ratio = ratio;
                Some(Task::none())
            }
            Message::FilterTabActivated(entity) => {
                self.filter_model.activate(entity);
                let idx = self.filter_model.position(entity).unwrap_or(0);
                self.filter_type = sidebar::idx_to_filter(idx as usize);
                self.entries = apply_local_filter(
                    &self.all_entries,
                    &self.search_query,
                    self.filter_type.as_ref(),
                    self.search_only_pinned,
                );
                Some(Task::none())
            }
            Message::SelectEntry(id) => {
                self.selected_entry_id = Some(id.clone());
                self.selected_entry = None;
                self.editing_entry = None;
                self.view = View::Vault;
                Some(Task::perform(
                    async move {
                        let agent = AgentClient::new();
                        match agent
                            .send(AgentAction::GetEntry { id, password: None })
                            .await
                        {
                            Ok(Response::Entry { entry }) => Ok(entry),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    },
                    |res| Action::App(Message::EntryReceived(res)),
                ))
            }
            Message::EntryReceived(res) => {
                match res {
                    Ok(entry) => {
                        self.notes_content = cosmic::widget::text_editor::Content::with_text(
                            entry.notes.as_deref().unwrap_or(""),
                        );
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
                    id: format!(
                        "new-{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0)
                    ),
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
                if let Some(mut entry) = self.editing_entry.take() {
                    if let Some(notes) = &entry.notes {
                        if notes.trim().is_empty() {
                            entry.notes = None;
                        }
                    }
                    Some(Task::perform(
                        async move {
                            let agent = AgentClient::new();
                            match agent.send(AgentAction::UpdateEntry { entry }).await {
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
            Message::DeleteEntry(id) => {
                self.show_delete_confirm = Some(id);
                Some(Task::none())
            }
            Message::ConfirmDelete => {
                self.deleting = true;
                if let Some(id) = self.show_delete_confirm.take() {
                    Some(Task::perform(
                        async move {
                            let agent = AgentClient::new();
                            match agent.send(AgentAction::DeleteEntry { id }).await {
                                Ok(Response::Ack) => Ok(()),
                                Ok(Response::Error { message }) => Err(message),
                                _ => Err("unexpected response".to_string()),
                            }
                        },
                        |res| Action::App(Message::DeleteEntryResult(res)),
                    ))
                } else {
                    Some(Task::none())
                }
            }
            Message::CancelDelete => {
                self.show_delete_confirm = None;
                Some(Task::none())
            }
            Message::DeleteEntryResult(res) => {
                self.deleting = false;
                match res {
                    Ok(()) => {
                        self.selected_entry_id = None;
                        self.selected_entry = None;
                        self.editing_entry = None;
                        self.search_id += 1;
                        Some(fetch_sidebar_entries(self.search_id, None, None, false))
                    }
                    Err(e) => {
                        self.sync_failed = true;
                        self.error = Some(e);
                        Some(Task::none())
                    }
                }
            }
            Message::EntriesReceived(id, res) => {
                if id == self.search_id {
                    match res {
                        Ok(entries) => {
                            // This is always a full (no-query) fetch used to refresh
                            // the cache after mutations. Re-apply the local filter so
                            // the displayed list stays consistent with the search bar.
                            self.all_entries = entries;
                            self.entries = apply_local_filter(
                                &self.all_entries,
                                &self.search_query,
                                self.filter_type.as_ref(),
                                self.search_only_pinned,
                            );
                            self.error = None;
                            // If the selected entry was deleted on the server but the
                            // previous sync failed (leaving it visible in the stale
                            // cache), clear the detail panel now that entries are fresh.
                            if let Some(sel_id) = &self.selected_entry_id {
                                if !self.all_entries.iter().any(|e| e.id == *sel_id) {
                                    self.selected_entry_id = None;
                                    self.selected_entry = None;
                                    self.editing_entry = None;
                                }
                            }
                        }
                        Err(e) if e == "agent is locked" => {
                            self.all_entries.clear();
                            self.entries.clear();
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                Some(Task::none())
            }
            Message::SyncClicked => {
                self.syncing = true;
                Some(Task::perform(
                    async {
                        let agent = AgentClient::new();
                        match agent.send(AgentAction::Sync).await {
                            Ok(Response::Ack) => Ok(()),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    },
                    |res| Action::App(Message::SyncResult(res)),
                ))
            }
            Message::SyncResult(res) => {
                self.syncing = false;
                match res {
                    Ok(()) => {
                        // Clear error state immediately — don't wait for the
                        // VaultChanged → RefreshStateInternal → ConfigReceived
                        // round-trip that would otherwise leave the red button
                        // visible for a noticeable moment after sync succeeds.
                        self.sync_failed = false;
                        self.error = None;
                        self.search_id += 1;
                        Some(Task::batch(vec![fetch_sidebar_entries(
                            self.search_id,
                            None,
                            None,
                            false,
                        )]))
                    }
                    Err(e) => {
                        self.sync_failed = true;
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

                Some(Task::perform(
                    async move {
                        let agent = AgentClient::new();
                        let _ = agent.send(action).await;
                    },
                    |_| Action::None,
                ))
            }
            Message::ToggleSearchPinned => {
                self.search_only_pinned = !self.search_only_pinned;
                self.entries = apply_local_filter(
                    &self.all_entries,
                    &self.search_query,
                    self.filter_type.as_ref(),
                    self.search_only_pinned,
                );
                Some(Task::none())
            }
            Message::RepromptPasswordChanged(p) => {
                self.reprompt_password = p;
                Some(Task::none())
            }
            Message::SubmitReprompt => {
                if let Some(id) = self.show_reprompt.clone() {
                    let password = self.reprompt_password.clone();
                    Some(Task::perform(
                        async move {
                            let agent = AgentClient::new();
                            match agent
                                .send(AgentAction::GetEntry {
                                    id,
                                    password: Some(password),
                                })
                                .await
                            {
                                Ok(Response::Entry { entry }) => Ok(entry),
                                Ok(Response::Error { message }) => Err(message),
                                _ => Err("unexpected response".to_string()),
                            }
                        },
                        |res| Action::App(Message::EntryReceived(res)),
                    ))
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

    /// Runtime hook (via `Application::on_window_resize`): records the vault
    /// window's width and keeps the sidebar within its limits. On the first
    /// report the split snaps to exactly `SIDEBAR_MIN_WIDTH`, which is the
    /// width the vault opens with; on later reports the current ratio is only
    /// re-clamped, so shrinking the window never drops the sidebar below the
    /// pixel minimum.
    pub(crate) fn vault_window_resized(&mut self, width: f32) {
        if width <= 0.0 {
            return;
        }
        let first = self.vault_window_width.is_none();
        self.vault_window_width = Some(width);

        let min_ratio = crate::app::state::sidebar_min_ratio(width);
        let ratio = if first {
            min_ratio
        } else {
            self.sidebar_ratio
                .clamp(min_ratio, crate::app::state::SIDEBAR_MAX_RATIO)
        };
        if ratio != self.sidebar_ratio {
            let split = self.vault_panes.layout().splits().next().copied();
            if let Some(split) = split {
                self.vault_panes.resize(split, ratio);
            }
            self.sidebar_ratio = ratio;
        }
    }
}
