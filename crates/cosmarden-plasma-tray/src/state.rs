//! The tray's whole state: what the menu needs, and nothing else.
//!
//! There is no vault model here and no cached plaintext. A row is a label, the
//! username and the id the agent already returns for a sidebar render; the
//! password is fetched per click and lives only long enough to reach the
//! clipboard (`crate::clipboard`).

use cosmarden_core::agent_client::AgentClient;
use cosmarden_core::protocol::{Action, EntryType, Response, SidebarEntry};

/// How many login rows the menu shows. The same policy as the applet's search
/// results (`APPLET_SEARCH_LIMIT`): a tray menu is a shortcut list, not a vault
/// browser, and the full app is one row away.
pub const MENU_ROW_LIMIT: usize = 10;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrayState {
    /// The agent answered `Version` with a different protocol version. The
    /// menu then offers nothing but Exit: no row this build sends would be
    /// understood.
    pub protocol_mismatch: bool,
    /// `GetConfig` came back — the agent is up. Before that, `has_account` and
    /// `locked` mean nothing.
    pub agent_ready: bool,
    /// An account exists (configured, and not waiting for a login).
    pub has_account: bool,
    pub locked: bool,
    pub rows: Vec<LoginRow>,
}

/// One login row: what the menu can do with it, and nothing else.
///
/// The password is *not* here — it is fetched on demand (`Copy password`) and
/// goes straight to the clipboard. The username rides along because the agent
/// already returns it for a sidebar row, so `Copy login` needs no round-trip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginRow {
    pub id: String,
    /// Compact display label (registrable domain, or the name verbatim).
    pub label: String,
    /// Absent when the entry stores no username: the row then offers no
    /// `Copy login`.
    pub username: Option<String>,
    /// Absent when the name is not URI-like: the row then offers no
    /// `Open link`.
    pub uri: Option<String>,
}

impl TrayState {
    /// Whether the vault is open, i.e. whether rows and `Lock`/`Logout` apply.
    pub fn unlocked(&self) -> bool {
        self.agent_ready && self.has_account && !self.locked
    }

    /// The row a `Copy …` command refers to, by id.
    ///
    /// Rows are a snapshot, so a click can arrive after a vault change replaced
    /// them; a missing id means "this row is gone", never a panic.
    pub fn row(&self, id: &str) -> Option<&LoginRow> {
        self.rows.iter().find(|row| row.id == id)
    }
}

/// The rows a menu can offer: `Login` entries, capped at [`MENU_ROW_LIMIT`].
///
/// Every login is listed, because every login has at least a password to copy.
/// `Open link` appears only when the name is URI-like — `domain::open_uri_for_name`
/// is that one decision, shared with the applet's 🔗 button — and `Copy login`
/// only when a username is stored.
///
/// A note or an SSH key contributes no row: it has no site to open, and the
/// only action left would be copying a secret blob whose type has no other
/// meaning in a menu. The vault app is the surface for those.
pub fn login_rows(entries: &[SidebarEntry]) -> Vec<LoginRow> {
    entries
        .iter()
        .filter(|e| e.entry_type == EntryType::Login)
        .map(|e| LoginRow {
            id: e.id.clone(),
            label: cosmarden_core::domain::display_label_for_name(&e.name),
            username: e.username.clone(),
            uri: cosmarden_core::domain::open_uri_for_name(&e.name),
        })
        .take(MENU_ROW_LIMIT)
        .collect()
}

