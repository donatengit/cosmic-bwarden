//! Pure StatusNotifierItem menu model. No ksni types — unit-tested without
//! a tray host. Labels go through `fl!`; tests assert commands, not copy.

use crate::app::applet_search::{AppletRow, AppletRowKind};
use crate::fl;
use crate::message::Message;

/// One user-visible action from the KDE tray menu (or left-click).
/// Secret inputs are not DBusMenu widgets — Unlock opens the compact popup
/// that already has `secure_input` (`view/applet/unlock.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayCommand {
    /// Left-click / Unlock: show the compact applet popup (search + PIN/password).
    TogglePopup,
    Generate,
    Lock,
    Logout,
    Exit,
    CopyPrimary(String),
    CopySecret(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuNode {
    Item {
        label: String,
        command: TrayCommand,
        enabled: bool,
    },
    SubMenu {
        label: String,
        children: Vec<MenuNode>,
    },
    Separator,
}

/// Chrome commands (no vault rows): protocol mismatch, not-configured, locked,
/// unlocked.
///
/// Exit is the only session-ending row, matching the applet popup's footer
/// (`app::applet_menu::quit_actions`): native menus expose one Quit row, and
/// the Lock/Logout entries above it leave the tray running.
pub fn chrome_commands(protocol_mismatch: bool, setup: bool, unlocked: bool) -> Vec<TrayCommand> {
    if protocol_mismatch {
        return vec![TrayCommand::Exit];
    }
    if setup {
        // No account yet: nothing to unlock, lock or log out of.
        return vec![TrayCommand::Generate, TrayCommand::Exit];
    }
    let mut out = Vec::new();
    if !unlocked {
        out.push(TrayCommand::TogglePopup);
    }
    out.push(TrayCommand::Generate);
    if unlocked {
        out.push(TrayCommand::Lock);
        out.push(TrayCommand::Logout);
    }
    out.push(TrayCommand::Exit);
    out
}

pub fn copy_commands(row: &AppletRow) -> Vec<TrayCommand> {
    match &row.kind {
        AppletRowKind::Login { username, .. } => {
            let mut cmds = Vec::new();
            if username.is_some() {
                cmds.push(TrayCommand::CopyPrimary(row.id.clone()));
            }
            cmds.push(TrayCommand::CopySecret(row.id.clone()));
            cmds
        }
        AppletRowKind::SecureNote | AppletRowKind::SshKey => {
            vec![TrayCommand::CopySecret(row.id.clone())]
        }
    }
}

pub fn build_menu(
    protocol_mismatch: bool,
    setup: bool,
    unlocked: bool,
    rows: &[AppletRow],
) -> Vec<MenuNode> {
    if protocol_mismatch {
        return vec![item(fl!("quit"), TrayCommand::Exit, true)];
    }

    let mut nodes = Vec::new();
    if unlocked {
        for row in rows {
            let commands = copy_commands(row);
            if let [TrayCommand::CopySecret(id)] = commands.as_slice() {
                // A submenu for a single action reads as noise.
                nodes.push(item(
                    format!("{} — {}", row.label, fl!("tray-copy-secret")),
                    TrayCommand::CopySecret(id.clone()),
                    true,
                ));
                continue;
            }
            let children = commands
                .into_iter()
                .map(|cmd| {
                    let label = match &cmd {
                        TrayCommand::CopyPrimary(_) => fl!("tray-copy-username"),
                        TrayCommand::CopySecret(_) => match row.kind {
                            AppletRowKind::Login { .. } => fl!("tray-copy-password"),
                            AppletRowKind::SecureNote => fl!("tray-copy-note"),
                            AppletRowKind::SshKey => fl!("tray-copy-key"),
                        },
                        _ => fl!("tray-copy-secret"),
                    };
                    item(label, cmd, true)
                })
                .collect();
            nodes.push(MenuNode::SubMenu {
                label: row.label.clone(),
                children,
            });
        }
        if !rows.is_empty() {
            nodes.push(MenuNode::Separator);
        }
    } else if !setup {
        nodes.push(item(fl!("tray-unlock"), TrayCommand::TogglePopup, true));
    }

    for cmd in chrome_commands(protocol_mismatch, setup, unlocked) {
        // Already added above, so that Unlock leads a locked menu.
        if matches!(cmd, TrayCommand::TogglePopup) && !unlocked && !setup {
            continue;
        }
        nodes.push(item(chrome_label(&cmd), cmd, true));
    }
    nodes
}

fn chrome_label(cmd: &TrayCommand) -> String {
    match cmd {
        TrayCommand::TogglePopup => fl!("tray-unlock"),
        TrayCommand::Generate => fl!("generate-password"),
        TrayCommand::Lock => fl!("lock"),
        TrayCommand::Logout => fl!("logout"),
        TrayCommand::Exit => fl!("quit"),
        TrayCommand::CopyPrimary(_) => fl!("tray-copy-username"),
        TrayCommand::CopySecret(_) => fl!("tray-copy-secret"),
    }
}

fn item(label: String, command: TrayCommand, enabled: bool) -> MenuNode {
    MenuNode::Item {
        label,
        command,
        enabled,
    }
}

/// Map a tray command onto the applet popup's existing handlers, so the tray
/// and the panel share one implementation of each action.
pub fn command_to_message(cmd: TrayCommand) -> Message {
    match cmd {
        // Re-enters `update_app`, which routes it to `kde::handle_tray_command`.
        TrayCommand::TogglePopup => Message::KdeTray(TrayCommand::TogglePopup),
        TrayCommand::Generate => Message::AppletGeneratePasswordRequested,
        TrayCommand::Lock => Message::LockClicked,
        TrayCommand::Logout => Message::LogoutClicked,
        TrayCommand::Exit => Message::Exit,
        TrayCommand::CopyPrimary(id) => Message::AppletCopyPrimary(id),
        TrayCommand::CopySecret(id) => Message::AppletCopySecret(id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::applet_search::AppletRowKind;

    fn login(id: &str, user: Option<&str>) -> AppletRow {
        AppletRow {
            id: id.into(),
            label: "example.com".into(),
            kind: AppletRowKind::Login {
                username: user.map(str::to_string),
                link: None,
            },
        }
    }

    fn node_labels(nodes: &[MenuNode]) -> Vec<String> {
        nodes
            .iter()
            .filter_map(|n| match n {
                MenuNode::Item { label, .. } => Some(label.clone()),
                MenuNode::SubMenu { label, .. } => Some(label.clone()),
                MenuNode::Separator => None,
            })
            .collect()
    }

    #[test]
    fn locked_chrome_opens_popup_for_secret_input_and_does_not_lock() {
        let cmds = chrome_commands(false, false, false);
        assert!(cmds.contains(&TrayCommand::TogglePopup));
        assert!(cmds.contains(&TrayCommand::Generate));
        assert!(!cmds.contains(&TrayCommand::Lock));
        assert!(cmds.contains(&TrayCommand::Exit));
    }

    #[test]
    fn unlocked_chrome_has_lock_logout_and_copy_targets() {
        let cmds = chrome_commands(false, false, true);
        assert!(cmds.contains(&TrayCommand::Lock));
        assert!(cmds.contains(&TrayCommand::Logout));
        assert!(cmds.contains(&TrayCommand::Generate));
        assert!(!cmds.contains(&TrayCommand::TogglePopup));
    }

    #[test]
    fn setup_chrome_offers_no_session_actions() {
        assert_eq!(
            chrome_commands(false, true, false),
            vec![TrayCommand::Generate, TrayCommand::Exit]
        );
    }

    /// The popup footer is one Quit row (`app::applet_menu::quit_actions`);
    /// the tray menu must not grow a lock/logout-and-quit cluster back.
    #[test]
    fn tray_chrome_has_no_quit_cluster() {
        let mut labels: Vec<String> = Vec::new();
        for (setup, unlocked) in [(false, false), (false, true), (true, false)] {
            labels.extend(node_labels(&build_menu(false, setup, unlocked, &[])));
        }
        for banned in [fl!("lock-and-quit"), fl!("logout-and-quit")] {
            assert!(
                !labels.contains(&banned),
                "{banned:?} must not appear in the tray menu: {labels:?}"
            );
        }
    }

    #[test]
    fn tray_chrome_never_opens_the_vault_window() {
        for (mismatch, setup, unlocked) in [
            (false, false, false),
            (false, false, true),
            (false, true, false),
            (true, false, true),
        ] {
            let labels = node_labels(&build_menu(mismatch, setup, unlocked, &[]));
            assert!(
                !labels.iter().any(|l| l == &fl!("open-vault-window")),
                "agent-only tray must not offer Open Vault (mismatch={mismatch} setup={setup} unlocked={unlocked})"
            );
        }
    }

    #[test]
    fn protocol_mismatch_is_quit_only() {
        assert_eq!(chrome_commands(true, false, true), vec![TrayCommand::Exit]);
    }

    #[test]
    fn login_row_copies_username_then_password() {
        let cmds = copy_commands(&login("id1", Some("alice")));
        assert_eq!(
            cmds,
            vec![
                TrayCommand::CopyPrimary("id1".into()),
                TrayCommand::CopySecret("id1".into()),
            ]
        );
    }

    #[test]
    fn login_without_username_only_copies_secret() {
        assert_eq!(
            copy_commands(&login("id1", None)),
            vec![TrayCommand::CopySecret("id1".into())]
        );
    }

    #[test]
    fn single_action_rows_render_as_flat_items() {
        let nodes = build_menu(false, false, true, &[login("id1", None)]);
        assert!(
            matches!(
                &nodes[0],
                MenuNode::Item {
                    command: TrayCommand::CopySecret(id),
                    ..
                } if id == "id1"
            ),
            "a row with one action must not open a submenu: {nodes:?}"
        );
    }

    #[test]
    fn command_to_message_uses_existing_applet_copy_actions() {
        assert!(matches!(
            command_to_message(TrayCommand::CopySecret("x".into())),
            Message::AppletCopySecret(id) if id == "x"
        ));
        assert!(matches!(
            command_to_message(TrayCommand::Generate),
            Message::AppletGeneratePasswordRequested
        ));
    }

    #[test]
    fn build_menu_locked_leads_with_unlock() {
        let nodes = build_menu(false, false, false, &[]);
        assert!(
            matches!(
                &nodes[0],
                MenuNode::Item {
                    command: TrayCommand::TogglePopup,
                    ..
                }
            ),
            "first item must open the popup so PIN/password fields are available"
        );
    }
}
