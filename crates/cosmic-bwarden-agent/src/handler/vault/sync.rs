use crate::server::with_refresh;
use crate::state::State;
use cosmic_bwarden_core::db::Secret;
use cosmic_bwarden_core::protocol::{Event, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Decide whether a failed sync is just a lock race. Only "unlocked when the
/// sync started, locked when it failed" means the user's re-lock overtook the
/// sync mid-flight — nothing to record. A sync that was ALREADY locked at
/// entry (e.g. `cosmic-bwarden-cli sync` on a locked vault) is reported and
/// logged honestly even if a lock→unlock→lock cycle raced around it.
fn failed_sync_is_lock_race(locked_now: bool, locked_at_entry: bool) -> bool {
    locked_now && !locked_at_entry
}

pub async fn handle_sync(state: &Arc<Mutex<State>>) -> Response {
    // Snapshot the locked state so completion can tell "the vault re-locked
    // while this sync was in flight" (nothing to record) apart from "sync
    // requested while already locked" (report the failure honestly).
    let locked_at_entry = state.lock().await.keys.is_none();
    let res = with_refresh(state, |at| async move {
        let config = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy()?;
        let client =
            cosmic_bwarden_core::api::Client::new(&config.base_url(), &config.identity_url());
        client.sync(&at).await
    })
    .await;

    match res {
        Ok((protected_key, protected_private_key, protected_org_keys, entries)) => {
            let mut state_guard = state.lock().await;

            let pinned_ids: std::collections::HashSet<String> = entries
                .iter()
                .filter(|e| e.favorite)
                .map(|e| e.id.clone())
                .collect();

            let State {
                db,
                pinned_ids: state_pinned_ids,
                ..
            } = &mut *state_guard;

            if let Some(db) = db {
                db.protected_key = Some(Secret::from(protected_key));
                db.protected_private_key = protected_private_key.map(Secret::from);
                db.protected_org_keys = protected_org_keys
                    .into_iter()
                    .map(|(k, v)| (k, Secret::from(v)))
                    .collect();
                db.entries = entries;

                let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                    Ok(c) => c,
                    Err(e) => {
                        return Response::Error {
                            message: format!("failed to load config: {}", e),
                        };
                    }
                };
                let email = match config.email.as_ref() {
                    Some(e) => e,
                    None => {
                        return Response::Error {
                            message: "email not set in config".to_string(),
                        };
                    }
                };
                let server = config.server_name();
                if let Err(e) = db.save(&server, email) {
                    log::error!("failed to save vault DB after sync: {}", e);
                }
            }

            *state_pinned_ids = pinned_ids;
            state_guard.sync_failed = false;
            state_guard.last_sync_error = None;
            state_guard.rebuild_sidebar_cache();
            state_guard.broadcast(Event::VaultChanged);
            Response::Ack
        }
        Err(e) => {
            let raced_lock = {
                let mut g = state.lock().await;
                if failed_sync_is_lock_race(g.keys.is_none(), locked_at_entry) {
                    // The vault re-locked while this sync was in flight (a
                    // post-unlock auto-sync racing the next lock). There is
                    // nothing to record — the next unlock's sync re-runs and
                    // sets/clears the flag truthfully. Keep the failure
                    // visible in the logs so a real server error that merely
                    // coincided with a re-lock is not invisible.
                    log::warn!(
                        "sync: vault re-locked while sync in flight; not marking out-of-sync (error: {})",
                        e
                    );
                    return Response::Ack;
                }
                g.sync_failed = true;
                g.last_sync_error = Some(e.clone());
                g.keys.is_none()
            };
            log::error!(
                "sync failed{}: {}",
                if raced_lock {
                    " (requested while locked)"
                } else {
                    ""
                },
                e
            );
            Response::Error {
                message: format!("sync failed: {}", e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lock-race decision: only "unlocked at entry, locked now" means the
    /// user's re-lock overtook the sync mid-flight. Requested-while-locked is
    /// not a race and must be reported honestly.
    #[test]
    fn failed_sync_lock_race_decision() {
        assert!(
            failed_sync_is_lock_race(true, false),
            "re-locked mid-sync is a race"
        );
        assert!(
            !failed_sync_is_lock_race(true, true),
            "requested while locked is not a race"
        );
        assert!(
            !failed_sync_is_lock_race(false, false),
            "unlocked is not a race"
        );
        assert!(!failed_sync_is_lock_race(false, true));
    }

    /// A sync requested while the vault is locked (epoch unchanged) fails
    /// honestly: Response::Error + out-of-sync flag, never a bare Ack that
    /// pretends the vault synced.
    #[tokio::test]
    async fn locked_sync_reports_failure() {
        let state = Arc::new(Mutex::new(State::new()));
        let res = handle_sync(&state).await;
        assert!(
            matches!(res, Response::Error { .. }),
            "locked sync must not pretend to succeed, got: {res:?}"
        );
        let g = state.lock().await;
        assert!(g.sync_failed, "locked sync failure must set sync_failed");
        assert!(g.last_sync_error.is_some());
    }
}
