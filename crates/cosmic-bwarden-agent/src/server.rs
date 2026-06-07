use std::sync::Arc;
use tokio::sync::Mutex;
use crate::state::State;
use crate::keyring;
use cosmic_bwarden_core::db::Secret;

pub async fn update_entry_on_server(
    state: &Arc<Mutex<State>>,
    entry: &cosmic_bwarden_core::db::Entry,
    config: &cosmic_bwarden_core::config::CosmicBWardenConfig,
    keys: &cosmic_bwarden_core::locked::Keys,
) -> Result<(), String> {
    let entry_key = if let Some(ek) = &entry.key {
        match cosmic_bwarden_core::cipherstring::CipherString::new(ek) {
            Ok(ek_cipher) => match ek_cipher.decrypt_locked_symmetric(keys) {
                Ok(k) => Some(cosmic_bwarden_core::locked::Keys::new(k)),
                Err(e) => return Err(format!("failed to decrypt entry key: {}", e)),
            },
            Err(e) => return Err(format!("invalid entry key: {}", e)),
        }
    } else {
        None
    };
    let crypt_keys = entry_key.as_ref().unwrap_or(keys);

    let name_enc = match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
        crypt_keys,
        entry.name.as_bytes(),
    ) {
        Ok(cs) => cs.to_string(),
        Err(e) => return Err(e.to_string()),
    };

    let mut notes_enc = None;
    if let Some(n) = &entry.notes {
        notes_enc = Some(
            match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
                crypt_keys,
                n.expose().as_bytes(),
            ) {
                Ok(cs) => cs.to_string(),
                Err(e) => return Err(e.to_string()),
            },
        );
    }

    let (ty, login_payload, ssh_key_payload) = match &entry.data {
        cosmic_bwarden_core::db::EntryData::Login {
            username,
            password,
            totp,
            ..
        } => {
            let mut u_enc = None;
            if let Some(u) = username {
                u_enc = Some(
                    match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
                        crypt_keys,
                        u.as_bytes(),
                    ) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Err(e.to_string()),
                    },
                );
            }
            let mut p_enc = None;
            if let Some(p) = password {
                p_enc = Some(
                    match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
                        crypt_keys,
                        p.expose().as_bytes(),
                    ) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Err(e.to_string()),
                    },
                );
            }
            let mut totp_enc = None;
            if let Some(t) = totp {
                totp_enc = Some(
                    match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
                        crypt_keys,
                        t.expose().as_bytes(),
                    ) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Err(e.to_string()),
                    },
                );
            }
            (
                1,
                Some(serde_json::json!({ "username": u_enc, "password": p_enc, "totp": totp_enc })),
                None,
            )
        }
        cosmic_bwarden_core::db::EntryData::SecureNote => (2, None, None),
        cosmic_bwarden_core::db::EntryData::Identity { .. } => (4, None, None),
        cosmic_bwarden_core::db::EntryData::Card { .. } => (3, None, None),
        cosmic_bwarden_core::db::EntryData::SshKey {
            private_key,
            public_key,
            ..
        } => {
            let mut priv_enc = None;
            if let Some(pk) = private_key {
                priv_enc = Some(
                    match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
                        crypt_keys,
                        pk.expose().as_bytes(),
                    ) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Err(e.to_string()),
                    },
                );
            }
            let mut pub_enc = None;
            if let Some(pk) = public_key {
                pub_enc = Some(
                    match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
                        crypt_keys,
                        pk.as_bytes(),
                    ) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Err(e.to_string()),
                    },
                );
            }
            (
                5,
                None,
                Some(serde_json::json!({ "privateKey": priv_enc, "publicKey": pub_enc })),
            )
        }
    };

    let mut fields_enc = Vec::new();
    for field in &entry.fields {
        let name_enc = if let Some(n) = &field.name {
            Some(
                match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
                    crypt_keys,
                    n.as_bytes(),
                ) {
                    Ok(cs) => cs.to_string(),
                    Err(e) => return Err(e.to_string()),
                },
            )
        } else {
            None
        };
        let value_enc = if let Some(v) = &field.value {
            Some(
                match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
                    crypt_keys,
                    v.expose().as_bytes(),
                ) {
                    Ok(cs) => cs.to_string(),
                    Err(e) => return Err(e.to_string()),
                },
            )
        } else {
            None
        };
        fields_enc.push(cosmic_bwarden_core::api::CipherField {
            ty: field.ty.map(|t| match t {
                cosmic_bwarden_core::api::FieldType::Text => {
                    cosmic_bwarden_core::api::ApiFieldType::Text
                }
                cosmic_bwarden_core::api::FieldType::Boolean => {
                    cosmic_bwarden_core::api::ApiFieldType::Boolean
                }
                cosmic_bwarden_core::api::FieldType::Hidden => {
                    cosmic_bwarden_core::api::ApiFieldType::Hidden
                }
                cosmic_bwarden_core::api::FieldType::Linked => {
                    cosmic_bwarden_core::api::ApiFieldType::Linked
                }
            }),
            name: name_enc,
            value: value_enc,
            linked_id: field.linked_id.map(|l| match l {
                cosmic_bwarden_core::api::LinkedIdType::Username => {
                    cosmic_bwarden_core::api::ApiLinkedIdType::LoginUsername
                }
                cosmic_bwarden_core::api::LinkedIdType::Password => {
                    cosmic_bwarden_core::api::ApiLinkedIdType::LoginPassword
                }
                _ => cosmic_bwarden_core::api::ApiLinkedIdType::LoginUsername,
            }),
        });
    }

    let entry_id = entry.id.clone();
    let reprompt = entry.master_password_reprompt;

    with_refresh(state, |at| {
        let entry_id = entry_id.clone();
        let name_enc = name_enc.clone();
        let login_payload = login_payload.clone();
        let ssh_key_payload = ssh_key_payload.clone();
        let notes_enc = notes_enc.clone();
        let fields_enc = fields_enc.clone();
        let base_url = config.base_url();
        let identity_url = config.identity_url();
        async move {
            let client = cosmic_bwarden_core::api::Client::new(&base_url, &identity_url);
            client
                .update_cipher(
                    &at,
                    &entry_id,
                    &name_enc,
                    ty,
                    login_payload,
                    ssh_key_payload,
                    notes_enc.as_deref(),
                    Some(reprompt),
                    Some(fields_enc),
                )
                .await
        }
    })
    .await
}

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
                .ok_or("not logged in")?
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
