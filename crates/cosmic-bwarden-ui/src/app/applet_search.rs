use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};

pub const APPLET_SEARCH_LIMIT: usize = 10;
pub const APPLET_LABEL_MAX_LEN: usize = 20;

/// A single result row in the applet search popup.
pub struct AppletRow {
    pub id: String,
    pub primary_label: String,
    pub primary_value: Option<String>,
    pub secret_label_key: &'static str,
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
            EntryType::Login => Some(AppletRow {
                id: e.id.clone(),
                primary_label: truncate_label(e.username.as_deref().unwrap_or("")),
                primary_value: e.username.clone(),
                secret_label_key: "password-label",
            }),
            EntryType::SecureNote => Some(AppletRow {
                id: e.id.clone(),
                primary_label: truncate_label(&e.name),
                primary_value: Some(e.name.clone()),
                secret_label_key: "note-label",
            }),
            EntryType::SshKey => Some(AppletRow {
                id: e.id.clone(),
                primary_label: truncate_label("Public key"),
                primary_value: e.public_key.clone(),
                secret_label_key: "private-key-label",
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
        let s = "abcdefghijklmnopqrstuvwxyz";
        assert_eq!(truncate_label(s), "abcdefghijklmnopqrst");
    }

    #[test]
    fn truncate_label_leaves_short_strings_untouched() {
        assert_eq!(truncate_label("short"), "short");
        let exact = "abcdefghijklmnopqrst";
        assert_eq!(exact.chars().count(), APPLET_LABEL_MAX_LEN);
        assert_eq!(truncate_label(exact), exact);
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
        e.username = Some("alice".to_string());
        let rows = build_applet_rows(&[e]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].primary_label, "alice");
        assert_eq!(rows[0].primary_value, Some("alice".to_string()));
        assert_eq!(rows[0].secret_label_key, "password-label");
    }

    #[test]
    fn build_applet_rows_maps_secure_note() {
        let mut e = entry("1", EntryType::SecureNote);
        e.name = "My Note".to_string();
        let rows = build_applet_rows(&[e]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].primary_label, "My Note");
        assert_eq!(rows[0].primary_value, Some("My Note".to_string()));
        assert_eq!(rows[0].secret_label_key, "note-label");
    }

    #[test]
    fn build_applet_rows_maps_ssh_key() {
        let mut e = entry("1", EntryType::SshKey);
        e.public_key = Some("ssh-ed25519 AAAA...".to_string());
        let rows = build_applet_rows(&[e]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].primary_label, "Public key");
        assert_eq!(rows[0].primary_value, Some("ssh-ed25519 AAAA...".to_string()));
        assert_eq!(rows[0].secret_label_key, "private-key-label");
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
