use std::sync::Arc;
use tokio::sync::Mutex;
use crate::state::State;
use crate::keyring;
use cosmic_bwarden_core::db::Secret;

pub async fn with_refresh<F, Fut, T>(state: &Arc<Mutex<State>>, f: F) -> Result<T, String>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = cosmic_bwarden_core::error::Result<T>>,
{
    let config = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy()
        .map_err(|e| e.to_string())?;

    let (access_token, refresh_token, base_url, identity_url) = {
        let mut state_guard = state.lock().await;
        let db = state_guard.db.as_mut().ok_or("agent is locked")?;

        if db.access_token.is_none() && config.persist_session {
            if let Ok(Some((at, rt))) =
                keyring::get_tokens(&config.server_name(), config.email.as_ref().unwrap()).await
            {
                db.access_token = Some(Secret::from(at));
                db.refresh_token = Some(Secret::from(rt));
            }
        }

        (
            db.access_token
                .as_ref()
                .ok_or("no API session token — please logout and log in again to restore server sync")?
                .expose()
                .to_string(),
            db.refresh_token
                .as_ref()
                .ok_or("no refresh token")?
                .expose()
                .to_string(),
            config.base_url(),
            config.identity_url(),
        )
    };

    match f(access_token).await {
        Ok(res) => Ok(res),
        Err(cosmic_bwarden_core::error::Error::Other(e)) if e.contains("401") => {
            log::info!("access token expired, refreshing...");
            let client = cosmic_bwarden_core::api::Client::new(&base_url, &identity_url);
            match client.exchange_refresh_token(&refresh_token).await {
                Ok((new_at, new_rt, _new_key)) => {
                    {
                        let mut state_guard = state.lock().await;
                        if let Some(db) = &mut state_guard.db {
                            db.access_token = Some(Secret::from(new_at.clone()));
                            if let Some(rt) = new_rt {
                                db.refresh_token = Some(Secret::from(rt));
                            }

                            if config.persist_session {
                                let _ = keyring::store_tokens(
                                    &config.server_name(),
                                    config.email.as_ref().unwrap(),
                                    db.access_token.as_ref().unwrap().expose(),
                                    db.refresh_token.as_ref().unwrap().expose(),
                                )
                                .await;
                            }

                            let _ = db.save(&config.server_name(), config.email.as_ref().unwrap());
                        }
                    }
                    // Retry
                    f(new_at).await.map_err(|e| e.to_string())
                }
                Err(e) => Err(format!("refresh failed: {}", e)),
            }
        }
        Err(e) => Err(e.to_string()),
    }
}
