//! Host/domain matching shared by every surface that answers "does this vault
//! entry belong to this web page?" — the browser-extension popup suggestions,
//! the save-prompt (`CheckLoginMatch`), and the applet display labels.
//!
//! The rules, in order (see docs/public_suffix_list.md for the rationale):
//! 1. exact host equality;
//! 2. label-boundary subdomain match, in both directions;
//! 3. same registrable domain (eTLD+1 via the Public Suffix List), only with
//!    the `public_suffix_list` feature.
//!
//! Rule 2 never derives anything from the page host, so it cannot cross a
//! public-suffix boundary: `evil.co.uk` does not match a stored
//! `mybank.co.uk`, because neither is `.`-suffix of the other. Rule 3 is the
//! only place a registrable domain is computed, and the PSL knows `co.uk` is
//! a suffix. IPs and dotless hosts (localhost, intranet names) only ever
//! match exactly.

/// Extract a comparable host from a stored login URI or URL-shaped string:
/// strip scheme, path, query, fragment, userinfo, port, and a leading `www.`;
/// lowercase the rest. Pure string handling so entries with bare hosts
/// ("example.com") work too.
pub fn host_from_uri(uri: &str) -> Option<String> {
    let s = uri.trim();
    let s = s.split_once("://").map_or(s, |(_, rest)| rest);
    let s = s.split(['/', '?', '#']).next().unwrap_or(s);
    let s = s.rsplit('@').next().unwrap_or(s);
    // Split a trailing `:port`, but leave bracketed/bare IPv6 alone (those
    // contain multiple colons and are handled as exact-match hosts anyway).
    let s = match s.find(':') {
        Some(i) if !s[i + 1..].contains(':') && !s.starts_with('[') => &s[..i],
        _ => s,
    };
    let s = s.to_lowercase();
    let s = s.strip_prefix("www.").unwrap_or(&s);
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Extract a host from an entry *name*, for legacy entries created without
/// URIs whose name is the domain (the save bar's own convention). Stricter
/// than [`host_from_uri`]: free-text names ("My Bank"), emails, and anything
/// dotless are rejected rather than coerced.
pub fn host_from_name(name: &str) -> Option<String> {
    let n = name.trim();
    if n.contains(' ') || n.contains('@') || !n.contains('.') {
        return None;
    }
    host_from_uri(n)
}

/// True when `sub` is a strict subdomain of `parent` — i.e. `sub` ends with
/// `.parent`, checked on a label boundary so `notfacebook.com` never matches
/// `facebook.com`. Allocation-free.
pub fn is_subdomain(sub: &str, parent: &str) -> bool {
    sub.len() > parent.len()
        && sub.ends_with(parent)
        && sub.as_bytes()[sub.len() - parent.len() - 1] == b'.'
}

/// The registrable domain (eTLD+1) of `host` per the Public Suffix List:
/// `account.facebook.com` → `facebook.com`, `www.example.co.uk` →
/// `example.co.uk`. `None` for IPs, dotless hosts, bare public suffixes, and
/// unknown TLDs — and always `None` without the `public_suffix_list` feature,
/// which callers must treat as "rule unavailable", never as a match.
#[cfg(feature = "public_suffix_list")]
pub fn registrable_domain(host: &str) -> Option<&str> {
    if host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    psl::domain_str(host)
}

#[cfg(not(feature = "public_suffix_list"))]
pub fn registrable_domain(_host: &str) -> Option<&str> {
    None
}

/// Whether an entry *name* is a URL or bare hostname a browser can open.
/// Every "Open Link" affordance (the COSMIC applet's 🔗 button, the Plasma
/// tray's menu row) keys off this one decision, so it lives here rather than
/// being re-derived per frontend. Uses [`host_from_name`], which rejects
/// free-text names, emails, and dotless hosts.
pub fn is_uri_like(name: &str) -> bool {
    host_from_name(name).is_some()
}

/// The URI to hand a browser for an entry name, or `None` when there is none to
/// hand over. A bare hostname gets `https://` prepended — `xdg-open` needs a
/// scheme; a name that already carries one is passed through unchanged, path
/// and all (`https://example.com/login` stays exactly that).
///
/// Only `http`/`https` survive. A vault name can hold any scheme
/// (`ftp://host`, `ssh://host`), and handing an arbitrary one to `xdg-open`
/// dispatches it to whatever handler the desktop registered for it. The browser
/// extension already refuses anything else before opening a tab
/// (`browser-extension/popup/popup-list-actions.js`, decision S3-7); this is the
/// same rule for the surfaces that hand a URI to the desktop.
pub fn open_uri_for_name(name: &str) -> Option<String> {
    let n = name.trim();
    if !is_uri_like(n) {
        return None;
    }
    let uri = if n.contains("://") {
        n.to_string()
    } else {
        format!("https://{n}")
    };
    let (scheme, _) = uri.split_once("://")?;
    (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")).then_some(uri)
}

/// Compact display label for an entry name: the registrable domain when the
/// name is URI-like (`account.facebook.com` → `facebook.com`), otherwise the
/// name verbatim (`My Facebook`). Without the `public_suffix_list` feature the
/// label is the full host, never a naive last-two-labels cut (which displayed
/// `example.co.uk` as `co.uk`).
pub fn display_label_for_name(name: &str) -> String {
    match host_from_name(name) {
        Some(host) => match registrable_domain(&host) {
            Some(domain) => domain.to_string(),
            None => host,
        },
        None => name.to_string(),
    }
}

/// Whether two already-normalized hosts (lowercase, no scheme/port — i.e.
/// [`host_from_uri`] output) refer to the same site for credential-matching
/// purposes. See the module docs for the rules and their ordering.
pub fn hosts_match(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    // IPs and dotless hosts carry no ownership hierarchy: exact only.
    if !a.contains('.')
        || !b.contains('.')
        || a.parse::<std::net::IpAddr>().is_ok()
        || b.parse::<std::net::IpAddr>().is_ok()
    {
        return false;
    }
    if is_subdomain(a, b) || is_subdomain(b, a) {
        return true;
    }
    match (registrable_domain(a), registrable_domain(b)) {
        (Some(ra), Some(rb)) => ra == rb,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_from_uri_handles_common_shapes() {
        assert_eq!(
            host_from_uri("https://example.com/login"),
            Some("example.com".into())
        );
        assert_eq!(
            host_from_uri("https://www.example.com"),
            Some("example.com".into())
        );
        assert_eq!(
            host_from_uri("http://user@example.com:8443/x?y#z"),
            Some("example.com".into())
        );
        assert_eq!(host_from_uri("example.com"), Some("example.com".into()));
        assert_eq!(
            host_from_uri("APP.Example.COM"),
            Some("app.example.com".into())
        );
        assert_eq!(host_from_uri(""), None);
        assert_eq!(host_from_uri("https://"), None);
    }

    #[test]
    fn host_from_name_rejects_free_text_and_emails() {
        assert_eq!(host_from_name("facebook.com"), Some("facebook.com".into()));
        assert_eq!(
            host_from_name("https://account.facebook.com/login"),
            Some("account.facebook.com".into())
        );
        assert_eq!(host_from_name("My Bank Account"), None);
        assert_eq!(host_from_name("user@gmail.com"), None);
        assert_eq!(host_from_name("My example.com login"), None);
        assert_eq!(host_from_name("localhost"), None);
    }

    /// The "Open Link" decision, shared by the applet and the Plasma tray.
    #[test]
    fn uri_like_names_accept_hosts_and_urls_only() {
        assert!(is_uri_like("account.facebook.com"));
        assert!(is_uri_like("facebook.com"));
        assert!(is_uri_like("https://account.facebook.com/login"));
        assert!(is_uri_like("http://example.co.uk"));
        assert!(!is_uri_like("My Bank Account"));
        assert!(!is_uri_like("user@gmail.com"));
        assert!(!is_uri_like("nodot"));
    }

    /// A scheme other than http/https is not offered as a link: `xdg-open`
    /// would hand it to whatever handler owns that scheme.
    #[test]
    fn open_uri_refuses_non_web_schemes() {
        for name in ["ftp://example.com", "ssh://example.com", "javascript://x"] {
            assert_eq!(open_uri_for_name(name), None, "{name} must not open");
        }
        assert!(
            is_uri_like("ftp://example.com"),
            "it is still a host-shaped name"
        );
        assert_eq!(
            open_uri_for_name("HTTPS://Example.com/login"),
            Some("HTTPS://Example.com/login".into()),
            "the scheme check is case-insensitive"
        );
    }

    #[test]
    fn open_uri_prepends_the_scheme_only_when_missing() {
        assert_eq!(
            open_uri_for_name("account.facebook.com"),
            Some("https://account.facebook.com".into())
        );
        assert_eq!(
            open_uri_for_name("https://account.facebook.com/login"),
            Some("https://account.facebook.com/login".into())
        );
        assert_eq!(open_uri_for_name("My Facebook"), None);
        // Never hand a browser a URI with whitespace in it.
        assert_eq!(
            open_uri_for_name("  facebook.com  "),
            Some("https://facebook.com".into())
        );
    }

    #[test]
    fn display_label_is_the_registrable_domain_or_the_name() {
        assert_eq!(display_label_for_name("www.facebook.com"), "facebook.com");
        assert_eq!(display_label_for_name("My Facebook"), "My Facebook");
        assert_eq!(display_label_for_name("localhost"), "localhost");
    }

    #[test]
    fn subdomain_match_respects_label_boundary() {
        assert!(hosts_match("account.facebook.com", "facebook.com"));
        assert!(hosts_match("facebook.com", "login.facebook.com"));
        assert!(!hosts_match("notfacebook.com", "facebook.com"));
        assert!(!hosts_match("facebook.com.evil.net", "facebook.com"));
        assert!(hosts_match("a.b.example.org", "example.org"));
    }

    #[test]
    fn multi_label_suffix_never_bridges_sites() {
        assert!(!hosts_match("evil.co.uk", "mybank.co.uk"));
        assert!(!hosts_match("victim.com.au", "attacker.com.au"));
        // A stored bare suffix is self-inflicted, but still must not match
        // via the eTLD+1 rule (registrable_domain of "co.uk" is None); only
        // the explicit subdomain rule applies.
        assert!(hosts_match("login.co.uk", "co.uk"));
    }

    #[test]
    fn ips_and_dotless_hosts_match_exactly_only() {
        assert!(hosts_match("192.168.1.10", "192.168.1.10"));
        assert!(!hosts_match("10.192.168.1", "192.168.1.10"));
        assert!(!hosts_match("1.192.168.1.10", "192.168.1.10"));
        assert!(hosts_match("localhost", "localhost"));
        assert!(!hosts_match("localhost", "notlocalhost"));
        assert!(!hosts_match("intranet", "intranet.example.com"));
    }

    #[cfg(feature = "public_suffix_list")]
    #[test]
    fn psl_matches_sibling_subdomains() {
        assert!(hosts_match("accounts.google.com", "mail.google.com"));
        assert!(hosts_match("a.example.co.uk", "b.example.co.uk"));
    }

    #[cfg(feature = "public_suffix_list")]
    #[test]
    fn psl_registrable_domain_handles_multi_label_suffixes() {
        assert_eq!(
            registrable_domain("www.example.co.uk"),
            Some("example.co.uk")
        );
        assert_eq!(
            registrable_domain("account.facebook.com"),
            Some("facebook.com")
        );
        assert_eq!(registrable_domain("co.uk"), None);
        assert_eq!(registrable_domain("192.168.1.10"), None);
        assert_eq!(registrable_domain("localhost"), None);
    }

    #[cfg(not(feature = "public_suffix_list"))]
    #[test]
    fn without_psl_siblings_do_not_match() {
        assert!(!hosts_match("accounts.google.com", "mail.google.com"));
        assert_eq!(registrable_domain("www.example.co.uk"), None);
    }
}
