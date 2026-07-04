//! Unlock the vault using TPM-sealed keys and a PIN.

#[cfg(feature = "tpm")]
use crate::keyring;
use crate::state::State;
#[cfg(feature = "tpm")]
use cosmic_bwarden_core::db::Secret;
use cosmic_bwarden_core::protocol::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Unlock the vault using the TPM-sealed keys and a PIN.
pub async fn handle_unlock_with_pin(pin: String, state: &Arc<Mutex<State>>) -> Response {
    #[cfg(feature = "tpm")]
    {
        let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
            Ok(c) => c,
            Err(e) => {
                return Response::Error {
                    message: format!("failed to load config: {}", e),
                }
            }
        };
        let email = match config.email.as_ref() {
            Some(e) => e.clone(),
            None => {
                return Response::Error {
                    message: "email not set in config".to_string(),
                }
            }
        };
        let mut db = match cosmic_bwarden_core::db::Db::load(&config.server_name(), &email) {
            Ok(d) => d,
            Err(e) => {
                return Response::Error {
                    message: format!("failed to load db: {}", e),
                }
            }
        };

        if !db.has_account() {
            return Response::Error {
                message: "no account configured — please login first".to_string(),
            };
        }

        let blob_path = cosmic_bwarden_core::dirs::tpm_blob_file(&config.server_name(), &email);
        let hash_blob_path =
            cosmic_bwarden_core::dirs::tpm_hash_blob_file(&config.server_name(), &email);

        // Unseal the vault symmetric keys from the TPM (these are the same keys stored
        // in state.keys after a normal password unlock — NOT the identity/KDF keys).
        let vault_keys = match crate::tpm::unseal(&blob_path, &pin).await {
            Ok(k) => k,
            Err(e) => {
                // The full chain (wrong PIN / changed PCRs / DA lockout / TSS
                // detail) is log-only; clients key on the stable short message.
                log::error!("TPM unseal failed: {:#}", e);
                return Response::Error {
                    message: cosmic_bwarden_core::protocol::ERR_TPM_UNSEAL_FAILED.to_string(),
                };
            }
        };

        // Try to unseal the server credentials (master_password_hash) if available.
        // This blob is TPM-bound only (no PIN) and enables silent re-auth after PIN unlock.
        let maybe_hash = if hash_blob_path.exists() {
            match crate::tpm::unseal_bytes(&hash_blob_path, "").await {
                Ok(bytes) => {
                    let mut locked_vec = cosmic_bwarden_core::locked::Vec::new();
                    locked_vec.extend(bytes.iter().copied());
                    Some(cosmic_bwarden_core::locked::PasswordHash::new(locked_vec))
                }
                Err(e) => {
                    log::warn!(
                        "pin unlock: could not unseal server credentials from TPM: {}",
                        e
                    );
                    None
                }
            }
        } else {
            None
        };

        // Derive org keys from the vault symmetric keys directly (no protected-key
        // decryption needed — we already have the vault keys).
        let org_keys_raw: std::collections::HashMap<_, _> = db
            .protected_org_keys
            .iter()
            .map(|(k, v)| (k.clone(), v.expose().to_string()))
            .collect();

        let org_keys = match cosmic_bwarden_core::vault::decrypt_org_keys(
            &vault_keys,
            db.protected_private_key.as_ref().map(|s| s.expose()),
            &org_keys_raw,
        ) {
            Ok(k) => k,
            Err(e) => {
                return Response::Error {
                    message: format!("org key derivation failed: {}", e),
                }
            }
        };

        let keys = vault_keys;

        // Restore session tokens — Db::load() never has them (serde skip).
        // Try keyring first, then fall back to whatever was in memory before
        // locking (covers a lock→pin-unlock cycle without agent restart).
        // PIN unlock cannot do a silent re-auth (no master_password_hash),
        // so if tokens are unavailable server sync will fail until the user
        // logs out and back in.
        if db.access_token.is_none() && config.persist_session {
            match keyring::get_tokens(&config.server_name(), &email).await {
                Ok(Some((at, rt))) => {
                    db.access_token = Some(at.into());
                    db.refresh_token = Some(rt.into());
                }
                Ok(None) => {}
                Err(e) => log::warn!("pin unlock: could not load tokens from keyring: {}", e),
            }
        }

        let needs_reauth = {
            let mut g = state.lock().await;

            // In-memory copy covers lock→pin-unlock without agent restart when
            // persist_session is false or keyring was unavailable.
            if db.access_token.is_none() {
                if let Some(prev_db) = &g.db {
                    db.access_token = prev_db.access_token.clone();
                    db.refresh_token = prev_db.refresh_token.clone();
                }
            }

            let has_token = db.access_token.is_some();
            let has_hash = maybe_hash.is_some();

            if !has_token && !has_hash {
                log::warn!(
                    "pin unlock: no session token available for {} — \
                     server sync will fail until you log out and log in again",
                    email
                );
            }

            g.keys = Some(keys);
            g.org_keys = Some(org_keys);
            g.master_password_hash = maybe_hash;
            g.pinned_ids.clear();
            for entry in &db.entries {
                if entry.favorite {
                    g.pinned_ids.insert(entry.id.clone());
                }
            }
            g.db = Some(db);
            g.rebuild_sidebar_cache();
            g.broadcast(cosmic_bwarden_core::protocol::Event::Unlocked);
            !has_token && has_hash
        };

        // If we have a sealed hash but no session token, do a silent re-auth now
        // (same path as master-password unlock when tokens are missing).
        if needs_reauth {
            log::info!("pin unlock: no session token, attempting silent re-auth via sealed hash");
            let hash = {
                let g = state.lock().await;
                g.master_password_hash.clone()
            };
            if let Some(master_password_hash) = hash {
                let client = cosmic_bwarden_core::api::Client::new(
                    &config.base_url(),
                    &config.identity_url(),
                );
                match config.device_id().await {
                    Ok(device_id) => {
                        match client
                            .login(
                                &email,
                                &device_id,
                                &master_password_hash,
                                None,
                                None,
                                None,
                                None,
                            )
                            .await
                        {
                            Ok((access_token, refresh_token, _protected_key)) => {
                                log::info!("pin unlock: silent re-auth succeeded");
                                {
                                    let mut g = state.lock().await;
                                    if let Some(db) = &mut g.db {
                                        db.access_token = Some(access_token.clone().into());
                                        db.refresh_token =
                                            refresh_token.as_ref().map(|rt| rt.clone().into());
                                    }
                                }
                                if config.persist_session {
                                    if let Some(rt) = &refresh_token {
                                        if let Err(e) = keyring::store_tokens(
                                            &config.server_name(),
                                            &email,
                                            &access_token,
                                            rt,
                                        )
                                        .await
                                        {
                                            log::error!("pin unlock: failed to store refreshed tokens in keyring: {}", e);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("pin unlock: silent re-auth failed (sync will be unavailable): {}", e);
                            }
                        }
                    }
                    Err(e) => log::warn!(
                        "pin unlock: could not obtain device_id for silent re-auth: {}",
                        e
                    ),
                }
            }
        }

        log::info!("vault unlocked via TPM PIN for {}", email);
        Response::Ack
    }
    #[cfg(not(feature = "tpm"))]
    {
        let _ = (pin, state);
        Response::Error {
            message: "TPM support not compiled in this build".to_string(),
        }
    }
}
