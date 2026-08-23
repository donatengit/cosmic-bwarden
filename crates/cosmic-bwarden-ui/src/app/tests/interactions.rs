use crate::app::update::vault_actions;
use crate::app::CosmicBWardenApp;
use crate::message::Message;
use crate::message::RepromptIntent;
use crate::message::View;
use cosmic::widget;
use cosmic::Application;
use cosmic_bwarden_core::db::{Entry, EntryData};
use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};

fn create_test_entry(id: &str, name: &str) -> Entry {
    Entry {
        id: id.to_string(),
        org_id: None,
        folder: None,
        folder_id: None,
        name: name.to_string(),
        data: EntryData::Login {
            username: Some("old-user".to_string()),
            password: Some("old-pass".to_string().into()),
            totp: None,
            uris: vec![],
        },
        fields: vec![],
        notes: Some("old-notes".into()),
        history: vec![],
        key: None,
        master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
        favorite: false,
    }
}

fn sidebar(
    id: &str,
    name: &str,
    username: Option<&str>,
    ty: EntryType,
    pinned: bool,
) -> SidebarEntry {
    SidebarEntry {
        id: id.to_string(),
        name: name.to_string(),
        username: username.map(str::to_string),
        public_key: None,
        entry_type: ty,
        is_pinned: pinned,
    }
}

/// Press the segmented-control tab corresponding to `filter`, as the sidebar
/// view would (tabs are order-matched to `filter_to_idx`).
fn activate_filter_tab(app: &mut CosmicBWardenApp, filter: Option<EntryType>) {
    let idx = crate::view::vault::sidebar::filter_to_idx(&filter) as u16;
    let entity = app.filter_model.entity_at(idx).expect("filter tab exists");
    let _ = app.update(Message::FilterTabActivated(entity));
}

#[test]
fn test_search_changed_filters_locally_no_ipc() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        ..Default::default()
    };
    let initial_search_id = app.search_id;
    app.all_entries = vec![
        sidebar("1", "GitHub", Some("alice"), EntryType::Login, false),
        sidebar("2", "GitLab", Some("alice"), EntryType::Login, false),
        sidebar("3", "AWS Console", Some("bob"), EntryType::Login, false),
    ];

    let _ = app.update(Message::SearchChanged("git".to_string()));

    assert_eq!(app.search_query, "git");
    assert_eq!(
        app.search_id, initial_search_id,
        "SearchChanged must NOT trigger IPC fetch"
    );
    assert_eq!(
        app.entries.len(),
        2,
        "filter should match GitHub and GitLab"
    );
    assert!(app
        .entries
        .iter()
        .all(|e| e.name.to_lowercase().contains("git")));
}

#[test]
fn test_search_changed_matches_username() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        all_entries: vec![
            sidebar(
                "1",
                "GitHub",
                Some("alice@example.com"),
                EntryType::Login,
                false,
            ),
            sidebar("2", "AWS", Some("bob@corp.com"), EntryType::Login, false),
        ],
        ..Default::default()
    };

    let _ = app.update(Message::SearchChanged("alice".to_string()));

    assert_eq!(app.entries.len(), 1);
    assert_eq!(app.entries[0].id, "1");
}

#[test]
fn test_search_changed_empty_query_shows_all() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.all_entries = vec![
        sidebar("1", "GitHub", None, EntryType::Login, false),
        sidebar("2", "AWS", None, EntryType::Login, false),
    ];
    app.search_query = "git".to_string();
    app.entries = vec![app.all_entries[0].clone()];

    let _ = app.update(Message::SearchChanged(String::new()));

    assert_eq!(app.entries.len(), 2, "empty query must show all entries");
}

#[test]
fn test_filter_tab_filters_locally_no_ipc() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        ..Default::default()
    };
    let initial_search_id = app.search_id;
    app.all_entries = vec![
        sidebar("1", "GitHub", None, EntryType::Login, false),
        sidebar("2", "My Note", None, EntryType::SecureNote, false),
        sidebar("3", "Server Key", None, EntryType::SshKey, false),
    ];

    activate_filter_tab(&mut app, Some(EntryType::SshKey));

    assert_eq!(app.filter_type, Some(EntryType::SshKey));
    assert_eq!(
        app.search_id, initial_search_id,
        "FilterTabActivated must NOT trigger IPC fetch"
    );
    assert_eq!(app.entries.len(), 1);
    assert_eq!(app.entries[0].id, "3");

    activate_filter_tab(&mut app, None);
    assert_eq!(
        app.entries.len(),
        3,
        "clearing filter must show all entries"
    );
}

