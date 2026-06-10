mod auth;
mod subscription_handler;
mod vault;

use crate::state::State;
use cosmic_bwarden_core::protocol::{Action, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_request(action: Action, state: &Arc<Mutex<State>>) -> Response {
    log::info!("Received action: {:?}", action);
    match action {
        // Authentication actions
        Action::Version
        | Action::GetConfig
        | Action::Register { .. }
        | Action::Login { .. }
        | Action::Unlock { .. }
        | Action::Lock
        | Action::Logout => auth::handle_request(action, state).await,
        // Vault operations
        Action::Sync
        | Action::GetEntries { .. }
        | Action::GetSidebarEntries { .. }
        | Action::GetEntry { .. }
        | Action::GetPassword { .. }
        | Action::CopyToClipboard { .. }
        | Action::DeleteEntry { .. }
        | Action::UpdateEntry { .. }
        | Action::PinEntry { .. }
        | Action::UnpinEntry { .. }
        | Action::AddEntry { .. }
        | Action::AddSecureNote { .. }
        | Action::AddCard { .. }
        | Action::AddIdentity { .. }
        | Action::AddSshKey { .. }
        | Action::GetTopFrequent { .. } => vault::handle_request(action, state).await,
        // Subscription / control actions
        Action::Subscribe | Action::Quit => {
            subscription_handler::handle_request(action, state).await
        }
    }
}
