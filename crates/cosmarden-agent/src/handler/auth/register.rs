use crate::state::State;
use cosmarden_core::protocol::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_register(
    email: String,
    password: String,
    server_url: String,
    _state: &Arc<Mutex<State>>,
) -> Response {
    let config = cosmarden_core::config::CosmardenConfig {
        base_url: Some(server_url),
        email: Some(email.clone()),
        ..Default::default()
    };

    let client = cosmarden_core::api::Client::new(&config.base_url(), &config.identity_url());

    let mut pw_vec = cosmarden_core::locked::Vec::new();
    pw_vec.extend(password.as_bytes().iter().copied());
    let pw = cosmarden_core::locked::Password::new(pw_vec);

    // Vaultwarden default for new users is PBKDF2 with 600,000 iterations
    let kdf_type = cosmarden_core::api::KdfType::Pbkdf2;
    let kdf_iterations = 600_000;

    let identity = match cosmarden_core::identity::Identity::new(
        &email,
        &pw,
        kdf_type,
        kdf_iterations,
        None,
        None,
    ) {
        Ok(id) => id,
        Err(e) => {
            return Response::Error {
                message: format!("identity derivation failed: {}", e),
            };
        }
    };

    let protected_key = match cosmarden_core::cipherstring::CipherString::encrypt_symmetric(
        &identity.keys,
        identity.keys.data(),
    ) {
        Ok(cs) => cs.to_string(),
        Err(e) => {
            return Response::Error {
                message: format!("encryption failed: {}", e),
            };
        }
    };

    match client
        .register(
            &email,
            "Cosmic User",
            &cosmarden_core::base64::encode(identity.master_password_hash.hash()),
            &protected_key,
            kdf_type,
            kdf_iterations,
        )
        .await
    {
        Ok(_) => Response::Ack,
        Err(e) => Response::Error {
            message: format!("registration failed: {}", e),
        },
    }
}
