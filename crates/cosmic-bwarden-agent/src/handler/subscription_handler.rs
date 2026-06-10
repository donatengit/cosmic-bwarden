use crate::state::State;
use cosmic_bwarden_core::protocol::{Action, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_request(action: Action, _state: &Arc<Mutex<State>>) -> Response {
    match action {
        Action::Subscribe => Response::Ack,
        Action::Quit => {
            log::info!("Quit requested");
            std::process::exit(0);
        }
        _ => Response::Error {
            message: "not implemented in subscription handler".to_string(),
        },
    }
}
