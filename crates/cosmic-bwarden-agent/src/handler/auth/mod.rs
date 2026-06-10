pub mod config;
pub mod login;
pub mod register;
pub mod unlock;

use crate::state::State;
use cosmic_bwarden_core::protocol::{Action, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_request(action: Action, state: &Arc<Mutex<State>>) -> Response {
    match action {
        Action::Version => config::handle_version().await,
        Action::GetConfig => config::handle_get_config(state).await,
        Action::Lock => config::handle_lock(state).await,
        Action::Logout => config::handle_logout(state).await,
        Action::Register {
            email,
            password,
            server_url,
        } => register::handle_register(email, password, server_url, state).await,
        Action::Login {
            email,
            password,
            server_url,
            remember_me,
            two_factor_token,
            two_factor_provider,
            two_factor_code,
            device_verification_code,
        } => {
            login::handle_login(
                email,
                password,
                server_url,
                remember_me,
                two_factor_token,
                two_factor_provider,
                two_factor_code,
                device_verification_code,
                state,
            )
            .await
        }
        Action::Unlock { password } => unlock::handle_unlock(password, state).await,
        _ => Response::Error {
            message: "not implemented in auth handler".to_string(),
        },
    }
}
