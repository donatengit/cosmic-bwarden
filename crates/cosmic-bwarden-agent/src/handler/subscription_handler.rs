use crate::state::State;
use cosmic_bwarden_core::protocol::{Action, Event, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_request(action: Action, state: &Arc<Mutex<State>>) -> Response {
    match action {
        Action::Subscribe => Response::Ack,
        Action::RequestUnlock => {
            let mut state_guard = state.lock().await;
            state_guard.broadcast(Event::UnlockRequested);
            Response::Ack
        }
        Action::SetPendingEntry { id } => {
            let mut state_guard = state.lock().await;
            // Broadcast to already-connected vault windows (vault already open).
            state_guard.broadcast(Event::OpenEntry { id: id.clone() });
            // Also store for the next subscriber (vault not yet launched).
            state_guard.pending_entry_id = Some(id);
            Response::Ack
        }
        Action::Quit => {
            log::info!("Quit requested");
            let state_guard = state.lock().await;
            if let Some(tx) = &state_guard.shutdown_tx {
                let _ = tx.send(());
            }
            Response::Ack
        }
        _ => Response::Error {
            message: "not implemented in subscription handler".to_string(),
        },
    }
}
