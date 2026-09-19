//! The detail pane's notes slot, driven through the real update path.
//!
//! Notes are a secret-class slot: the agent's `redact_entry_secrets` blanks them
//! in every `GetEntryMeta`/bulk read. So every fixture here is a *redacted*
//! entry — the state the desktop actually receives on selection. Tests that
//! hand-seed `notes_content` or hand-feed a full entry assert a state production
//! never produces, which is how the notes area stayed empty behind a green suite
//! (see docs/testing.md, "Which action does the client send?").

use crate::app::update::vault_actions;
use crate::app::CosmardenApp;
use crate::message::{Message, OnDemandPayload};
use crate::view::vault::notes::{notes_display, NotesDisplay};
use cosmarden_core::db::{Entry, EntryData, Secret};
use cosmarden_core::protocol::Action as AgentAction;
use cosmic::Application;

fn entry(id: &str, name: &str, data: EntryData, notes: Option<&str>) -> Entry {
    Entry {
        id: id.to_string(),
        org_id: None,
        folder: None,
        folder_id: None,
        name: name.to_string(),
        data,
        fields: vec![],
        notes: notes.map(Secret::from),
        history: vec![],
        key: None,
        master_password_reprompt: cosmarden_core::api::CipherRepromptType::None,
        favorite: false,
    }
}

fn login_data() -> EntryData {
    EntryData::Login {
        username: Some("user".to_string()),
        password: None,
        totp: None,
        uris: vec![],
    }
}

/// A login as `GetEntryMeta` answers with it: no password, no notes.
fn redacted_login(id: &str) -> Entry {
    entry(id, "Login", login_data(), None)
}

/// The same entry after an explicit `GetEntry` (reveal, edit, or the SecureNote
/// content fetch): secrets back in place.
fn revealed_login(id: &str) -> Entry {
    let mut e = redacted_login(id);
    e.notes = Some("note body".into());
    e
}

/// A `SecureNote` as `GetEntryMeta` answers with it: the type arrives, the
/// content it carries does not.
fn redacted_secure_note(id: &str) -> Entry {
    entry(id, "Note", EntryData::SecureNote, None)
}

#[test]
fn notes_display_needs_both_a_reveal_and_a_loaded_body() {
    let redacted = redacted_login("1");
    let loaded = revealed_login("1");

    assert_eq!(
        notes_display(&redacted, true),
        NotesDisplay::Masked,
        "a reveal with nothing loaded must stay masked"
    );
    assert_eq!(
        notes_display(&loaded, false),
        NotesDisplay::Masked,
        "a loaded body the user has not revealed must stay masked"
    );
    assert_eq!(notes_display(&loaded, true), NotesDisplay::Plaintext);
}

#[test]
fn a_login_selection_fetches_nothing_beyond_the_meta_read() {
    let mut app = CosmardenApp::default();

    let _ = app.update(Message::EntryReceived(Ok(redacted_login("1"))));

    assert!(
        vault_actions::notes_fetch_after_selection(&redacted_login("1")).is_none(),
        "a login's notes wait for an explicit reveal"
    );
    assert!(
        app.pending_secret_field.is_none(),
        "selection must not start a secret fetch for a login"
    );
}

#[test]
fn a_secure_note_selection_fetches_its_content() {
    // The content *is* the notes, so the one action that brings it back is a
    // full `GetEntry` without a reprompt answer.
    match vault_actions::notes_fetch_after_selection(&redacted_secure_note("7")) {
        Some(AgentAction::GetEntry { id, password }) => {
            assert_eq!(id, "7");
            assert!(password.is_none());
        }
        other => panic!(
            "expected GetEntry, got {:?}",
            other.map(|a| a.variant_name())
        ),
    }

    let mut app = CosmardenApp::default();
    let _ = app.update(Message::EntryReceived(Ok(redacted_secure_note("7"))));
    assert_eq!(app.pending_secret_field.as_deref(), Some("Notes"));
}

