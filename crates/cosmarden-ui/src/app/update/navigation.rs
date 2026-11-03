//! The edit-buffer guard for vault navigation.
//!
//! Add, the Settings pane, the password generator, and picking another entry
//! all replace the buffer that `view_entry_details` is editing. When one of
//! those arrives with a draft open, the destination is parked and a dialog asks
//! before anything is dropped, so a stray sidebar click cannot delete what the
//! user typed. Confirming replays the parked message rather than rebuilding the
//! destination here: each of the four handlers stays its own single definition.
//!
//! Lock, logout and autolock do not come through here. They must drop the draft
//! immediately — the guard is about not losing input, not about keeping
//! plaintext in memory past a lock.

use crate::app::state::CosmardenApp;
use crate::message::Message;
use cosmic::app::Task;

/// Messages that replace the vault pane or the entry shown in it.
fn replaces_edit_buffer(message: &Message) -> bool {
    matches!(
        message,
        Message::SelectEntry(_)
            | Message::AddEntryRequested
            | Message::SettingsViewClicked
            | Message::GeneratorViewClicked
    )
}

impl CosmardenApp {
    /// Park `message` when it would replace an open draft. Returns true when it
    /// was parked, so `update_app` stops before the handler runs.
    pub(crate) fn hold_for_draft(&mut self, message: &Message) -> bool {
        if self.editing_entry.is_none() || !replaces_edit_buffer(message) {
            return false;
        }
        self.pending_navigation = Some(message.clone());
        true
    }

    /// The discard dialog's two answers.
    pub(crate) fn update_navigation(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::DiscardEditConfirmed => {
                let next = self.pending_navigation.take();
                // The same cleanup `CancelEdit` does, applied only now that the
                // user has chosen to drop the draft: the buffer and the
                // plaintext notes content must not outlive it.
                self.editing_entry = None;
                self.notes_content = cosmic::widget::text_editor::Content::new();
                self.edit_password_revealed = false;
                Some(match next {
                    Some(next) => self.update_app(next),
                    None => Task::none(),
                })
            }
            Message::DiscardEditCancelled => {
                self.pending_navigation = None;
                Some(Task::none())
            }
            _ => None,
        }
    }
}
