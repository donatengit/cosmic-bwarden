//! Unlock the vault using TPM-sealed keys and a PIN.

#[cfg(feature = "tpm")]
use crate::keyring;
use crate::state::State;
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
        if let Some(msg) = super::flags::pin_unlock_blocked_reason(config.tpm_enabled) {
            return Response::Error {
                message: msg.to_string(),
            };
        }
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

        // The blob path is derived from the server URL, so changing that URL (or a
        // TPM reset) silently points this at a file that no longer exists. Say so
        // plainly: an unseal attempt would fail as ERR_TPM_UNSEAL_FAILED, which
        // reads as "wrong PIN" and makes the user retry — burning TPM
        // dictionary-attack attempts against a blob that isn't there.
        if !blob_path.exists() {
            log::error!(
                "pin unlock: TPM is enabled for {} but no sealed blob at {}",
                email,
                blob_path.display()
            );
            return Response::Error {
                message: cosmic_bwarden_core::protocol::ERR_TPM_BLOB_MISSING.to_string(),
            };
        }

        // Unseal the vault symmetric keys from the TPM (these are the same keys stored
        // in state.keys after a normal password unlock — NOT the identity/KDF keys).
        let vault_keys = match crate::tpm::unseal(&blob_path, &pin).await {
            Ok(k) => k,
            Err(e) => {
                // Wrong PIN / DA lockout vs. changed PCR state must be told
                // apart: the first consumed a DA attempt (and the PIN was
                // wrong), the second means the machine state changed and the
                // user needs master-password unlock to re-seal. The full chain
                // is log-only; clients key on the stable short message.
                match crate::tpm::classify_unseal_failure(&e) {
                    crate::tpm::UnsealFailure::StateChanged => {
                        log::warn!("TPM unseal failed: PCR state changed: {:#}", e);
                        return Response::Error {
                            message: cosmic_bwarden_core::protocol::ERR_TPM_STATE_CHANGED
                                .to_string(),
                        };
                    }
                    _ => {
                        log::error!("TPM unseal failed: {:#}", e);
                        return Response::Error {
                            message: cosmic_bwarden_core::protocol::ERR_TPM_UNSEAL_FAILED
                                .to_string(),
                        };
                    }
                }
            }
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

        // Kept out of the state commit below so the re-auth chain can use it
        // after `keys` has moved into `State`.
        let keys_for_reauth = vault_keys.clone();
        let keys = vault_keys;

        // Restore session tokens — Db::load() never has them (serde skip).
        // Try keyring first, then fall back to whatever was in memory before
        // locking (covers a lock→pin-unlock cycle without agent restart).
        if db.access_token.is_none() && config.persist_session {
            match keyring::get_tokens(&config.server_name(), &email).await {
                Ok(Some((at, rt))) => {
                    db.access_token = Some(at.into());
                    db.refresh_token = Some(rt.into());
                }
                Ok(None) => log::warn!(
                    "pin unlock: persist_session is on but the keyring holds no session for {} \
                     (is this build compiled with --features keyring?)",
                    email
                ),
                Err(e) => log::warn!("pin unlock: could not load tokens from keyring: {}", e),
            }
        }

        let has_token = {
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

            g.keys = Some(keys);
            g.org_keys = Some(org_keys);
            g.pinned_ids.clear();
            for entry in &db.entries {
                if entry.favorite {
                    g.pinned_ids.insert(entry.id.clone());
                }
            }
            g.db = Some(db);
            g.bump_epoch();
            g.rebuild_sidebar_cache();
            g.broadcast(cosmic_bwarden_core::protocol::Event::Unlocked);
            has_token
        };

        // No live token: re-mint one from the stored refresh-token envelope.
        // A PIN unlock never has the master password, so if that fails the user
        // is asked for it — the vault stays unlocked and usable offline either
        // way, only sync depends on this.
        let restored = if has_token {
            Ok(())
        } else {
            super::super::reauth::restore_session(state, &config, &email, &keys_for_reauth, None)
                .await
        };

        match restored {
            Ok(()) => {
                // Catch the vault up with the server now that we are unlocked and
                // have a session (clears a stale out-of-sync flag truthfully).
                let sync_state = Arc::clone(state);
                tokio::spawn(async move {
                    // Skip if the user re-locked before this ran: a sync without
                    // tokens would falsely mark the state out-of-sync.
                    let has_token = {
                        let g = sync_state.lock().await;
                        g.db.as_ref().is_some_and(|db| db.access_token.is_some())
                    };
                    if has_token {
                        let _ = crate::handler::vault::sync::handle_sync(&sync_state).await;
                    }
                });
            }
            Err(reason) => {
                // The unlock itself succeeded, but every server operation will
                // fail until a master-password unlock re-authenticates —
                // announce it instead of handing back a bare Ack that claims
                // full success. The reason travels to the UI, so a 2FA
                // challenge doesn't read as a generic network failure.
                log::error!("pin unlock: no session could be restored for {email}: {reason}");
                let mut g = state.lock().await;
                g.sync_failed = true;
                g.last_sync_error = Some(format!(
                    "{}: {reason}",
                    cosmic_bwarden_core::protocol::ERR_NO_SESSION
                ));
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
