use crate::app::CosmicBWardenApp;
use crate::message::Message;
use crate::message::View;
use cosmic::Application;
use cosmic::widget;
use cosmic_bwarden_core::db::{Entry, EntryData};
use cosmic_bwarden_core::protocol::EntryType;

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

#[test]
fn test_vault_filtering_and_searching() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;

    // 1. Search Query
    let _ = app.update(Message::SearchChanged("my search".to_string()));
    assert_eq!(app.search_query, "my search");
    assert_eq!(app.search_id, 1);

    // 2. Filter Type
    let _ = app.update(Message::FilterTypeChanged(Some(EntryType::SshKey)));
    assert_eq!(app.filter_type, Some(EntryType::SshKey));
    assert_eq!(app.search_id, 2);

    // 3. Search Submitted
    let _ = app.update(Message::SearchSubmitted("final query".to_string()));
    assert_eq!(app.search_id, 3);
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
