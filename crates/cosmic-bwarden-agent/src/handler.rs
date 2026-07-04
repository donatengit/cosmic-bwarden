mod auth;
mod subscription_handler;
mod vault;

use crate::state::State;
use cosmic_bwarden_core::protocol::{Action, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_request(action: Action, state: &Arc<Mutex<State>>) -> Response {
    log::info!("Received action: {:?}", action);
    let response = dispatch(action, state).await;
    if let Response::Error { message } = &response {
        log::error!("Action returned error: {}", message);
    }
    response
}

async fn dispatch(action: Action, state: &Arc<Mutex<State>>) -> Response {
    match action {
        // Authentication and TPM PIN actions
        Action::Version
        | Action::GetConfig
        | Action::Register { .. }
        | Action::Login { .. }
        | Action::Unlock { .. }
        | Action::Lock
        | Action::Logout
        | Action::CheckTpm
        | Action::CheckTpmDiagnostics
        | Action::GetTpmDaStatus
        | Action::SetupTpmPin { .. }
        | Action::SetupTpmPinFromUnlocked { .. }
        | Action::UnlockWithPin { .. }
        | Action::DisableTpmPin
        | Action::EnableTpmServerCredentials
        | Action::DisableTpmServerCredentials => auth::handle_request(action, state).await,
        // Vault operations
        Action::Sync
        | Action::GetEntries { .. }
        | Action::GetSidebarEntries { .. }
        | Action::GetEntry { .. }
        | Action::GetEntryMeta { .. }
        | Action::GetPassword { .. }
        | Action::GetTotp { .. }
        | Action::CopyToClipboard { .. }
        | Action::DeleteEntry { .. }
        | Action::UpdateEntry { .. }
        | Action::PinEntry { .. }
        | Action::UnpinEntry { .. }
        | Action::AddEntry { .. }
        | Action::AddSecureNote { .. }
        | Action::AddCard { .. }
        | Action::AddIdentity { .. }
        | Action::AddSshKey { .. } => vault::handle_request(action, state).await,
        // Subscription / control actions
        Action::Subscribe
        | Action::Quit
        | Action::SetPendingEntry { .. }
        | Action::RequestUnlock => subscription_handler::handle_request(action, state).await,
        // Intercepted in main.rs before reaching dispatch
        Action::UpdateLockTimeout { .. } => Response::Ack,
    }
}
