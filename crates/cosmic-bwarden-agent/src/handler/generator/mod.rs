//! Password generator: charset-based generation, "last used settings", and a
//! device-global 7-day history. Deliberately its own top-level handler module
//! rather than living under `handler/vault/` — every `vault::*` handler
//! assumes an unlocked vault (`state.db`), but generation must work locked,
//! and even with no account configured at all. Keeping it a sibling module
//! makes that "works without unlock" property visible directly from
//! `handler.rs`'s dispatch table.

mod algorithm;
mod storage;

use crate::state::State;
use cosmic_bwarden_core::protocol::{Action, GeneratorSettings, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_request(action: Action, _state: &Arc<Mutex<State>>) -> Response {
    match action {
        Action::GeneratePassword { settings } => handle_generate(settings).await,
        Action::GetGeneratorSettings => handle_get_settings().await,
        Action::GetPasswordHistory => handle_get_history().await,
        Action::DeleteGeneratedPassword { created_at } => handle_delete(created_at).await,
        _ => Response::Error {
            message: "not implemented in generator handler".to_string(),
        },
    }
}

async fn handle_generate(settings: Option<GeneratorSettings>) -> Response {
    let settings = match settings {
        Some(s) => {
            // Persist as the new device-wide "last used" settings. A save
            // failure doesn't block generation (it's a preference, not vault
            // data), but is still worth a loud log since it means the next
            // caller silently falls back to a stale/default value.
            if let Err(e) = s.save() {
                log::error!("failed to persist generator settings: {e}");
            }
            s
        }
        None => match GeneratorSettings::load() {
            Ok(s) => s,
            Err(e) => {
                log::warn!("failed to load generator settings, using defaults: {e}");
                GeneratorSettings::default()
            }
        },
    };

    let password = match algorithm::generate_password(&settings) {
        Ok(p) => p,
        Err(message) => return Response::Error { message },
    };

    // A history-append failure shouldn't block returning the password the
    // user asked for, but it does mean that generation silently isn't
    // recorded — loud log per this project's "no silent failures" rule.
    if let Err(e) = storage::append(&password) {
        log::error!("failed to append generated password to history: {e:#}");
    }

    Response::GeneratedPassword { password }
}

async fn handle_get_settings() -> Response {
    match GeneratorSettings::load() {
        Ok(settings) => Response::GeneratorSettings { settings },
        Err(e) => Response::Error {
            message: format!("failed to load generator settings: {e}"),
        },
    }
}

async fn handle_get_history() -> Response {
    match storage::get_pruned_newest_first() {
        Ok(entries) => Response::PasswordHistory { entries },
        Err(e) => Response::Error {
            message: format!("failed to load password history: {e:#}"),
        },
    }
}

async fn handle_delete(created_at: u64) -> Response {
    match storage::delete_by_created_at(created_at) {
        Ok(()) => Response::Ack,
        Err(e) => Response::Error {
            message: format!("failed to delete password history entry: {e:#}"),
        },
    }
}
