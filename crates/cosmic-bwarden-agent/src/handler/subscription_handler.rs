use crate::state::State;
use cosmic_bwarden_core::protocol::{Action, Event, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_request(action: Action, state: &Arc<Mutex<State>>) -> Response {
    match action {
        Action::Subscribe => Response::Ack,
        Action::RequestUnlock => {
            let mut state_guard = state.lock().await;
            // Same decision as the agent-internal paths (ssh_agent): broadcast
            // PinRequested when a TPM PIN is configured, UnlockRequested
            // otherwise — and only once per lock period, so a client that
            // polls can't spam unlock prompts.
            state_guard.request_unlock();
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    async fn state_with_subscriber() -> (Arc<Mutex<State>>, mpsc::UnboundedReceiver<Event>) {
        let state = Arc::new(Mutex::new(State::new()));
        let (tx, rx) = mpsc::unbounded_channel();
        state.lock().await.subscribers.push(tx);
        (state, rx)
    }

    /// Action::RequestUnlock must use the same decision as the agent-internal
    /// paths (state.request_unlock): PinRequested when a TPM PIN is
    /// configured, UnlockRequested otherwise, and only once per lock period.
    #[tokio::test]
    async fn request_unlock_broadcasts_pin_requested_when_tpm_configured() {
        let (state, mut rx) = state_with_subscriber().await;
        state.lock().await.tpm_configured = true;

        let res = handle_request(Action::RequestUnlock, &state).await;
        assert!(matches!(res, Response::Ack));
        assert!(
            matches!(rx.try_recv(), Ok(Event::PinRequested)),
            "expected PinRequested when tpm_configured"
        );

        // Debounced: a second request in the same lock period must not
        // broadcast again.
        let res = handle_request(Action::RequestUnlock, &state).await;
        assert!(matches!(res, Response::Ack));
        assert!(
            rx.try_recv().is_err(),
            "second RequestUnlock must not re-broadcast"
        );

        // A new lock period (lock resets the debounce flag) broadcasts again.
        state.lock().await.lock();
        let res = handle_request(Action::RequestUnlock, &state).await;
        assert!(matches!(res, Response::Ack));
        // The lock() call itself broadcast Event::Locked; drain it.
        let _ = rx.try_recv();
        assert!(
            matches!(rx.try_recv(), Ok(Event::PinRequested)),
            "expected PinRequested after a fresh lock"
        );
    }

    #[tokio::test]
    async fn request_unlock_broadcasts_unlock_requested_without_tpm_pin() {
        let (state, mut rx) = state_with_subscriber().await;
        // tpm_configured defaults to false: password unlock is the offer.
        let res = handle_request(Action::RequestUnlock, &state).await;
        assert!(matches!(res, Response::Ack));
        assert!(
            matches!(rx.try_recv(), Ok(Event::UnlockRequested)),
            "expected UnlockRequested when no TPM PIN configured"
        );
    }
}
