use cosmic_bwarden_core::protocol::{EntryType, SidebarEntry};

pub const APPLET_SEARCH_LIMIT: usize = 10;
pub const APPLET_LABEL_MAX_LEN: usize = 28;

pub enum AppletRowKind {
    Login {
        username: Option<String>,
        link: Option<String>,
    },
    SecureNote,
    SshKey,
}

/// A single result row in the applet search popup.
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

/// Extracts the eTLD+1 (e.g. `"facebook.com"`) from a hostname-like name.
/// Falls back to the original name if it doesn't look like a URL/hostname.
pub fn extract_domain_label(name: &str) -> String {
    // Strip protocol prefix if present
    let stripped = name
        .strip_prefix("https://")
        .or_else(|| name.strip_prefix("http://"))
        .unwrap_or(name);

    // Take only the host part (before any path)
    let host = stripped.split('/').next().unwrap_or(stripped);

    // Must look like a hostname: no spaces, has a dot, no @ (email)
    if host.contains(' ') || !host.contains('.') || host.contains('@') {
        return name.to_string();
    }

    // Extract eTLD+1: last two dot-separated segments
    if let Some(last_dot) = host.rfind('.') {
        if let Some(prev_dot) = host[..last_dot].rfind('.') {
            return host[prev_dot + 1..].to_string();
        }
    }
    host.to_string()
}

/// Returns `true` if `name` looks like a URL or bare hostname that a browser
/// can open (has a dot, no spaces, no `@`).  Used to decide whether to enable
/// the 🔗 button for a login row.
pub fn is_uri_like(name: &str) -> bool {
    let stripped = name
        .strip_prefix("https://")
        .or_else(|| name.strip_prefix("http://"))
        .unwrap_or(name);
    let host = stripped.split('/').next().unwrap_or(stripped);
    !host.contains(' ') && host.contains('.') && !host.contains('@')
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
                    link: is_uri_like(&e.name).then(|| {
                        if e.name.contains("://") {
                            e.name.clone()
                        } else {
                            format!("https://{}", e.name)
                        }
                    }),
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

    #[test]
    fn extract_domain_label_strips_subdomain() {
        assert_eq!(extract_domain_label("account.facebook.com"), "facebook.com");
        assert_eq!(extract_domain_label("www.facebook.com"), "facebook.com");
    }

    #[test]
    fn extract_domain_label_strips_protocol_and_path() {
        assert_eq!(
            extract_domain_label("https://account.facebook.com/login"),
            "facebook.com"
        );
        assert_eq!(
            extract_domain_label("http://www.example.co.uk/path"),
            "co.uk"
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

    #[test]
    fn is_uri_like_accepts_hostnames_and_urls() {
        assert!(is_uri_like("account.facebook.com"));
        assert!(is_uri_like("facebook.com"));
        assert!(is_uri_like("https://account.facebook.com/login"));
        assert!(is_uri_like("http://example.co.uk"));
    }

    #[test]
    fn is_uri_like_rejects_plain_names_and_emails() {
        assert!(!is_uri_like("My Bank Account"));
        assert!(!is_uri_like("user@gmail.com"));
        assert!(!is_uri_like("nodot"));
    }

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
