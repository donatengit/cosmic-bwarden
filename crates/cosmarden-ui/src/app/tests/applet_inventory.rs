//! Pins the COSMIC applet popup's user-visible `Message::` wiring to the
//! inventory in `docs/kde_applet_parity.md`. Scans the shipped
//! `view/applet/` sources (via `include_str!`) and the real IPC builders
//! those controls call. A new popup button that is not on the list fails
//! this test until the report is updated.

use crate::app::update::{activation, auth_actions, generator_actions, vault_actions};
use cosmarden_core::protocol::Action as AgentAction;
use std::collections::BTreeSet;

const APPLET_MOD: &str = include_str!("../../view/applet/mod.rs");
const APPLET_MENU: &str = include_str!("../../view/applet/menu.rs");
const APPLET_SEARCH: &str = include_str!("../../view/applet/search.rs");
const APPLET_UNLOCK: &str = include_str!("../../view/applet/unlock.rs");

/// Every `Message::Ident` constructor the applet *views* wire. Must match
/// `docs/kde_applet_parity.md` (inventory tables).
///
/// `Exit` is deliberately absent: the quit row is built from
/// `app::applet_menu::QuitAction::message()`. Settings-only TPM form messages
/// (`TpmSetup*`) never appear in `view/applet/` either.
const INVENTORIED_VIEW_MESSAGES: &[&str] = &[
    "AppletCopyPrimary",
    "AppletCopySecret",
    "AppletGeneratePasswordRequested",
    "AppletIconClicked",
    "AppletOpenInVault",
    "AppletOpenLink",
    "AppletPinChanged",
    "AppletPinSubmitted",
    "AppletRepromptCancelled",
    "AppletRepromptPasswordChanged",
    "AppletRepromptSubmitted",
    "AppletSearchChanged",
    "AppletSearchRowHoverChanged",
    "AppletToggleFavouritesFilter",
    "AppletTogglePinReveal",
    "AppletToggleRepromptPasswordReveal",
    "AppletToggleUnlockPasswordReveal",
    "AppletUnlockPasswordChanged",
    "AppletUnlockSubmitted",
    "AppletUseMasterPasswordInstead",
    "LockClicked",
    "LogoutClicked",
    "OpenVaultRequested",
    "Surface",
    "SyncClicked",
    "UnlockPinChanged",
    "UnlockPinRevealToggled",
];

fn message_idents(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let bytes = src.as_bytes();
    let needle = b"Message::";
    let mut i = 0;
    while i + needle.len() < bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let start = i + needle.len();
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if end > start {
                out.insert(src[start..end].to_string());
            }
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn applet_view_message_inventory_matches_parity_report() {
    let mut found = BTreeSet::new();
    for src in [APPLET_MOD, APPLET_MENU, APPLET_SEARCH, APPLET_UNLOCK] {
        found.extend(message_idents(src));
    }
    let expected: BTreeSet<String> = INVENTORIED_VIEW_MESSAGES
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    assert_eq!(
        found, expected,
        "view/applet Message:: set drifted from docs/kde_applet_parity.md"
    );
}

#[test]
fn applet_ipc_builders_emit_the_protocol_actions_the_popup_sends() {
    match vault_actions::applet_search(Some("mail".into()), false) {
        AgentAction::GetSidebarEntries {
            query,
            entry_type,
            only_pinned,
            domain,
        } => {
            assert_eq!(query.as_deref(), Some("mail"));
            assert!(entry_type.is_none(), "applet lists every entry kind");
            assert!(!only_pinned);
            assert!(domain.is_none());
        }
        other => panic!("expected GetSidebarEntries, got {}", other.variant_name()),
    }

    match vault_actions::fetch_password("id".into(), None) {
        AgentAction::GetPassword { id, password } => {
            assert_eq!(id, "id");
            assert!(password.is_none());
        }
        other => panic!("expected GetPassword, got {}", other.variant_name()),
    }
    match vault_actions::fetch_password("id".into(), Some("master".into())) {
        AgentAction::GetPassword { password, .. } => {
            assert_eq!(password.as_deref(), Some("master"));
        }
        other => panic!("expected GetPassword, got {}", other.variant_name()),
    }

    match generator_actions::generate_with_stored() {
        AgentAction::GeneratePassword { settings: None } => {}
        other => panic!(
            "quick-gen must send GeneratePassword {{ settings: None }}, got {}",
            other.variant_name()
        ),
    }

    assert!(matches!(auth_actions::lock(), AgentAction::Lock));
    assert!(matches!(auth_actions::logout(), AgentAction::Logout));
    match auth_actions::unlock("pw".into()) {
        AgentAction::Unlock { password } => assert_eq!(password, "pw"),
        other => panic!("expected Unlock, got {}", other.variant_name()),
    }
    match auth_actions::unlock_with_pin("1234".into()) {
        AgentAction::UnlockWithPin { pin } => assert_eq!(pin, "1234"),
        other => panic!("expected UnlockWithPin, got {}", other.variant_name()),
    }
    match auth_actions::setup_tpm_pin("1234".into()) {
        AgentAction::SetupTpmPinFromUnlocked { pin } => assert_eq!(pin, "1234"),
        other => panic!(
            "expected SetupTpmPinFromUnlocked, got {}",
            other.variant_name()
        ),
    }
    assert!(matches!(
        auth_actions::disable_tpm_pin(),
        AgentAction::DisableTpmPin
    ));

    assert_eq!(
        activation::classify_activation("open-vault"),
        activation::ActivationAction::SpawnVault
    );
}