#[test]
fn test_vault_filtering_and_searching() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        all_entries: vec![
            sidebar("1", "GitHub", Some("alice"), EntryType::Login, false),
            sidebar("2", "AWS Console", Some("bob"), EntryType::Login, false),
            sidebar("3", "Server Key", None, EntryType::SshKey, false),
        ],
        ..Default::default()
    };

    let _ = app.update(Message::SearchChanged("git".to_string()));
    assert_eq!(app.search_query, "git");
    assert_eq!(app.entries.len(), 1);
    assert_eq!(app.entries[0].id, "1");

    activate_filter_tab(&mut app, Some(EntryType::SshKey));
    assert_eq!(app.filter_type, Some(EntryType::SshKey));
    // "git" query + SSH filter → nothing matches
    assert_eq!(app.entries.len(), 0);

    let _ = app.update(Message::SearchChanged(String::new()));
    // no query + SSH filter → only the SSH entry
    assert_eq!(app.entries.len(), 1);
    assert_eq!(app.entries[0].id, "3");
}

#[test]
fn test_entry_field_editing() {
    let mut app = CosmicBWardenApp::default();

    // 1. Login Entry
    let mut entry = create_test_entry("1", "Login");
    app.editing_entry = Some(entry.clone());

    let _ = app.update(Message::EditFieldChanged(
        "Username".to_string(),
        "new-user".to_string(),
    ));
    let _ = app.update(Message::EditFieldChanged(
        "Password".to_string(),
        "new-pass".to_string(),
    ));

    if let EntryData::Login {
        username, password, ..
    } = &app.editing_entry.as_ref().unwrap().data
    {
        assert_eq!(username.as_deref(), Some("new-user"));
        assert_eq!(password.as_deref(), Some("new-pass"));
    }

    // 2. SSH Key Entry
    entry.data = EntryData::SshKey {
        private_key: None,
        public_key: None,
        fingerprint: None,
    };
    app.editing_entry = Some(entry.clone());

    let _ = app.update(Message::EditFieldChanged(
        "Private Key".to_string(),
        "PRIVATE".to_string(),
    ));
    let _ = app.update(Message::EditFieldChanged(
        "Public Key".to_string(),
        "PUBLIC".to_string(),
    ));

    if let EntryData::SshKey {
        private_key,
        public_key,
        ..
    } = &app.editing_entry.as_ref().unwrap().data
    {
        assert_eq!(private_key.as_deref(), Some("PRIVATE"));
        assert_eq!(public_key.as_deref(), Some("PUBLIC"));
    }
}

#[test]
fn test_filter_tab_maps_to_entry_type() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        ..Default::default()
    };

    activate_filter_tab(&mut app, Some(EntryType::Login));
    assert_eq!(app.filter_type, Some(EntryType::Login));

    activate_filter_tab(&mut app, Some(EntryType::SecureNote));
    assert_eq!(app.filter_type, Some(EntryType::SecureNote));

    activate_filter_tab(&mut app, Some(EntryType::SshKey));
    assert_eq!(app.filter_type, Some(EntryType::SshKey));

    activate_filter_tab(&mut app, None);
    assert_eq!(app.filter_type, None);
}

#[test]
fn test_filter_idx_roundtrip() {
    use crate::view::vault::sidebar::{filter_to_idx, idx_to_filter};

    for idx in 0..4 {
        let filter = idx_to_filter(idx);
        assert_eq!(filter_to_idx(&filter), idx);
    }
}

#[test]
fn test_cancel_edit_restores_notes() {
    let mut app = CosmicBWardenApp::default();
    let entry = create_test_entry("1", "Login");
    app.selected_entry_id = Some("1".to_string());
    app.selected_entry = Some(entry.clone());
    app.notes_content = widget::text_editor::Content::with_text("old-notes");

    let _ = app.update(Message::EditEntry);
    let _ = app.update(Message::EditEntryLoaded(Ok(entry.clone())));
    assert!(app.editing_entry.is_some());

    // Simulate the user scratching out the note before hitting Cancel.
    app.notes_content = widget::text_editor::Content::with_text("scratched-out");

    let _ = app.update(Message::CancelEdit);
    assert!(app.editing_entry.is_none());
    assert_eq!(app.notes_content.text().trim(), "old-notes");
}

#[test]
fn test_reprompt_password_reveal_toggle() {
    let mut app = CosmicBWardenApp::default();
    assert!(!app.reprompt_password_revealed);

    let _ = app.update(Message::ToggleRepromptPasswordReveal);
    assert!(app.reprompt_password_revealed);

    let _ = app.update(Message::ToggleRepromptPasswordReveal);
    assert!(!app.reprompt_password_revealed);
}

