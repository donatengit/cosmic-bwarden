use crate::state::State;
use cosmic_bwarden_core::protocol::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_register(
    email: String,
    password: String,
    server_url: String,
    _state: &Arc<Mutex<State>>,
) -> Response {
    let mut config = cosmic_bwarden_core::config::CosmicBWardenConfig::default();
    config.base_url = Some(server_url);
    config.email = Some(email.clone());

    let client =
        cosmic_bwarden_core::api::Client::new(&config.base_url(), &config.identity_url());

    let mut pw_vec = cosmic_bwarden_core::locked::Vec::new();
    pw_vec.extend(password.as_bytes().iter().copied());
    let pw = cosmic_bwarden_core::locked::Password::new(pw_vec);

    // Vaultwarden default for new users is PBKDF2 with 600,000 iterations
    let kdf_type = cosmic_bwarden_core::api::KdfType::Pbkdf2;
    let kdf_iterations = 600_000;

    let identity = match cosmic_bwarden_core::identity::Identity::new(
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

    let protected_key =
        match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
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
            &cosmic_bwarden_core::base64::encode(identity.master_password_hash.hash()),
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
