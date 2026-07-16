//! Browser save-prompt support: agent-side login matching and password update.
//!
//! The extension must never pull stored secrets into JS just to decide whether
//! to offer "Save" or "Update" (see the browser-extension security invariant in
//! AGENTS.md). Both handlers keep stored secrets inside the agent:
//! `CheckLoginMatch` compares the just-submitted password against the stored
//! one and returns only an equality bit for a value the client already holds;
//! `UpdateLoginPassword` decrypts the stored entry, swaps the password, and
//! reuses the normal update path (which inherits `error!` logging and
//! `sync_failed` handling).

use crate::state::State;
use cosmic_bwarden_core::api::UriMatchType;
use cosmic_bwarden_core::db::{EntryData, Secret};
use cosmic_bwarden_core::protocol::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

use cosmic_bwarden_core::domain::{host_from_name, host_from_uri, hosts_match};

/// True when the (decrypted) login entry is bound to `domain`, either via a
/// URI or — for legacy entries created without URIs — via a hostname-shaped
/// display name (the save bar's own `name = domain` convention). Matching
/// rules live in `cosmic_bwarden_core::domain` (exact / boundary-subdomain /
/// eTLD+1); free-text names never domain-match.
fn entry_matches_domain(name: &str, uris: &[cosmic_bwarden_core::db::Uri], domain: &str) -> bool {
    // All non-`Never` match types are treated as domain matching (v1
    // simplification, documented in docs/browser_integration.md).
    let uri_match = uris.iter().any(|u| {
        u.match_type != Some(UriMatchType::Never)
            && host_from_uri(&u.uri).is_some_and(|h| hosts_match(&h, domain))
    });
    uri_match || host_from_name(name).is_some_and(|h| hosts_match(&h, domain))
}

pub async fn handle_check_login_match(
    domain: String,
    username: String,
    password: String,
    state: &Arc<Mutex<State>>,
) -> Response {
    let state = state.lock().await;
    let (db, keys) = match (&state.db, &state.keys) {
        (Some(db), Some(keys)) => (db, keys),
        _ => {
            return Response::Error {
                message: "agent is locked".to_string(),
            }
        }
    };
    let empty_org_keys = std::collections::HashMap::new();
    let org_keys = state.org_keys.as_ref().unwrap_or(&empty_org_keys);

    let domain = domain.trim().to_lowercase();
    let wanted_user = username.trim().to_lowercase();
    if domain.is_empty() || wanted_user.is_empty() {
        return Response::LoginMatch {
            entry_id: None,
            name: None,
            password_matches: false,
        };
    }

    let mut first_match: Option<(String, String)> = None;
    for entry in &db.entries {
        if !matches!(entry.data, EntryData::Login { .. }) {
            continue;
        }
        let decrypted = entry.decrypt(keys, org_keys);
        let EntryData::Login {
            username: entry_user,
            password: entry_password,
            uris,
            ..
        } = &decrypted.data
        else {
            continue;
        };

        let user_ok = entry_user
            .as_ref()
            .is_some_and(|u| u.trim().to_lowercase() == wanted_user);
        if !user_ok || !entry_matches_domain(&decrypted.name, uris, &domain) {
            continue;
        }

        if entry_password
            .as_ref()
            .is_some_and(|p| p.expose() == password)
        {
            // Credential already stored as-is: the extension stays silent.
            return Response::LoginMatch {
                entry_id: Some(decrypted.id.clone()),
                name: Some(decrypted.name.clone()),
                password_matches: true,
            };
        }
        if first_match.is_none() {
            first_match = Some((decrypted.id.clone(), decrypted.name.clone()));
        }
    }

    match first_match {
        Some((id, name)) => Response::LoginMatch {
            entry_id: Some(id),
            name: Some(name),
            password_matches: false,
        },
        None => Response::LoginMatch {
            entry_id: None,
            name: None,
            password_matches: false,
        },
    }
}

pub async fn handle_update_login_password(
    id: String,
    password: String,
    state: &Arc<Mutex<State>>,
) -> Response {
    log::debug!("browser password update: id={}", id);
    // Fully decrypt the stored entry inside the agent: every secret is `Some`,
    // so the redacted-merge in handle_update_entry is a no-op and nothing can
    // be wiped, while no stored secret ever transits to the extension.
    let mut entry = {
        let g = state.lock().await;
        let (db, keys) = match (&g.db, &g.keys) {
            (Some(db), Some(keys)) => (db, keys),
            _ => {
                return Response::Error {
                    message: "agent is locked".to_string(),
                }
            }
        };
        let empty_org_keys = std::collections::HashMap::new();
        let org_keys = g.org_keys.as_ref().unwrap_or(&empty_org_keys);
        match db.entries.iter().find(|e| e.id == id) {
            Some(e) => e.decrypt(keys, org_keys),
            None => {
                return Response::Error {
                    message: "entry not found".to_string(),
                }
            }
        }
    };

    match &mut entry.data {
        EntryData::Login { password: p, .. } => *p = Some(Secret::from(password)),
        _ => {
            return Response::Error {
                message: "entry is not a Login".to_string(),
            }
        }
    }

    super::ops::handle_update_entry(entry, state).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_bwarden_core::db::Uri;

    #[test]
    fn uri_match_covers_subdomain_pages() {
        let uris = vec![Uri {
            uri: "https://example.com".to_string(),
            match_type: None,
        }];
        assert!(entry_matches_domain("Whatever", &uris, "example.com"));
        assert!(entry_matches_domain("Whatever", &uris, "login.example.com"));
        assert!(!entry_matches_domain("Whatever", &uris, "notexample.com"));
        assert!(!entry_matches_domain(
            "Whatever",
            &uris,
            "example.com.evil.net"
        ));
    }

    #[test]
    fn never_match_type_uris_are_ignored() {
        let uris = vec![Uri {
            uri: "https://example.com".to_string(),
            match_type: Some(UriMatchType::Never),
        }];
        assert!(!entry_matches_domain("Unrelated", &uris, "example.com"));
        // ...but a hostname-shaped name still matches independently.
        assert!(entry_matches_domain("example.com", &uris, "example.com"));
    }

    #[test]
    fn name_fallback_matches_legacy_entries_without_uris() {
        // The save bar's convention: name = domain, no free text.
        assert!(entry_matches_domain("example.com", &[], "example.com"));
        assert!(entry_matches_domain("example.com", &[], "app.example.com"));
        // Free-text names deliberately no longer domain-match (the old
        // substring rule also matched look-alikes); they remain reachable
        // through typed search.
        assert!(!entry_matches_domain(
            "My example.com login",
            &[],
            "example.com"
        ));
        assert!(!entry_matches_domain("Something else", &[], "example.com"));
    }
}
