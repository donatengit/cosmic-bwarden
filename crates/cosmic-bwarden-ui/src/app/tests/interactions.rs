use crate::app::CosmicBWardenApp;
use crate::message::Message;
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

#[test]
fn test_search_changed_filters_locally_no_ipc() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
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
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.all_entries = vec![
        sidebar(
            "1",
            "GitHub",
            Some("alice@example.com"),
            EntryType::Login,
            false,
        ),
        sidebar("2", "AWS", Some("bob@corp.com"), EntryType::Login, false),
    ];

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
fn test_filter_type_changed_filters_locally_no_ipc() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    let initial_search_id = app.search_id;
    app.all_entries = vec![
        sidebar("1", "GitHub", None, EntryType::Login, false),
        sidebar("2", "My Note", None, EntryType::SecureNote, false),
        sidebar("3", "Server Key", None, EntryType::SshKey, false),
    ];

    let _ = app.update(Message::FilterTypeChanged(Some(EntryType::SshKey)));

    assert_eq!(app.filter_type, Some(EntryType::SshKey));
    assert_eq!(
        app.search_id, initial_search_id,
        "FilterTypeChanged must NOT trigger IPC fetch"
    );
    assert_eq!(app.entries.len(), 1);
    assert_eq!(app.entries[0].id, "3");

    let _ = app.update(Message::FilterTypeChanged(None));
    assert_eq!(
        app.entries.len(),
        3,
        "clearing filter must show all entries"
    );
}

#[test]
fn test_vault_filtering_and_searching() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.all_entries = vec![
        sidebar("1", "GitHub", Some("alice"), EntryType::Login, false),
        sidebar("2", "AWS Console", Some("bob"), EntryType::Login, false),
        sidebar("3", "Server Key", None, EntryType::SshKey, false),
    ];

    let _ = app.update(Message::SearchChanged("git".to_string()));
    assert_eq!(app.search_query, "git");
    assert_eq!(app.entries.len(), 1);
    assert_eq!(app.entries[0].id, "1");

    let _ = app.update(Message::FilterTypeChanged(Some(EntryType::SshKey)));
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
fn test_filter_type_changed_maps_to_entry_type() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;

    let _ = app.update(Message::FilterTypeChanged(Some(EntryType::Login)));
    assert_eq!(app.filter_type, Some(EntryType::Login));

    let _ = app.update(Message::FilterTypeChanged(Some(EntryType::SecureNote)));
    assert_eq!(app.filter_type, Some(EntryType::SecureNote));

    let _ = app.update(Message::FilterTypeChanged(Some(EntryType::SshKey)));
    assert_eq!(app.filter_type, Some(EntryType::SshKey));

    let _ = app.update(Message::FilterTypeChanged(None));
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
    let mut app = CosmicBWardenApp::default();
    app.show_reprompt = Some("1".to_string());
    app.reprompt_password = "secret".to_string();
    app.reprompt_password_revealed = true;

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

    assert!(!app.revealed_fields.contains(&(id.clone(), field.clone())));

    // Toggle On
    let _ = app.update(Message::ToggleRevealField(id.clone(), field.clone()));
    assert!(app.revealed_fields.contains(&(id.clone(), field.clone())));

    // Toggle Off
    let _ = app.update(Message::ToggleRevealField(id.clone(), field.clone()));
    assert!(!app.revealed_fields.contains(&(id.clone(), field.clone())));
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