/// Read the agent once: protocol version, configuration, and — when the vault
/// is open — the sidebar rows.
///
/// Failures are not fatal and never panic: an unreachable agent yields the
/// "Open Vault and Exit" state, and the supervisor's next poll or the
/// subscription retries. The supervisor logs the *transitions* (reachable →
/// unreachable, locked → unlocked); per-poll noise stays at debug so a stopped
/// agent cannot fill the journal.
pub async fn probe(client: &AgentClient) -> TrayState {
    let mut next = TrayState::default();

    match client.send(Action::Version).await {
        Ok(Response::Version {
            protocol_version, ..
        }) => next.protocol_mismatch = protocol_version != cosmarden_core::PROTOCOL_VERSION,
        Ok(other) => {
            tracing::debug!("Version answered with {other:?}; treating the agent as unreachable");
            return next;
        }
        Err(e) => {
            tracing::debug!("agent unreachable: {e}");
            return next;
        }
    }
    if next.protocol_mismatch {
        return next;
    }

    match client.send(Action::GetConfig).await {
        Ok(Response::Config {
            has_account,
            is_locked,
            needs_login,
            ..
        }) => {
            next.agent_ready = true;
            // `needs_login` means the stored session is gone: the app has to
            // log in again, so there is no vault to show rows from.
            next.has_account = has_account && !needs_login;
            next.locked = is_locked;
        }
        Ok(other) => {
            tracing::debug!("GetConfig answered with {other:?}; treating the agent as unreachable");
            return next;
        }
        Err(e) => {
            tracing::debug!("agent unreachable: {e}");
            return next;
        }
    }

    if next.unlocked() {
        // Favourites only, matching the applet popup's default view (an empty
        // query is favourites-only there too): the menu shows what the user
        // pinned, not the first ten entries of the vault in some order.
        let action = Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: true,
            domain: None,
        };
        match client.send(action).await {
            Ok(Response::SidebarEntries { entries }) => next.rows = login_rows(&entries),
            Ok(other) => tracing::warn!("GetSidebarEntries answered with {other:?}"),
            // The vault can lock between the config read and this call; a
            // failed read means "no rows now", not a broken tray.
            Err(e) => tracing::warn!("sidebar entries unavailable: {e}"),
        }
    }

    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, name: &str, entry_type: EntryType, is_pinned: bool) -> SidebarEntry {
        SidebarEntry {
            id: id.to_string(),
            name: name.to_string(),
            username: Some("alice".to_string()),
            public_key: None,
            entry_type,
            is_pinned,
        }
    }

    #[test]
    fn every_login_is_a_row_and_only_logins_are() {
        let entries = vec![
            entry("1", "facebook.com", EntryType::Login, true),
            entry("2", "My Facebook", EntryType::Login, true),
            entry("3", "My note", EntryType::SecureNote, true),
            entry("4", "ssh server", EntryType::SshKey, true),
        ];
        let rows = login_rows(&entries);
        assert_eq!(rows.len(), 2, "non-logins contribute no row: {rows:?}");
        assert_eq!(
            rows[0].uri.as_deref(),
            Some("https://facebook.com"),
            "a URI-like name gets an open-link target"
        );
        assert_eq!(
            rows[1].uri, None,
            "a free-text name is still listed (it has a password to copy), but has nothing to open"
        );
        assert_eq!(rows[1].label, "My Facebook");
    }

    #[test]
    fn rows_keep_the_id_username_and_domain_label() {
        let rows = login_rows(&[entry(
            "1",
            "https://account.facebook.com/login",
            EntryType::Login,
            true,
        )]);
        assert_eq!(rows[0].id, "1");
        assert_eq!(rows[0].label, "facebook.com");
        assert_eq!(
            rows[0].uri.as_deref(),
            Some("https://account.facebook.com/login")
        );
        assert_eq!(rows[0].username.as_deref(), Some("alice"));
    }

    #[test]
    fn a_stored_entry_without_a_username_has_no_login_to_copy() {
        let mut e = entry("1", "facebook.com", EntryType::Login, true);
        e.username = None;
        assert_eq!(login_rows(&[e])[0].username, None);
    }

    #[test]
    fn rows_are_found_by_id_and_a_stale_id_finds_nothing() {
        let state = TrayState {
            agent_ready: true,
            has_account: true,
            rows: login_rows(&[entry("7", "facebook.com", EntryType::Login, true)]),
            ..Default::default()
        };
        assert_eq!(
            state.row("7").map(|row| row.label.as_str()),
            Some("facebook.com")
        );
        assert!(
            state.row("gone").is_none(),
            "a row replaced by a vault change must not panic the click handler"
        );
    }

    #[test]
    fn rows_are_capped() {
        let entries: Vec<SidebarEntry> = (0..MENU_ROW_LIMIT + 5)
            .map(|i| {
                entry(
                    &format!("{i}"),
                    &format!("site{i}.com"),
                    EntryType::Login,
                    true,
                )
            })
            .collect();
        assert_eq!(login_rows(&entries).len(), MENU_ROW_LIMIT);
    }

    #[test]
    fn only_an_answered_config_means_unlocked() {
        assert!(!TrayState::default().unlocked());
        assert!(
            !TrayState {
                has_account: true,
                locked: false,
                ..Default::default()
            }
            .unlocked(),
            "an account without a reachable agent has nothing to show"
        );
        assert!(TrayState {
            agent_ready: true,
            has_account: true,
            locked: false,
            rows: vec![],
            protocol_mismatch: false,
        }
        .unlocked());
    }
}
