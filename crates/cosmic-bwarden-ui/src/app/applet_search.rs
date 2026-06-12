use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};
use crate::fl;

pub const APPLET_SEARCH_LIMIT: usize = 10;
pub const APPLET_LABEL_MAX_LEN: usize = 40;

/// A single result row in the applet search popup.
pub struct AppletRow {
    pub id: String,
    pub primary_label: String,
    pub primary_value: Option<String>,
    /// Render as a single button (name + note marker) rather than a
    /// primary/secret button pair.
    pub single_button: bool,
}

/// Whether `GetSidebarEntries` should be called with `only_pinned = true`.
/// An empty query always restricts to favourites, regardless of the toggle.
pub fn effective_only_pinned(query: &str, favourites_toggle: bool) -> bool {
    query.trim().is_empty() || favourites_toggle
}

/// Truncates a string to at most `APPLET_LABEL_MAX_LEN` characters.
pub fn truncate_label(s: &str) -> String {
    s.chars().take(APPLET_LABEL_MAX_LEN).collect()
}

/// Builds the applet result rows from sidebar entries, dropping entry types
/// that don't have an applet copy action (Card, Identity), and limiting to
/// `APPLET_SEARCH_LIMIT` rows.
pub fn build_applet_rows(entries: &[SidebarEntry]) -> Vec<AppletRow> {
    entries
        .iter()
        .filter_map(|e| match e.entry_type {
            EntryType::Login => {
                let label = match e.username.as_deref() {
                    Some(login) if !login.is_empty() => format!("{} | {}", e.name, login),
                    _ => e.name.clone(),
                };
                Some(AppletRow {
                    id: e.id.clone(),
                    primary_label: truncate_label(&label),
                    primary_value: e.username.clone(),
                    single_button: false,
                })
            }
            EntryType::SecureNote => Some(AppletRow {
                id: e.id.clone(),
                primary_label: truncate_label(&e.name),
                primary_value: None,
                single_button: true,
            }),
            EntryType::SshKey => Some(AppletRow {
                id: e.id.clone(),
                primary_label: truncate_label(&format!("{} | {}", e.name, fl!("public-key-label"))),
                primary_value: e.public_key.clone(),
                single_button: false,
            }),
            EntryType::Card | EntryType::Identity => None,
        })
        .take(APPLET_SEARCH_LIMIT)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, entry_type: EntryType) -> SidebarEntry {
        SidebarEntry {
            id: id.to_string(),
            name: "Entry".to_string(),
            username: None,
            public_key: None,
            entry_type,
            is_pinned: false,
        }
    }

    #[test]
    fn empty_query_is_favourites_only_regardless_of_toggle() {
        assert!(effective_only_pinned("", false));
        assert!(effective_only_pinned("  ", false));
        assert!(effective_only_pinned("", true));
    }

    #[test]
    fn non_empty_query_respects_toggle() {
        assert!(!effective_only_pinned("foo", false));
        assert!(effective_only_pinned("foo", true));
    }

    #[test]
    fn truncate_label_truncates_long_strings() {
        let s = "abcdefghijklmnopqrstuvwxyz0123456789abcdef";
        let truncated = truncate_label(s);
        assert_eq!(truncated.chars().count(), APPLET_LABEL_MAX_LEN);
        assert_eq!(truncated, "abcdefghijklmnopqrstuvwxyz0123456789abcd");
    }

    #[test]
    fn truncate_label_leaves_short_strings_untouched() {
        assert_eq!(truncate_label("short"), "short");
        let exact: String = "abcdefghijklmnopqrstuvwxyz0123456789abcd".chars().collect();
        assert_eq!(exact.chars().count(), APPLET_LABEL_MAX_LEN);
        assert_eq!(truncate_label(&exact), exact);
    }

    #[test]
    fn build_applet_rows_drops_card_and_identity() {
        let entries = vec![
            entry("1", EntryType::Card),
            entry("2", EntryType::Identity),
        ];
        assert!(build_applet_rows(&entries).is_empty());
    }

    #[test]
    fn build_applet_rows_maps_login() {
        let mut e = entry("1", EntryType::Login);
        e.name = "My Site".to_string();
        e.username = Some("alice".to_string());
        let rows = build_applet_rows(&[e]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].primary_label, "My Site | alice");
        assert_eq!(rows[0].primary_value, Some("alice".to_string()));
        assert!(!rows[0].single_button);
    }

    #[test]
    fn build_applet_rows_maps_login_without_username() {
        let mut e = entry("1", EntryType::Login);
        e.name = "My Site".to_string();
        e.username = None;
        let rows = build_applet_rows(&[e]);
        assert_eq!(rows[0].primary_label, "My Site");
    }

    #[test]
    fn build_applet_rows_keeps_full_domain_and_email() {
        let mut e = entry("1", EntryType::Login);
        e.name = "www.facebook.com".to_string();
        e.username = Some("some@email.com".to_string());
        let rows = build_applet_rows(&[e]);
        assert_eq!(rows[0].primary_label, "www.facebook.com | some@email.com");
    }

    #[test]
    fn build_applet_rows_maps_secure_note() {
        let mut e = entry("1", EntryType::SecureNote);
        e.name = "My Note".to_string();
        let rows = build_applet_rows(&[e]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].primary_label, "My Note");
        assert_eq!(rows[0].primary_value, None);
        assert!(rows[0].single_button);
    }

    #[test]
    fn build_applet_rows_maps_ssh_key() {
        let mut e = entry("1", EntryType::SshKey);
        e.name = "My Server".to_string();
        e.public_key = Some("ssh-ed25519 AAAA...".to_string());
        let rows = build_applet_rows(&[e]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].primary_label, "My Server | Public key");
        assert_eq!(rows[0].primary_value, Some("ssh-ed25519 AAAA...".to_string()));
        assert!(!rows[0].single_button);
    }

    #[test]
    fn build_applet_rows_limits_to_ten_after_dropping_unsupported() {
        let mut entries = Vec::new();
        for i in 0..5 {
            entries.push(entry(&format!("card-{i}"), EntryType::Card));
        }
        for i in 0..12 {
            entries.push(entry(&format!("login-{i}"), EntryType::Login));
        }
        let rows = build_applet_rows(&entries);
        assert_eq!(rows.len(), APPLET_SEARCH_LIMIT);
        assert_eq!(rows[0].id, "login-0");
        assert_eq!(rows[9].id, "login-9");
    }
}