#[test]
fn a_loaded_secure_note_is_not_fetched_twice() {
    let loaded = entry("7", "Note", EntryData::SecureNote, Some("already here"));
    assert!(vault_actions::notes_fetch_after_selection(&loaded).is_none());
}

#[test]
fn the_edit_buffer_is_never_seeded_from_a_redacted_read() {
    // Whatever the previous entry left behind must not survive a selection.
    let mut app = CosmardenApp {
        notes_content: cosmic::widget::text_editor::Content::with_text("previous entry notes"),
        ..Default::default()
    };

    let _ = app.update(Message::EntryReceived(Ok(redacted_login("1"))));

    assert_eq!(app.notes_content.text(), "");
    let selected = app.selected_entry.as_ref().expect("entry selected");
    assert!(!vault_actions::secret_is_loaded(selected, "Notes"));
}

#[test]
fn revealing_notes_loads_the_body_and_the_view_shows_it() {
    let mut app = CosmardenApp {
        selected_entry_id: Some("1".to_string()),
        ..Default::default()
    };
    let _ = app.update(Message::EntryReceived(Ok(redacted_login("1"))));

    let _ = app.update(Message::ToggleRevealField("1".into(), "Notes".into()));
    assert_eq!(app.pending_secret_field.as_deref(), Some("Notes"));
    assert_eq!(
        notes_display(app.selected_entry.as_ref().unwrap(), false),
        NotesDisplay::Masked,
        "the body has not arrived yet"
    );

    let _ = app.update(Message::OnDemandSecretLoaded {
        field: "Notes".into(),
        copy: false,
        result: Ok(OnDemandPayload::Entry(Box::new(revealed_login("1")))),
    });

    let selected = app.selected_entry.as_ref().expect("entry selected");
    assert_eq!(
        selected.notes.as_ref().map(|n| n.expose()),
        Some("note body")
    );
    assert!(app.revealed_fields.contains(&("1".into(), "Notes".into())));
    assert_eq!(notes_display(selected, true), NotesDisplay::Plaintext);
    let _ = app.view();
}

#[test]
fn copying_notes_after_a_meta_selection_copies_the_body() {
    let mut app = CosmardenApp {
        selected_entry_id: Some("1".to_string()),
        ..Default::default()
    };
    let _ = app.update(Message::EntryReceived(Ok(redacted_login("1"))));

    let _ = app.update(Message::CopySecretField("1".into(), "Notes".into()));
    assert_eq!(app.pending_secret_field.as_deref(), Some("Notes"));

    let _ = app.update(Message::OnDemandSecretLoaded {
        field: "Notes".into(),
        copy: true,
        result: Ok(OnDemandPayload::Entry(Box::new(revealed_login("1")))),
    });

    assert_eq!(
        app.clipboard_pending_clear.as_deref(),
        Some("note body"),
        "the copy gesture must put the body on the clipboard"
    );
}

#[test]
fn a_secure_note_reprompt_resumes_the_notes_fetch() {
    let mut app = CosmardenApp {
        selected_entry_id: Some("7".to_string()),
        ..Default::default()
    };
    let _ = app.update(Message::EntryReceived(Ok(redacted_secure_note("7"))));

    // A reprompt-gated note answers the content fetch with "reprompt_required".
    let _ = app.update(Message::OnDemandSecretLoaded {
        field: "Notes".into(),
        copy: false,
        result: Err("reprompt_required".into()),
    });
    assert_eq!(app.show_reprompt.as_deref(), Some("7"));
    assert_eq!(
        app.reprompt_intent,
        Some(crate::message::RepromptIntent::Reveal {
            field: "Notes".into()
        })
    );

    match vault_actions::submit_reprompt_action(
        app.reprompt_intent.as_ref(),
        "7".into(),
        "master".into(),
    ) {
        AgentAction::GetEntry { id, password } => {
            assert_eq!(id, "7");
            assert_eq!(password.as_deref(), Some("master"));
        }
        other => panic!("expected GetEntry, got {}", other.variant_name()),
    }
}