#[test]
fn test_cancel_reprompt_resets_reveal() {
    let mut app = CosmicBWardenApp {
        show_reprompt: Some("1".to_string()),
        reprompt_password: "secret".to_string(),
        reprompt_password_revealed: true,
        ..Default::default()
    };

    let _ = app.update(Message::CancelReprompt);

    assert!(app.show_reprompt.is_none());
    assert!(app.reprompt_password.is_empty());
    assert!(!app.reprompt_password_revealed);
}

#[test]
fn test_reveal_toggle() {
    let mut app = CosmicBWardenApp::default();
    let id = "entry-1".to_string();
    let field = "Password".to_string();
    let mut entry = create_test_entry(&id, "Login");
    if let EntryData::Login { password, .. } = &mut entry.data {
        *password = Some("loaded".to_string().into());
    }
    app.selected_entry = Some(entry);

    assert!(!app.revealed_fields.contains(&(id.clone(), field.clone())));

    let _ = app.update(Message::ToggleRevealField(id.clone(), field.clone()));
    assert!(app.revealed_fields.contains(&(id.clone(), field.clone())));
    assert!(app.pending_secret_field.is_none());

    let _ = app.update(Message::ToggleRevealField(id.clone(), field.clone()));
    assert!(!app.revealed_fields.contains(&(id.clone(), field.clone())));
}

#[test]
fn reveal_password_on_meta_entry_starts_get_password() {
    let mut app = CosmicBWardenApp::default();
    let mut entry = create_test_entry("1", "Login");
    if let EntryData::Login { password, .. } = &mut entry.data {
        *password = None;
    }
    app.selected_entry = Some(entry);
    app.selected_entry_id = Some("1".into());

    match vault_actions::on_demand_secret("Password", "1".into(), None) {
        cosmic_bwarden_core::protocol::Action::GetPassword { id, password } => {
            assert_eq!(id, "1");
            assert!(password.is_none());
        }
        other => panic!("expected GetPassword, got {}", other.variant_name()),
    }

    let _ = app.update(Message::ToggleRevealField("1".into(), "Password".into()));
    assert_eq!(app.pending_secret_field.as_deref(), Some("Password"));
    assert!(!app
        .revealed_fields
        .contains(&("1".into(), "Password".into())));
}

#[test]
fn copy_secret_on_meta_entry_starts_get_password() {
    let mut app = CosmicBWardenApp::default();
    let mut entry = create_test_entry("1", "Login");
    if let EntryData::Login { password, .. } = &mut entry.data {
        *password = None;
    }
    app.selected_entry = Some(entry);
    app.selected_entry_id = Some("1".into());

    let _ = app.update(Message::CopySecretField("1".into(), "Password".into()));
    assert_eq!(app.pending_secret_field.as_deref(), Some("Password"));
}

#[test]
fn edit_reprompt_sets_intent_and_submit_sends_get_entry() {
    let mut app = CosmicBWardenApp {
        selected_entry_id: Some("1".into()),
        ..Default::default()
    };
    let _ = app.update(Message::EditEntryLoaded(Err("reprompt_required".into())));
    assert_eq!(app.show_reprompt.as_deref(), Some("1"));
    assert_eq!(app.reprompt_intent, Some(RepromptIntent::Edit));

    match vault_actions::submit_reprompt_action(
        app.reprompt_intent.as_ref(),
        "1".into(),
        "master".into(),
    ) {
        cosmic_bwarden_core::protocol::Action::GetEntry { id, password } => {
            assert_eq!(id, "1");
            assert_eq!(password.as_deref(), Some("master"));
        }
        other => panic!("expected GetEntry, got {}", other.variant_name()),
    }

    app.reprompt_password = "master".into();
    let _ = app.update(Message::SubmitReprompt);
    // Submit clears nothing until the agent replies; intent stays Edit so a
    // loaded result is routed to EditEntryLoaded.
    assert_eq!(app.reprompt_intent, Some(RepromptIntent::Edit));
}

#[test]
fn on_demand_password_loaded_patches_entry_and_reveals() {
    let mut app = CosmicBWardenApp::default();
    let mut entry = create_test_entry("1", "Login");
    if let EntryData::Login { password, .. } = &mut entry.data {
        *password = None;
    }
    app.selected_entry = Some(entry);
    app.selected_entry_id = Some("1".into());
    app.pending_secret_field = Some("Password".into());

    let _ = app.update(Message::OnDemandSecretLoaded {
        field: "Password".into(),
        copy: false,
        result: Ok(crate::message::OnDemandPayload::Password("hunter2".into())),
    });
    match &app.selected_entry.as_ref().unwrap().data {
        EntryData::Login { password, .. } => {
            assert_eq!(password.as_ref().map(|s| s.expose()), Some("hunter2"));
        }
        _ => panic!("login"),
    }
    assert!(app
        .revealed_fields
        .contains(&("1".into(), "Password".into())));
    assert!(app.pending_secret_field.is_none());
}

