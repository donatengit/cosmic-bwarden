use crate::app::CosmicBWardenApp;
use crate::message::View;
use cosmic::Application;
use cosmic::widget;
use cosmic_bwarden_core::db::{Entry, EntryData};
use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};

fn sidebar_entry(id: &str, name: &str, entry_type: EntryType) -> SidebarEntry {
    SidebarEntry {
        id: id.to_string(),
        name: name.to_string(),
        username: Some("user".to_string()),
        public_key: None,
        entry_type,
        is_pinned: false,
    }
}

fn login_entry(id: &str, name: &str) -> Entry {
    Entry {
        id: id.to_string(),
        org_id: None,
        folder: None,
        folder_id: None,
        name: name.to_string(),
        data: EntryData::Login {
            username: Some("user".to_string()),
            password: Some("pass".to_string().into()),
            totp: None,
            uris: vec![],
        },
        fields: vec![],
        notes: Some("some notes".into()),
        history: vec![],
        key: None,
        master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
        favorite: false,
    }
}

fn ssh_entry(id: &str, name: &str) -> Entry {
    Entry {
        id: id.to_string(),
        org_id: None,
        folder: None,
        folder_id: None,
        name: name.to_string(),
        data: EntryData::SshKey {
            private_key: Some("PRIVATE".to_string().into()),
            public_key: Some("PUBLIC".to_string()),
            fingerprint: None,
        },
        fields: vec![],
        notes: None,
        history: vec![],
        key: None,
        master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
        favorite: false,
    }
}

fn note_entry(id: &str, name: &str) -> Entry {
    Entry {
        id: id.to_string(),
        org_id: None,
        folder: None,
        folder_id: None,
        name: name.to_string(),
        data: EntryData::SecureNote,
        fields: vec![],
        notes: Some("note body".into()),
        history: vec![],
        key: None,
        master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
        favorite: false,
    }
}

fn select_entry(app: &mut CosmicBWardenApp, entry: Entry) {
    app.notes_content = widget::text_editor::Content::with_text(entry.notes.as_deref().unwrap_or(""));
    app.selected_entry_id = Some(entry.id.clone());
    app.selected_entry = Some(entry);
}

#[test]
fn test_view_auth_setup_renders() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Setup;
    let _ = app.view();
}

#[test]
fn test_view_auth_unlock_renders() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Unlock;
    app.login_email = "user@example.com".to_string();
    let _ = app.view();
}

#[test]
fn test_view_vault_no_selection_renders() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    let _ = app.view();
}

#[test]
fn test_view_vault_with_login_entry_renders() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.entries = vec![sidebar_entry("1", "Login Entry", EntryType::Login)];
    select_entry(&mut app, login_entry("1", "Login Entry"));
    let _ = app.view();
}

#[test]
fn test_view_vault_with_ssh_entry_renders() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.entries = vec![sidebar_entry("2", "SSH Entry", EntryType::SshKey)];
    select_entry(&mut app, ssh_entry("2", "SSH Entry"));
    let _ = app.view();
}

#[test]
fn test_view_vault_with_note_entry_renders() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.entries = vec![sidebar_entry("3", "Note Entry", EntryType::SecureNote)];
    select_entry(&mut app, note_entry("3", "Note Entry"));
    let _ = app.view();
}

#[test]
fn test_view_vault_settings_panel_renders() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Settings;
    app.entries = vec![sidebar_entry("1", "Login Entry", EntryType::Login)];
    let _ = app.view();
    assert!(!app.entries.is_empty());
}
