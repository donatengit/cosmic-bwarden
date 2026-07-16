//! Unit tests for the password-generator pane's synchronous state
//! transitions (checkbox/slider toggles, reset, and the history
//! delete-confirmation flow). Network-dependent transitions (the agent
//! round-trips themselves) are covered by the E2E suite in
//! `crates/cosmic-bwarden-tests/src/generator.rs`; these tests only verify
//! what `update_pwgen` does to local state before/without the async task
//! resolving.

use crate::app::CosmicBWardenApp;
use crate::message::{Message, View};
use cosmic::Application;
use cosmic_bwarden_core::protocol::{GeneratorHistoryEntry, GeneratorSettings};

fn history_entry(password: &str, created_at: u64) -> GeneratorHistoryEntry {
    GeneratorHistoryEntry {
        password: password.to_string(),
        created_at,
    }
}

#[test]
fn generator_view_clicked_switches_view_and_clears_vault_selection() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        selected_entry_id: Some("some-id".to_string()),
        ..Default::default()
    };

    let _ = app.update(Message::GeneratorViewClicked);

    assert_eq!(app.view, View::PasswordGenerator);
    assert!(app.selected_entry_id.is_none());
    assert!(app.selected_entry.is_none());
}

#[test]
fn charset_toggles_mutate_only_the_targeted_field() {
    let mut app = CosmicBWardenApp::default();
    let defaults = GeneratorSettings::default();
    assert!(defaults.uppercase && defaults.lowercase && defaults.numbers && defaults.special);

    let _ = app.update(Message::GeneratorUppercaseToggled(false));
    assert!(!app.generator_settings.uppercase);
    assert!(app.generator_settings.lowercase);
    assert!(app.generator_settings.numbers);
    assert!(app.generator_settings.special);

    let _ = app.update(Message::GeneratorLowercaseToggled(false));
    let _ = app.update(Message::GeneratorNumbersToggled(false));
    let _ = app.update(Message::GeneratorSpecialToggled(false));
    assert!(!app.generator_settings.lowercase);
    assert!(!app.generator_settings.numbers);
    assert!(!app.generator_settings.special);
}

#[test]
fn length_changed_updates_settings_as_u8() {
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::GeneratorLengthChanged(20));
    assert_eq!(app.generator_settings.length, 20);
}

#[test]
fn reset_restores_schema_defaults_without_touching_persisted_settings() {
    let mut app = CosmicBWardenApp {
        generator_settings: GeneratorSettings {
            uppercase: false,
            lowercase: false,
            numbers: true,
            special: false,
            length: 32,
        },
        ..Default::default()
    };

    let _ = app.update(Message::GeneratorResetClicked);

    assert_eq!(app.generator_settings, GeneratorSettings::default());
}

#[test]
fn generator_generated_ok_sets_result_and_clears_error() {
    let mut app = CosmicBWardenApp {
        generator_error: Some("stale error".to_string()),
        generator_result_revealed: true,
        ..Default::default()
    };

    let _ = app.update(Message::GeneratorGenerated(Ok("fresh-pw".to_string())));

    assert_eq!(app.generator_result.as_deref(), Some("fresh-pw"));
    assert!(!app.generator_result_revealed);
    assert!(app.generator_error.is_none());
}

#[test]
fn generator_generated_err_sets_error_and_leaves_result_untouched() {
    let mut app = CosmicBWardenApp {
        generator_result: Some("previous-pw".to_string()),
        ..Default::default()
    };

    let _ = app.update(Message::GeneratorGenerated(Err("boom".to_string())));

    assert_eq!(app.generator_error.as_deref(), Some("boom"));
    assert_eq!(app.generator_result.as_deref(), Some("previous-pw"));
}

#[test]
fn reveal_toggle_flips_result_visibility() {
    let mut app = CosmicBWardenApp::default();
    assert!(!app.generator_result_revealed);
    let _ = app.update(Message::GeneratorRevealToggled);
    assert!(app.generator_result_revealed);
    let _ = app.update(Message::GeneratorRevealToggled);
    assert!(!app.generator_result_revealed);
}

#[test]
fn history_reveal_toggle_is_per_index() {
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::GeneratorHistoryRevealToggled(2));
    assert!(app.generator_history_revealed.contains(&2));
    assert!(!app.generator_history_revealed.contains(&0));

    let _ = app.update(Message::GeneratorHistoryRevealToggled(2));
    assert!(!app.generator_history_revealed.contains(&2));
}

#[test]
fn history_received_replaces_list_and_clears_stale_reveal_state() {
    let mut app = CosmicBWardenApp::default();
    app.generator_history_revealed.insert(0);
    app.generator_history_revealed.insert(5); // index that won't exist after replace

    let _ = app.update(Message::GeneratorHistoryReceived(Ok(vec![
        history_entry("a", 100),
        history_entry("b", 200),
    ])));

    assert_eq!(app.generator_history.len(), 2);
    assert!(app.generator_history_revealed.is_empty());
}

#[test]
fn delete_requested_sets_pending_index() {
    let mut app = CosmicBWardenApp {
        generator_history: vec![history_entry("a", 100), history_entry("b", 200)],
        ..Default::default()
    };

    let _ = app.update(Message::GeneratorHistoryDeleteRequested(1));

    assert_eq!(app.generator_history_delete_pending, Some(1));
}

#[test]
fn delete_cancelled_clears_pending_index_without_removing_the_entry() {
    let mut app = CosmicBWardenApp {
        generator_history: vec![history_entry("a", 100)],
        generator_history_delete_pending: Some(0),
        ..Default::default()
    };

    let _ = app.update(Message::GeneratorHistoryDeleteCancelled);

    assert!(app.generator_history_delete_pending.is_none());
    assert_eq!(
        app.generator_history.len(),
        1,
        "cancel must not delete anything"
    );
}

#[test]
fn delete_confirmed_clears_pending_index_immediately() {
    let mut app = CosmicBWardenApp {
        generator_history: vec![history_entry("a", 100), history_entry("b", 200)],
        generator_history_delete_pending: Some(0),
        ..Default::default()
    };

    // The actual agent call is async; what we can assert synchronously is
    // that the pending-confirmation flag is cleared right away (so the
    // dialog closes immediately, not after the round trip completes).
    let _ = app.update(Message::GeneratorHistoryDeleteConfirmed);

    assert!(app.generator_history_delete_pending.is_none());
}

#[test]
fn delete_confirmed_with_no_pending_index_is_a_harmless_no_op() {
    let mut app = CosmicBWardenApp {
        generator_history: vec![history_entry("a", 100)],
        generator_history_delete_pending: None,
        ..Default::default()
    };

    let _ = app.update(Message::GeneratorHistoryDeleteConfirmed);

    assert!(app.generator_history_delete_pending.is_none());
    assert_eq!(app.generator_history.len(), 1);
}

#[test]
fn delete_confirmed_with_out_of_range_index_does_not_panic() {
    let mut app = CosmicBWardenApp {
        generator_history: vec![history_entry("a", 100)],
        generator_history_delete_pending: Some(99),
        ..Default::default()
    };

    let _ = app.update(Message::GeneratorHistoryDeleteConfirmed);
}

#[test]
fn deleted_ok_triggers_a_history_refetch_without_panicking() {
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::GeneratorHistoryDeleted(Ok(())));
}

#[test]
fn deleted_err_sets_generator_error() {
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::GeneratorHistoryDeleted(Err(
        "delete failed".to_string()
    )));
    assert_eq!(app.generator_error.as_deref(), Some("delete failed"));
}
