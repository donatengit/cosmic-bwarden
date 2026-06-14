use crate::state::State;
use cosmic_bwarden_core::protocol::{Action, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_request(action: Action, state: &Arc<Mutex<State>>) -> Response {
    match action {
        Action::Subscribe => Response::Ack,
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
