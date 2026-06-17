use crate::app::CosmicBWardenApp;
use crate::message::Message;
use crate::message::View;
use cosmic::Application;
use cosmic_bwarden_core::db::{Entry, EntryData};

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

#[tokio::test]
async fn test_auth_window_transition() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Unlock;

    // Successful auth
    let _ = app.update(Message::AuthResult(Ok(())));

    assert_eq!(app.view, View::Vault);
}

#[tokio::test]
async fn test_reactive_events() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.selected_entry_id = Some("1".to_string());

    // 1. Receive Locked event
    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::Locked,
    ));
    assert_eq!(app.view, View::Unlock);
    assert!(app.selected_entry_id.is_none());

    // 2. Receive Unlocked event
    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::Unlocked,
    ));
    assert_eq!(app.view, View::Vault);
}

#[tokio::test]
async fn test_unlock_requested_event_shows_unlock_view() {
    let mut app = CosmicBWardenApp::default();
    app.view = View::Vault;
    app.selected_entry_id = Some("1".to_string());

    let _ = app.update(Message::EventReceived(
        cosmic_bwarden_core::protocol::Event::UnlockRequested,
    ));
    assert_eq!(app.view, View::Unlock);
    assert!(app.selected_entry_id.is_none());
}
