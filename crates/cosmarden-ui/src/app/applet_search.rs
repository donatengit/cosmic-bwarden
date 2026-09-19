use cosmarden_core::protocol::{EntryType, SidebarEntry};

pub const APPLET_SEARCH_LIMIT: usize = 10;
pub const APPLET_LABEL_MAX_LEN: usize = 28;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppletRowKind {
    Login {
        username: Option<String>,
        link: Option<String>,
    },
    SecureNote,
    SshKey,
}

/// A single result row in the applet search popup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppletRow {
    pub id: String,
    pub label: String,
    pub kind: AppletRowKind,
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

/// Extracts a compact domain label (e.g. `"facebook.com"`) from a
/// hostname-like name. Falls back to the original name if it doesn't look
/// like a URL/hostname. With the `public_suffix_list` feature the label is
/// the PSL-derived eTLD+1 (`www.example.co.uk` → `example.co.uk`).
///
/// Delegates to `cosmarden_core::domain` so the applet, the Plasma tray and
/// the extension label the same name the same way.
pub fn extract_domain_label(name: &str) -> String {
    cosmarden_core::domain::display_label_for_name(name)
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
                label: truncate_label(&extract_domain_label(&e.name)),
                kind: AppletRowKind::Login {
                    username: e.username.clone(),
                    link: cosmarden_core::domain::open_uri_for_name(&e.name),
                },
            }),
            EntryType::SecureNote => Some(AppletRow {
                id: e.id.clone(),
                label: truncate_label(&e.name),
                kind: AppletRowKind::SecureNote,
            }),
            EntryType::SshKey => Some(AppletRow {
                id: e.id.clone(),
                label: truncate_label(&e.name),
                kind: AppletRowKind::SshKey,
            }),
            EntryType::Card | EntryType::Identity => None,
            EntryType::BankAccount | EntryType::DriversLicense | EntryType::Passport => None,
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
        let s = "abcdefghijklmnopqrstuvwxyz01234";
        let truncated = truncate_label(s);
        assert_eq!(truncated.chars().count(), APPLET_LABEL_MAX_LEN);
    }

    #[test]
    fn truncate_label_leaves_short_strings_untouched() {
        assert_eq!(truncate_label("short"), "short");
    }

    #[cfg(feature = "public_suffix_list")]
    #[test]
    fn extract_domain_label_strips_subdomain() {
        assert_eq!(extract_domain_label("account.facebook.com"), "facebook.com");
        assert_eq!(extract_domain_label("www.facebook.com"), "facebook.com");
    }

    #[cfg(feature = "public_suffix_list")]
    #[test]
    fn extract_domain_label_strips_protocol_and_path() {
        assert_eq!(
            extract_domain_label("https://account.facebook.com/login"),
            "facebook.com"
        );
        // The PSL knows co.uk is a public suffix; the old last-two-labels
        // cut displayed this as "co.uk".
        assert_eq!(
            extract_domain_label("http://www.example.co.uk/path"),
            "example.co.uk"
        );
    }

    #[cfg(not(feature = "public_suffix_list"))]
    #[test]
    fn extract_domain_label_without_psl_keeps_full_host() {
        assert_eq!(
            extract_domain_label("account.facebook.com"),
            "account.facebook.com"
        );
        assert_eq!(extract_domain_label("www.facebook.com"), "facebook.com");
        assert_eq!(
            extract_domain_label("http://www.example.co.uk/path"),
            "example.co.uk"
        );
    }

    #[test]
    fn extract_domain_label_leaves_non_url_names() {
        assert_eq!(extract_domain_label("My Bank Account"), "My Bank Account");
        assert_eq!(extract_domain_label("user@gmail.com"), "user@gmail.com");
    }

    #[test]
    fn extract_domain_label_handles_single_dot() {
        assert_eq!(extract_domain_label("facebook.com"), "facebook.com");
    }

    /// The applet's 🔗 and the tray's `Open link` both come from
    /// `cosmarden_core::domain::open_uri_for_name`; these pin the wiring, the
    /// predicate itself is tested in core.
    #[test]
    fn build_applet_rows_sets_link_for_hostname_name() {
        let mut e = entry("1", EntryType::Login);
        e.name = "account.facebook.com".to_string();
        let rows = build_applet_rows(&[e]);
        // bare hostname → https:// prepended so xdg-open treats it as a URL
        assert!(
            matches!(&rows[0].kind, AppletRowKind::Login { link: Some(l), .. } if l == "https://account.facebook.com")
        );
    }

    #[test]
    fn build_applet_rows_preserves_existing_scheme_in_link() {
        let mut e = entry("1", EntryType::Login);
        e.name = "https://account.facebook.com/login".to_string();
        let rows = build_applet_rows(&[e]);
        assert!(
            matches!(&rows[0].kind, AppletRowKind::Login { link: Some(l), .. } if l == "https://account.facebook.com/login")
        );
    }

    #[test]
    fn build_applet_rows_no_link_for_plain_name() {
        let mut e = entry("1", EntryType::Login);
        e.name = "My Facebook".to_string();
        let rows = build_applet_rows(&[e]);
        assert!(matches!(
            &rows[0].kind,
            AppletRowKind::Login { link: None, .. }
        ));
    }

    #[test]
    fn build_applet_rows_drops_card_and_identity() {
        let entries = vec![entry("1", EntryType::Card), entry("2", EntryType::Identity)];
        assert!(build_applet_rows(&entries).is_empty());
    }

    #[test]
    fn build_applet_rows_maps_login() {
        let mut e = entry("1", EntryType::Login);
        e.name = "My Site".to_string();
        e.username = Some("alice".to_string());
        let rows = build_applet_rows(&[e]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "My Site");
        assert!(
            matches!(&rows[0].kind, AppletRowKind::Login { username: Some(u), .. } if u == "alice")
        );
    }

    #[test]
    fn build_applet_rows_extracts_domain_from_url_name() {
        let mut e = entry("1", EntryType::Login);
        e.name = "www.facebook.com".to_string();
        e.username = Some("some@email.com".to_string());
        let rows = build_applet_rows(&[e]);
        assert_eq!(rows[0].label, "facebook.com");
    }

    #[test]
    fn build_applet_rows_maps_secure_note() {
        let mut e = entry("1", EntryType::SecureNote);
        e.name = "My Note".to_string();
        let rows = build_applet_rows(&[e]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "My Note");
        assert!(matches!(rows[0].kind, AppletRowKind::SecureNote));
    }

    #[test]
    fn build_applet_rows_maps_ssh_key() {
        let mut e = entry("1", EntryType::SshKey);
        e.name = "My Server".to_string();
        e.public_key = Some("ssh-ed25519 AAAA...".to_string());
        let rows = build_applet_rows(&[e]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "My Server");
        assert!(matches!(rows[0].kind, AppletRowKind::SshKey));
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
