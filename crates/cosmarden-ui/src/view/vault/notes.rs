//! The detail pane's notes area.
//!
//! Notes are a secret-class slot: the agent's `redact_entry_secrets` blanks them
//! in every bulk/`GetEntryMeta` read, so an entry that carries them is one an
//! explicit `GetEntry` brought back. The view therefore renders the masked
//! placeholder until the user reveals the slot, and the plaintext only after a
//! reveal loaded it. Seeding an editable buffer from the redacted read instead
//! is what made notes render empty; the edit buffer is seeded by the edit form
//! (`Message::EditEntryLoaded`) alone.
//!
//! A `SecureNote`'s content *is* its notes, so selecting one fetches the body
//! (`vault_actions::notes_fetch_after_selection`) rather than leaving the item
//! unreadable. That fetch arrives through the same reveal path, which is why the
//! display rule below needs no `SecureNote` special case.

use crate::app::CosmardenApp;
use crate::fl;
use crate::message::Message;
use cosmarden_core::db::Entry;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, container, icon, text, text_editor};
use cosmic::Element;

/// Stable field key for the notes slot: what `revealed_fields`,
/// `ToggleRevealField`/`CopySecretField`, and the agent's `filled_secret_keys`
/// all match on. The same literal `"Notes"` appears in `vault_actions`'
/// `secret_is_loaded`/`field_plaintext` — it is a key, never display text.
const NOTES_FIELD: &str = "Notes";

/// What the view (non-edit) notes area shows.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NotesDisplay {
    /// Nothing loaded, or the user hid it again: masked placeholder plus the
    /// reveal/copy controls.
    Masked,
    /// The body, reachable only through an explicit reveal.
    Plaintext,
}

/// The display decision, kept out of the widget tree so a test can assert it.
/// Plaintext needs *both* an explicit reveal and a body in the entry: a redacted
/// read leaves the body absent, so a reveal alone is not enough to show one.
pub(crate) fn notes_display(entry: &Entry, revealed: bool) -> NotesDisplay {
    if revealed && entry.notes.is_some() {
        NotesDisplay::Plaintext
    } else {
        NotesDisplay::Masked
    }
}

impl CosmardenApp {
    /// The notes area: an editable buffer while editing, a masked-or-plaintext
    /// read-only body otherwise.
    pub(crate) fn view_notes<'a>(
        &'a self,
        entry: &'a Entry,
        is_editing: bool,
    ) -> Element<'a, Message> {
        if is_editing {
            let editor = text_editor::TextEditor::new(&self.notes_content)
                .on_action(Message::NotesAction)
                .height(Length::Fill);
            return container(editor)
                .height(Length::Fill)
                .padding(5)
                .class(cosmic::theme::Container::Background)
                .into();
        }

        let revealed = self
            .revealed_fields
            .contains(&(entry.id.clone(), NOTES_FIELD.to_string()));

        let body: Element<'a, Message> = match notes_display(entry, revealed) {
            NotesDisplay::Plaintext => {
                text::body(entry.notes.as_ref().map(|n| n.expose()).unwrap_or_default())
                    .width(Length::Fill)
                    .into()
            }
            NotesDisplay::Masked => text::caption(fl!("notes-hidden"))
                .width(Length::Fill)
                .into(),
        };

        let controls = cosmic::widget::row::with_capacity(2)
            .spacing(10)
            .push(
                button::icon(icon::from_name("image-red-eye-symbolic")).on_press(
                    Message::ToggleRevealField(entry.id.clone(), NOTES_FIELD.to_string()),
                ),
            )
            .push(
                button::icon(icon::from_name("edit-copy-symbolic")).on_press(
                    Message::CopySecretField(entry.id.clone(), NOTES_FIELD.to_string()),
                ),
            );

        container(
            cosmic::widget::row::with_capacity(2)
                .spacing(10)
                .align_y(Alignment::Start)
                .push(body)
                .push(controls),
        )
        .height(Length::Fill)
        .padding(5)
        .class(cosmic::theme::Container::Background)
        .into()
    }
}
