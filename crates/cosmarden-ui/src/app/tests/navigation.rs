//! The discard guard around vault navigation: with an edit draft open, Add,
//! Settings, the generator and picking another entry ask before dropping it,
//! and the parked destination runs only after the user confirms.

use crate::app::CosmardenApp;
use crate::message::{Message, View};
use cosmic::Application;

fn app_with_draft() -> CosmardenApp {
    let mut app = CosmardenApp::default();
    let _ = app.update(Message::AddEntryRequested);
    assert!(app.editing_entry.is_some(), "Add must open the editor");
    assert!(app.pending_navigation.is_none());
    app
}

#[test]
fn navigation_with_an_open_draft_is_parked_for_confirmation() {
    for message in [
        Message::SelectEntry("other".into()),
        Message::AddEntryRequested,
        Message::SettingsViewClicked,
        Message::GeneratorViewClicked,
    ] {
        let mut app = app_with_draft();

        let _ = app.update(message.clone());

        assert_eq!(app.view, View::Vault, "{message:?} ran without asking");
        assert!(
            app.pending_navigation.is_some(),
            "{message:?} was not parked"
        );
        assert!(
            app.editing_entry.is_some(),
            "{message:?} dropped the draft before confirmation"
        );
        assert!(
            app.selected_entry_id.is_none(),
            "{message:?} changed the selection before confirmation"
        );
    }
}

#[test]
fn confirming_drops_the_draft_and_runs_the_parked_navigation() {
    let mut app = app_with_draft();
    let _ = app.update(Message::GeneratorViewClicked);
    assert!(app.pending_navigation.is_some());

    let _ = app.update(Message::DiscardEditConfirmed);

    assert!(
        app.pending_navigation.is_none(),
        "the slot must be consumed"
    );
    assert!(app.editing_entry.is_none(), "the draft must be dropped");
    assert_eq!(
        app.view,
        View::PasswordGenerator,
        "the parked pane must open"
    );
}

#[test]
fn confirming_a_parked_selection_opens_that_entry() {
    let mut app = app_with_draft();
    let _ = app.update(Message::SelectEntry("other".into()));

    let _ = app.update(Message::DiscardEditConfirmed);

    assert!(app.editing_entry.is_none());
    assert_eq!(app.selected_entry_id.as_deref(), Some("other"));
    assert_eq!(app.view, View::Vault);
}

#[test]
fn cancelling_keeps_the_draft_and_the_pane() {
    let mut app = app_with_draft();
    let _ = app.update(Message::SettingsViewClicked);

    let _ = app.update(Message::DiscardEditCancelled);

    assert!(app.pending_navigation.is_none());
    assert!(app.editing_entry.is_some(), "cancel must keep the draft");
    assert_eq!(app.view, View::Vault, "cancel must keep the editor open");
}

#[test]
fn navigation_without_a_draft_is_immediate() {
    let mut app = CosmardenApp {
        view: View::Vault,
        ..Default::default()
    };

    let _ = app.update(Message::SettingsViewClicked);

    assert_eq!(app.view, View::Settings);
    assert!(app.pending_navigation.is_none());
}

#[test]
fn typing_does_not_arm_the_guard() {
    // Only navigation arms it; an edit message must never raise the dialog.
    let mut app = app_with_draft();

    let _ = app.update(Message::EditNameChanged("Renamed".into()));

    assert!(app.pending_navigation.is_none());
    assert!(app.editing_entry.is_some());
}
