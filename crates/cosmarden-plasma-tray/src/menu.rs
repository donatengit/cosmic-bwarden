//! The tray's menu: one list, one submenu per login, no popup.
//!
//! Labels are English literals rather than `fl!` strings. The Fluent bundle and
//! its loader live in `cosmarden-ui`, which is exactly the dependency this
//! binary exists to avoid, and `en` is the only locale the project ships — so
//! the only thing a literal costs today is a second place to edit if a locale
//! is ever added.
//!
//! Every decision here is pure and unit-tested: the menu a user sees is the menu
//! these functions return, with no host involvement.

use crate::state::{LoginRow, TrayState};
use cosmarden_core::protocol::Action;

/// A user-visible menu action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuCommand {
    /// Start the full app. The only way to unlock from this process.
    OpenVault,
    /// Copy the stored username. It is a sidebar field the agent already
    /// returned, so this needs no round-trip.
    CopyLogin(String),
    /// Fetch the stored password (`GetPassword`) and copy it.
    CopyPassword(String),
    /// Open a login's site in the default browser. Carries the URI built by
    /// `domain::open_uri_for_name`.
    OpenLink(String),
    /// `Action::Lock` — the agent keeps running, the vault is locked.
    Lock,
    /// `Action::Logout` — drop the session and the stored tokens.
    Logout,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuNode {
    Item {
        label: String,
        command: MenuCommand,
    },
    /// A login's actions. The host renders it as a nested menu.
    SubMenu {
        label: String,
        children: Vec<MenuNode>,
    },
    Separator,
}

/// The whole menu, in order.
///
/// `Open Vault` and `Exit` are always present — including when the agent is
/// down, not configured, or a protocol mismatch: the full app is where all of
/// that is explained and fixed. Login rows only exist while the vault is
/// unlocked, because they come from `GetSidebarEntries`.
pub fn build_menu(state: &TrayState) -> Vec<MenuNode> {
    if state.protocol_mismatch {
        return vec![item("Exit", MenuCommand::Exit)];
    }

    let mut nodes = vec![item("Open Vault", MenuCommand::OpenVault)];
    if !state.rows.is_empty() {
        nodes.push(MenuNode::Separator);
        for row in &state.rows {
            nodes.push(row_node(row));
        }
    }
    if state.unlocked() {
        nodes.push(MenuNode::Separator);
        nodes.push(item("Lock", MenuCommand::Lock));
        nodes.push(item("Logout", MenuCommand::Logout));
    }
    nodes.push(MenuNode::Separator);
    nodes.push(item("Exit", MenuCommand::Exit));
    nodes
}

/// One login's entries, in the order a click is most often for: `Copy password`
/// first, then `Copy login` when a username is stored, then `Open link` when the
/// name is URI-like.
///
/// A row with a single action renders as one flat item (`"facebook.com — Copy
/// password"`) instead of a submenu with one child — the rule the retired
/// libcosmic tray used, kept because a one-child submenu is noise.
fn row_node(row: &LoginRow) -> MenuNode {
    let mut actions: Vec<(&str, MenuCommand)> = Vec::new();
    actions.push(("Copy password", MenuCommand::CopyPassword(row.id.clone())));
    if row.username.is_some() {
        actions.push(("Copy login", MenuCommand::CopyLogin(row.id.clone())));
    }
    if let Some(uri) = &row.uri {
        actions.push(("Open link", MenuCommand::OpenLink(uri.clone())));
    }

    if actions.len() == 1 {
        let (action, command) = actions.pop().expect("exactly one action");
        return item(format!("{} — {action}", row.label), command);
    }
    MenuNode::SubMenu {
        label: row.label.clone(),
        children: actions
            .into_iter()
            .map(|(label, command)| item(label, command))
            .collect(),
    }
}