// --- Clipboard auto-clear ([P1-9]) state machine ---

#[test]
fn test_copy_arms_the_autoclear() {
    let mut app = CosmicBWardenApp::default();
    assert_eq!(app.clipboard_clear_generation, 0);

    let _ = app.update(Message::CopyToClipboard("hunter2".to_string()));

    assert_eq!(app.clipboard_clear_generation, 1);
    assert_eq!(app.clipboard_pending_clear.as_deref(), Some("hunter2"));
}

#[test]
fn test_recopy_supersedes_the_pending_clear() {
    let mut app = CosmicBWardenApp::default();

    let _ = app.update(Message::CopyToClipboard("first".to_string()));
    let _ = app.update(Message::CopyToClipboard("second".to_string()));

    assert_eq!(app.clipboard_clear_generation, 2);
    assert_eq!(app.clipboard_pending_clear.as_deref(), Some("second"));

    // The first copy's timer fires with a stale generation: pending must
    // survive untouched for the second copy's own timer.
    let _ = app.update(Message::ClipboardClearElapsed(1));
    assert_eq!(app.clipboard_pending_clear.as_deref(), Some("second"));
}

#[test]
fn test_readback_matching_our_value_resolves_the_clear() {
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::CopyToClipboard("hunter2".to_string()));

    let _ = app.update(Message::ClipboardClearReadback(
        1,
        Some("hunter2".to_string()),
    ));

    assert!(app.clipboard_pending_clear.is_none());
}

#[test]
fn test_readback_with_foreign_content_still_resolves_but_leaves_it() {
    // The user copied something else meanwhile — we must not wipe it, but the
    // pending value must still be dropped (nothing left to clear).
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::CopyToClipboard("hunter2".to_string()));

    let _ = app.update(Message::ClipboardClearReadback(
        1,
        Some("something the user copied".to_string()),
    ));

    assert!(app.clipboard_pending_clear.is_none());
}

#[test]
fn test_stale_readback_does_not_touch_a_newer_copy() {
    let mut app = CosmicBWardenApp::default();
    let _ = app.update(Message::CopyToClipboard("first".to_string()));
    let _ = app.update(Message::CopyToClipboard("second".to_string()));

    let _ = app.update(Message::ClipboardClearReadback(
        1,
        Some("first".to_string()),
    ));

    assert_eq!(app.clipboard_pending_clear.as_deref(), Some("second"));
}

/// The split of the vault window's two-pane grid.
fn vault_split(app: &CosmicBWardenApp) -> widget::pane_grid::Split {
    *app.vault_panes
        .layout()
        .splits()
        .next()
        .expect("vault pane grid has a split")
}

#[test]
fn test_first_window_size_snaps_sidebar_to_min_width() {
    let mut app = CosmicBWardenApp::default();

    app.vault_window_resized(1000.0);

    let expected = crate::app::state::SIDEBAR_MIN_WIDTH / 1000.0;
    assert!((app.sidebar_ratio - expected).abs() < f32::EPSILON);
}

#[test]
fn test_pane_drag_clamps_to_min_width() {
    let mut app = CosmicBWardenApp::default();
    app.vault_window_resized(1000.0);

    let _ = app.update(Message::PaneResized(widget::pane_grid::ResizeEvent {
        split: vault_split(&app),
        ratio: 0.05,
    }));

    let min = crate::app::state::sidebar_min_ratio(1000.0);
    assert!((app.sidebar_ratio - min).abs() < f32::EPSILON);
}

#[test]
fn test_pane_drag_clamps_to_max_ratio() {
    let mut app = CosmicBWardenApp::default();
    app.vault_window_resized(1000.0);

    let _ = app.update(Message::PaneResized(widget::pane_grid::ResizeEvent {
        split: vault_split(&app),
        ratio: 0.95,
    }));

    assert!((app.sidebar_ratio - crate::app::state::SIDEBAR_MAX_RATIO).abs() < f32::EPSILON);
}

#[test]
fn test_window_shrink_keeps_sidebar_at_min_width() {
    let mut app = CosmicBWardenApp::default();
    app.vault_window_resized(1000.0);
    // Widen the sidebar to 40% (= 400px), then shrink the window so 40%
    // would fall below the pixel minimum.
    let _ = app.update(Message::PaneResized(widget::pane_grid::ResizeEvent {
        split: vault_split(&app),
        ratio: 0.4,
    }));

    app.vault_window_resized(700.0);

    let min = crate::app::state::sidebar_min_ratio(700.0);
    assert!((app.sidebar_ratio - min).abs() < f32::EPSILON);
}
