use std::sync::Arc;
use tokio::sync::Mutex;
use crate::state::State;
use cosmic_bwarden_core::db::Secret;
use super::auth::with_refresh;

pub async fn add_entry_on_server(
    state: &Arc<Mutex<State>>,
    entry: &cosmic_bwarden_core::db::Entry,
    config: &cosmic_bwarden_core::config::CosmicBWardenConfig,
    keys: &cosmic_bwarden_core::locked::Keys,
) -> Result<(), String> {
    let name_enc = match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
        keys,
        entry.name.as_bytes(),
    ) {
        Ok(cs) => cs.to_string(),
        Err(e) => return Err(e.to_string()),
    };

    let mut notes_enc = None;
    if let Some(n) = &entry.notes {
        notes_enc = Some(
            match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
                keys,
                n.expose().as_bytes(),
            ) {
                Ok(cs) => cs.to_string(),
                Err(e) => return Err(e.to_string()),
            },
        );
    }

    let (ty, u_enc, p_enc, priv_enc, pub_enc, card_payload, identity_payload) = match &entry.data {
        cosmic_bwarden_core::db::EntryData::Login {
            username,
            password,
            ..
        } => {
            let u_enc = if let Some(u) = username {
                Some(
                    match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
                        keys,
                        u.as_bytes(),
                    ) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Err(e.to_string()),
                    },
                )
            } else {
                None
            };
            let p_enc = if let Some(p) = password {
                Some(
                    match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
                        keys,
                        p.expose().as_bytes(),
                    ) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Err(e.to_string()),
                    },
                )
            } else {
                None
            };
            (1, u_enc, p_enc, None, None, None, None)
        }
        cosmic_bwarden_core::db::EntryData::SecureNote => (2, None, None, None, None, None, None),
        cosmic_bwarden_core::db::EntryData::Identity {
            first_name,
            last_name,
            address1,
            city,
            state,
            postal_code,
            country,
            email,
            phone,
            ..
        } => {
            let mut id = cosmic_bwarden_core::api::CipherIdentity {
                first_name: None,
                last_name: None,
                address1: None,
                city: None,
                state: None,
                postal_code: None,
                country: None,
                email: None,
                phone: None,
                title: None,
                middle_name: None,
                address2: None,
                address3: None,
                ssn: None,
                license_number: None,
                passport_number: None,
                username: None,
            };

            let encrypt = |s: &Option<String>| -> Result<Option<String>, String> {
                if let Some(s) = s {
                    Ok(Some(
                        cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(keys, s.as_bytes())
                            .map_err(|e| e.to_string())?
                            .to_string(),
                    ))
                } else {
                    Ok(None)
                }
            };

            id.first_name = encrypt(first_name)?;
            id.last_name = encrypt(last_name)?;
            id.address1 = encrypt(address1)?;
            id.city = encrypt(city)?;
            id.state = encrypt(state)?;
            id.postal_code = encrypt(postal_code)?;
            id.country = encrypt(country)?;
            id.email = encrypt(email)?;
            id.phone = encrypt(phone)?;

            (4, None, None, None, None, None, Some(id))
        }
        cosmic_bwarden_core::db::EntryData::Card {
            cardholder_name,
            brand,
            number,
            exp_month,
            exp_year,
            code,
        } => {
            let mut card = cosmic_bwarden_core::api::CipherCard {
                cardholder_name: None,
                brand: None,
                number: None,
                exp_month: None,
                exp_year: None,
                code: None,
            };

            let encrypt = |s: &Option<String>| -> Result<Option<String>, String> {
                if let Some(s) = s {
                    Ok(Some(
                        cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(keys, s.as_bytes())
                            .map_err(|e| e.to_string())?
                            .to_string(),
                    ))
                } else {
                    Ok(None)
                }
            };

            let encrypt_secret = |s: &Option<Secret>| -> Result<Option<String>, String> {
                if let Some(s) = s {
                    Ok(Some(
                        cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(keys, s.expose().as_bytes())
                            .map_err(|e| e.to_string())?
                            .to_string(),
                    ))
                } else {
                    Ok(None)
                }
            };

            card.cardholder_name = encrypt(cardholder_name)?;
            card.brand = encrypt(brand)?;
            card.number = encrypt_secret(number)?;
            card.exp_month = encrypt(exp_month)?;
            card.exp_year = encrypt(exp_year)?;
            card.code = encrypt_secret(code)?;

            (3, None, None, None, None, Some(card), None)
        }
        cosmic_bwarden_core::db::EntryData::SshKey {
            private_key,
            public_key,
            ..
        } => {
            let priv_enc = if let Some(pk) = private_key {
                Some(
                    match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
                        keys,
                        pk.expose().as_bytes(),
                    ) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Err(e.to_string()),
                    },
                )
            } else {
                None
            };
            let pub_enc = if let Some(pk) = public_key {
                Some(
                    match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
                        keys,
                        pk.as_bytes(),
                    ) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Err(e.to_string()),
                    },
                )
            } else {
                None
            };
            (5, None, None, priv_enc, pub_enc, None, None)
        }
    };

    let mut fields_enc = Vec::new();
    for field in &entry.fields {
        let name_enc = if let Some(n) = &field.name {
            Some(
                match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(
                    keys,
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
                    keys,
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
            ty: field.ty,
            name: name_enc,
            value: value_enc,
            linked_id: field.linked_id,
        });
    }

    with_refresh(state, |at| {
        let name_enc = name_enc.clone();
        let u_enc = u_enc.clone();
        let p_enc = p_enc.clone();
        let priv_enc = priv_enc.clone();
        let pub_enc = pub_enc.clone();
        let card_payload = card_payload.clone();
        let identity_payload = identity_payload.clone();
        let notes_enc = notes_enc.clone();
        let fields_enc = fields_enc.clone();
        let base_url = config.base_url();
        let identity_url = config.identity_url();
        let favorite = entry.favorite;
        async move {
            let client = cosmic_bwarden_core::api::Client::new(&base_url, &identity_url);
            match ty {
                1 | 2 => {
                    client
                        .add_cipher(
                            &at,
                            ty,
                            &name_enc,
                            favorite,
                            u_enc.as_deref(),
                            p_enc.as_deref(),
                            notes_enc.as_deref(),
                            Some(fields_enc),
                        )
                        .await
                }
                5 => {
                    client
                        .add_ssh_key(
                            &at,
                            &name_enc,
                            favorite,
                            priv_enc.as_deref().unwrap_or(""),
                            pub_enc.as_deref(),
                            notes_enc.as_deref(),
                            Some(fields_enc),
                        )
                        .await
                }
                3 => {
                    let card = card_payload.unwrap();
                    client
                        .add_card(
                            &at,
                            &name_enc,
                            favorite,
                            card.cardholder_name.as_deref(),
                            card.brand.as_deref(),
                            card.number.as_deref(),
                            card.exp_month.as_deref(),
                            card.exp_year.as_deref(),
                            card.code.as_deref(),
                            notes_enc.as_deref(),
                            Some(fields_enc),
                        )
                        .await
                }
                4 => {
                    let id = identity_payload.unwrap();
                    client
                        .add_identity(
                            &at,
                            &name_enc,
                            favorite,
                            id.first_name.as_deref(),
                            id.last_name.as_deref(),
                            id.address1.as_deref(),
                            id.city.as_deref(),
                            id.state.as_deref(),
                            id.postal_code.as_deref(),
                            id.country.as_deref(),
                            id.email.as_deref(),
                            id.phone.as_deref(),
                            notes_enc.as_deref(),
                            Some(fields_enc),
                        )
                        .await
                }
                _ => unreachable!(),
            }
        }
    })
    .await
}

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

    let (ty, login_payload, ssh_key_payload, card_payload, identity_payload) = match &entry.data {
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
                None,
                None,
            )
        }
        cosmic_bwarden_core::db::EntryData::SecureNote => (2, None, None, None, None),
        cosmic_bwarden_core::db::EntryData::Identity {
            first_name,
            last_name,
            address1,
            city,
            state,
            postal_code,
            country,
            email,
            phone,
            ..
        } => {
            let mut id = cosmic_bwarden_core::api::CipherIdentity {
                first_name: None,
                last_name: None,
                address1: None,
                city: None,
                state: None,
                postal_code: None,
                country: None,
                email: None,
                phone: None,
                title: None,
                middle_name: None,
                address2: None,
                address3: None,
                ssn: None,
                license_number: None,
                passport_number: None,
                username: None,
            };

            let encrypt = |s: &Option<String>| -> Result<Option<String>, String> {
                if let Some(s) = s {
                    Ok(Some(
                        cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(crypt_keys, s.as_bytes())
                            .map_err(|e| e.to_string())?
                            .to_string(),
                    ))
                } else {
                    Ok(None)
                }
            };

            id.first_name = encrypt(first_name)?;
            id.last_name = encrypt(last_name)?;
            id.address1 = encrypt(address1)?;
            id.city = encrypt(city)?;
            id.state = encrypt(state)?;
            id.postal_code = encrypt(postal_code)?;
            id.country = encrypt(country)?;
            id.email = encrypt(email)?;
            id.phone = encrypt(phone)?;

            (4, None, None, None, Some(id))
        }
        cosmic_bwarden_core::db::EntryData::Card {
            cardholder_name,
            brand,
            number,
            exp_month,
            exp_year,
            code,
        } => {
            let mut card = cosmic_bwarden_core::api::CipherCard {
                cardholder_name: None,
                brand: None,
                number: None,
                exp_month: None,
                exp_year: None,
                code: None,
            };

            let encrypt = |s: &Option<String>| -> Result<Option<String>, String> {
                if let Some(s) = s {
                    Ok(Some(
                        cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(crypt_keys, s.as_bytes())
                            .map_err(|e| e.to_string())?
                            .to_string(),
                    ))
                } else {
                    Ok(None)
                }
            };

            let encrypt_secret = |s: &Option<Secret>| -> Result<Option<String>, String> {
                if let Some(s) = s {
                    Ok(Some(
                        cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(crypt_keys, s.expose().as_bytes())
                            .map_err(|e| e.to_string())?
                            .to_string(),
                    ))
                } else {
                    Ok(None)
                }
            };

            card.cardholder_name = encrypt(cardholder_name)?;
            card.brand = encrypt(brand)?;
            card.number = encrypt_secret(number)?;
            card.exp_month = encrypt(exp_month)?;
            card.exp_year = encrypt(exp_year)?;
            card.code = encrypt_secret(code)?;

            (3, None, None, Some(card), None)
        }
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
                Some(cosmic_bwarden_core::api::CipherSshKey {
                    private_key: priv_enc,
                    public_key: pub_enc,
                    fingerprint: None,
                }),
                None,
                None,
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
            ty: field.ty,
            name: name_enc,
            value: value_enc,
            linked_id: field.linked_id,
        });
    }

    let entry_id = entry.id.clone();
    let reprompt = entry.master_password_reprompt;
    let favorite = entry.favorite;

    with_refresh(state, |at| {
        let entry_id = entry_id.clone();
        let name_enc = name_enc.clone();
        let login_payload = login_payload.clone();
        let ssh_key_payload = ssh_key_payload.clone();
        let card_payload = card_payload.clone();
        let identity_payload = identity_payload.clone();
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
                    favorite,
                    ty,
                    login_payload,
                    ssh_key_payload,
                    card_payload,
                    identity_payload,
                    notes_enc.as_deref(),
                    Some(reprompt),
                    Some(fields_enc),
                )
                .await
        }
    })
    .await
}