/// The agent action a command sends, if it sends one.
///
/// Pure on purpose: an action selected inside the async dispatcher is
/// unreachable from any test, so a wrong variant stays invisible until it hits
/// the agent (docs/testing.md).
pub fn agent_action(command: &MenuCommand) -> Option<Action> {
    match command {
        MenuCommand::Lock => Some(Action::Lock),
        MenuCommand::Logout => Some(Action::Logout),
        // No master password is available here: a reprompt-gated entry answers
        // `reprompt_required`, and the tray has no way to ask for one. The
        // dispatcher logs that and points at `Open Vault`.
        MenuCommand::CopyPassword(id) => Some(Action::GetPassword {
            id: id.clone(),
            password: None,
        }),
        // Local hand-offs, a local exit, and a copy served from the sidebar
        // snapshot send nothing here.
        MenuCommand::OpenVault | MenuCommand::CopyLogin(_) | MenuCommand::OpenLink(_) => None,
        MenuCommand::Exit => None,
    }
}

fn item(label: impl Into<String>, command: MenuCommand) -> MenuNode {
    MenuNode::Item {
        label: label.into(),
        command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, label: &str, username: Option<&str>, uri: Option<&str>) -> LoginRow {
        LoginRow {
            id: id.to_string(),
            label: label.to_string(),
            username: username.map(str::to_string),
            uri: uri.map(str::to_string),
        }
    }

    fn unlocked_with(rows: Vec<LoginRow>) -> TrayState {
        TrayState {
            agent_ready: true,
            has_account: true,
            locked: false,
            rows,
            ..Default::default()
        }
    }

    fn labels(nodes: &[MenuNode]) -> Vec<String> {
        nodes
            .iter()
            .filter_map(|n| match n {
                MenuNode::Item { label, .. } | MenuNode::SubMenu { label, .. } => {
                    Some(label.clone())
                }
                MenuNode::Separator => None,
            })
            .collect()
    }

    fn commands(nodes: &[MenuNode]) -> Vec<MenuCommand> {
        nodes
            .iter()
            .filter_map(|n| match n {
                MenuNode::Item { command, .. } => Some(command.clone()),
                MenuNode::SubMenu { .. } | MenuNode::Separator => None,
            })
            .collect()
    }

    /// The node a user would click by name, so a test does not depend on how
    /// many chrome rows and separators sit in front of it.
    fn node_labelled<'a>(nodes: &'a [MenuNode], label: &str) -> &'a MenuNode {
        nodes
            .iter()
            .find(|node| match node {
                MenuNode::Item { label: l, .. } | MenuNode::SubMenu { label: l, .. } => l == label,
                MenuNode::Separator => false,
            })
            .unwrap_or_else(|| panic!("no node labelled {label:?} in {nodes:?}"))
    }

    fn children_of(node: &MenuNode) -> Vec<String> {
        match node {
            MenuNode::SubMenu { children, .. } => labels(children),
            other => panic!("expected a submenu, got {other:?}"),
        }
    }

    fn child_commands(node: &MenuNode) -> Vec<MenuCommand> {
        match node {
            MenuNode::SubMenu { children, .. } => commands(children),
            other => panic!("expected a submenu, got {other:?}"),
        }
    }

    /// The shape that was asked for: one submenu per login, password first.
    #[test]
    fn a_login_row_is_a_submenu_with_copy_password_then_the_rest() {
        let nodes = build_menu(&unlocked_with(vec![row(
            "id1",
            "facebook.com",
            Some("alice"),
            Some("https://facebook.com"),
        )]));
        let row_node = node_labelled(&nodes, "facebook.com");
        assert_eq!(
            children_of(row_node),
            vec!["Copy password", "Copy login", "Open link"],
            "the password is the action a tray click is most often for"
        );
        assert_eq!(
            child_commands(row_node),
            vec![
                MenuCommand::CopyPassword("id1".to_string()),
                MenuCommand::CopyLogin("id1".to_string()),
                MenuCommand::OpenLink("https://facebook.com".to_string()),
            ]
        );
    }

    #[test]
    fn open_link_is_omitted_when_the_name_is_not_uri_like() {
        let nodes = build_menu(&unlocked_with(vec![row(
            "id1",
            "My Bank",
            Some("alice"),
            None,
        )]));
        assert_eq!(
            children_of(node_labelled(&nodes, "My Bank")),
            vec!["Copy password", "Copy login"],
            "nothing to open, so no dead row"
        );
    }

    #[test]
    fn copy_login_is_omitted_when_no_username_is_stored() {
        let nodes = build_menu(&unlocked_with(vec![row(
            "id1",
            "facebook.com",
            None,
            Some("https://facebook.com"),
        )]));
        assert_eq!(
            children_of(node_labelled(&nodes, "facebook.com")),
            vec!["Copy password", "Open link"]
        );
    }

    /// A login with neither a username nor a site has one action: a flat item
    /// rather than a submenu with one child.
    #[test]
    fn a_single_action_row_renders_flat() {
        let nodes = build_menu(&unlocked_with(vec![row("id1", "My Bank", None, None)]));
        assert!(
            matches!(
                node_labelled(&nodes, "My Bank — Copy password"),
                MenuNode::Item {
                    label,
                    command: MenuCommand::CopyPassword(id),
                } if label == "My Bank — Copy password" && id == "id1"
            ),
            "expected a flat item, got {:?}",
            nodes[1]
        );
    }

    #[test]
    fn protocol_mismatch_offers_only_exit() {
        let state = TrayState {
            protocol_mismatch: true,
            ..unlocked_with(vec![row("id1", "facebook.com", None, None)])
        };
        assert_eq!(build_menu(&state), vec![item("Exit", MenuCommand::Exit)]);
    }

    /// No agent, no account, locked: the same two rows. Everything else the
    /// user might need is in the app `Open Vault` starts.
    #[test]
    fn every_state_without_a_vault_offers_open_vault_and_exit() {
        let states = [
            TrayState::default(),
            TrayState {
                agent_ready: true,
                has_account: false,
                ..Default::default()
            },
            TrayState {
                agent_ready: true,
                has_account: true,
                locked: true,
                ..Default::default()
            },
        ];
        for state in states {
            let nodes = build_menu(&state);
            assert_eq!(
                labels(&nodes),
                vec!["Open Vault", "Exit"],
                "state {state:?} must offer the app and a way out only"
            );
            assert!(
                !commands(&nodes)
                    .iter()
                    .any(|c| matches!(c, MenuCommand::Lock | MenuCommand::Logout)),
                "a locked vault cannot be locked or logged out from here: {state:?}"
            );
        }
    }

    #[test]
    fn unlocked_menu_lists_rows_then_lock_logout_and_exit() {
        let nodes = build_menu(&unlocked_with(vec![
            row(
                "1",
                "facebook.com",
                Some("alice"),
                Some("https://facebook.com"),
            ),
            row("2", "My Bank", Some("bob"), None),
        ]));
        assert_eq!(
            labels(&nodes),
            vec![
                "Open Vault",
                "facebook.com",
                "My Bank",
                "Lock",
                "Logout",
                "Exit"
            ]
        );
    }

    /// An unlocked vault with no pinned login to show: no empty section, no
    /// dangling separator.
    #[test]
    fn unlocked_menu_without_rows_has_no_row_section() {
        let nodes = build_menu(&unlocked_with(vec![]));
        assert_eq!(labels(&nodes), vec!["Open Vault", "Lock", "Logout", "Exit"]);
    }

    /// The password fetch is one `GetPassword` without a master password: the
    /// variant production sends, asserted here rather than in the dispatcher.
    #[test]
    fn copy_password_asks_the_agent_for_exactly_one_action() {
        let action = agent_action(&MenuCommand::CopyPassword("id1".to_string()));
        assert!(
            matches!(
                action,
                Some(Action::GetPassword { ref id, password: None }) if id == "id1"
            ),
            "expected GetPassword without a reprompt answer, got {action:?}"
        );
    }

    #[test]
    fn session_commands_map_to_agent_actions_and_the_rest_are_local() {
        assert!(matches!(
            agent_action(&MenuCommand::Lock),
            Some(Action::Lock)
        ));
        assert!(matches!(
            agent_action(&MenuCommand::Logout),
            Some(Action::Logout)
        ));
        for local in [
            MenuCommand::OpenVault,
            MenuCommand::CopyLogin("id1".to_string()),
            MenuCommand::OpenLink("https://example.com".to_string()),
            MenuCommand::Exit,
        ] {
            assert!(
                agent_action(&local).is_none(),
                "{local:?} is a local hand-off, not an agent action"
            );
        }
    }
}
