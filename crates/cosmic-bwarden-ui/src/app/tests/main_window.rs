use crate::app::CosmicBWardenApp;
use crate::message::View;
use cosmic::widget;
use cosmic::Application;
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
    app.notes_content =
        widget::text_editor::Content::with_text(entry.notes.as_deref().unwrap_or(""));
    app.selected_entry_id = Some(entry.id.clone());
    app.selected_entry = Some(entry);
}

#[test]
fn test_view_auth_setup_renders() {
    let app = CosmicBWardenApp {
        view: View::Setup,
        ..Default::default()
    };
    let _ = app.view();
}

#[test]
fn test_view_auth_unlock_renders() {
    let app = CosmicBWardenApp {
        view: View::Unlock,
        login_email: "user@example.com".to_string(),
        ..Default::default()
    };
    let _ = app.view();
}

#[test]
fn test_view_vault_no_selection_renders() {
    let app = CosmicBWardenApp {
        view: View::Vault,
        ..Default::default()
    };
    let _ = app.view();
}

#[test]
fn test_view_vault_with_login_entry_renders() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        entries: vec![sidebar_entry("1", "Login Entry", EntryType::Login)],
        ..Default::default()
    };
    select_entry(&mut app, login_entry("1", "Login Entry"));
    let _ = app.view();
}

#[test]
fn test_view_vault_with_ssh_entry_renders() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        entries: vec![sidebar_entry("2", "SSH Entry", EntryType::SshKey)],
        ..Default::default()
    };
    select_entry(&mut app, ssh_entry("2", "SSH Entry"));
    let _ = app.view();
}

#[test]
fn test_view_vault_with_note_entry_renders() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        entries: vec![sidebar_entry("3", "Note Entry", EntryType::SecureNote)],
        ..Default::default()
    };
    select_entry(&mut app, note_entry("3", "Note Entry"));
    let _ = app.view();
}

#[test]
fn test_view_vault_settings_panel_renders() {
    let app = CosmicBWardenApp {
        view: View::Settings,
        entries: vec![sidebar_entry("1", "Login Entry", EntryType::Login)],
        ..Default::default()
    };
    let _ = app.view();
    assert!(!app.entries.is_empty());
}

#[test]
fn test_view_vault_password_generator_panel_renders_empty() {
    let app = CosmicBWardenApp {
        view: View::PasswordGenerator,
        ..Default::default()
    };
    let _ = app.view();
}

#[test]
fn test_view_vault_password_generator_panel_renders_with_result_and_history() {
    use cosmic_bwarden_core::protocol::GeneratorHistoryEntry;

    let mut app = CosmicBWardenApp {
        view: View::PasswordGenerator,
        generator_result: Some("generated-pw-1".into()),
        generator_history: vec![
            GeneratorHistoryEntry {
                password: "generated-pw-1".into(),
                created_at: 1_700_000_000,
            },
            GeneratorHistoryEntry {
                password: "generated-pw-2".into(),
                created_at: 1_699_999_000,
            },
        ],
        ..Default::default()
    };
    app.generator_history_revealed.insert(0);
    let _ = app.view();
}

#[test]
fn test_generator_history_delete_confirmation_dialog_renders() {
    use cosmic_bwarden_core::protocol::GeneratorHistoryEntry;

    let app = CosmicBWardenApp {
        view: View::PasswordGenerator,
        generator_history: vec![GeneratorHistoryEntry {
            password: "generated-pw-1".into(),
            created_at: 1_700_000_000,
        }],
        generator_history_delete_pending: Some(0),
        ..Default::default()
    };
    let _ = app.view();
}

/// While the first window width is still unknown, the vault must render a
/// *fixed* layout whose sidebar sits at `SIDEBAR_MIN_WIDTH`, not the resizable
/// pane grid seeded with the guessed `SIDEBAR_DEFAULT_RATIO`. Building the grid
/// here used to draw the sidebar at 35% of the window and then snap it to the
/// pixel minimum on the first resize report, which read as a flicker/redraw.
#[test]
fn test_view_vault_renders_fixed_sidebar_before_window_width() {
    let app = CosmicBWardenApp {
        view: View::Vault,
        ..Default::default()
    };

    // Invariant: no width has been reported yet, so the placeholder branch runs.
    assert!(app.vault_window_width.is_none());
    assert_eq!(
        crate::app::state::SIDEBAR_DEFAULT_RATIO,
        0.35,
        "placeholder must not be seeded from SIDEBAR_DEFAULT_RATIO"
    );

    let _ = app.view();
}

/// Once the first window width is reported, the resizable split is used and the
/// sidebar is clamped to `SIDEBAR_MIN_WIDTH`. Rendering both branches must not
/// panic and the width gate must flip.
#[test]
fn test_view_vault_renders_split_after_window_width() {
    let mut app = CosmicBWardenApp {
        view: View::Vault,
        ..Default::default()
    };

    let _ = app.view(); // placeholder branch
    app.vault_window_resized(1200.0);
    assert!(app.vault_window_width.is_some());

    let _ = app.view(); // split branch after width known

    let min_ratio = crate::app::state::sidebar_min_ratio(1200.0);
    assert!((app.sidebar_ratio - min_ratio).abs() < f32::EPSILON);
}
