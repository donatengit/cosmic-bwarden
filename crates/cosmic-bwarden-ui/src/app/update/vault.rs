use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::fetch_sidebar_entries;
use crate::app::update::vault_actions;
use crate::message::{Message, View};
use crate::view::vault::sidebar;
use cosmic::app::Task;
use cosmic::Action;
use cosmic_bwarden_core::agent_client::AgentClient;

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
                let action = vault_actions::fetch_entry(id, None);
                Some(Task::perform(
                    async move {
                        let agent = AgentClient::new();
                        match agent.send(action).await {
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
            Message::DeleteEntry(id) => {
                self.show_delete_confirm = Some(id);
                Some(Task::none())
            }
            Message::ConfirmDelete => {
                self.deleting = true;
                if let Some(id) = self.show_delete_confirm.take() {
                    let action = vault_actions::delete_entry(id);
                    Some(Task::perform(
                        async move {
                            let agent = AgentClient::new();
                            match agent.send(action).await {
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

                let action = vault_actions::toggle_pin(id.clone(), is_pinned);

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
                    let action =
                        vault_actions::fetch_entry(id, Some(self.reprompt_password.clone()));
                    Some(Task::perform(
                        async move {
                            let agent = AgentClient::new();
                            match agent.send(action).await {
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
