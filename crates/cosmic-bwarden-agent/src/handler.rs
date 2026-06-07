use std::sync::Arc;
use tokio::sync::Mutex;
use crate::state::State;
use crate::keyring;
use crate::server::{with_refresh, update_entry_on_server};
use cosmic_bwarden_core::protocol::{Action, Response, SidebarEntry, Event, EntryType};
use cosmic_bwarden_core::db::Secret;

pub async fn handle_request(action: Action, state: &Arc<Mutex<State>>) -> Response {
    log::info!("Received action: {:?}", action);
    match action {
        Action::Version => Response::Version {
            version: cosmic_bwarden_core::version().to_string(),
        },
        Action::GetConfig => {
            let state_guard = state.lock().await;
            let is_locked = state_guard.keys.is_none();
            let state_db_needs_login = state_guard.db.as_ref().map(|db| db.needs_login());

            match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                Ok(config) => {
                    let needs_login = if let Some(needs) = state_db_needs_login {
                        needs
                    } else if let Some(email) = &config.email {
                        match cosmic_bwarden_core::db::Db::load(&config.server_name(), email) {
                            Ok(db) => db.needs_login(),
                            Err(_) => true,
                        }
                    } else {
                        true
                    };
                    Response::Config {
                        config,
                        needs_login,
                        is_locked,
                    }
                }
                Err(e) => Response::Error {
                    message: format!("failed to load config: {}", e),
                },
            }
        }
        Action::Register {
            email,
            password,
            server_url,
        } => {
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
                    }
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
                        }
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
            let mut config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                Ok(c) => c,
                Err(_) => cosmic_bwarden_core::config::CosmicBWardenConfig::default(),
            };
            config.base_url = server_url;
            if remember_me {
                config.email = Some(email.clone());
            } else {
                config.email = None;
            }

            let device_id = match config.device_id().await {
                Ok(id) => id,
                Err(e) => {
                    return Response::Error {
                        message: format!("failed to get device id: {}", e),
                    }
                }
            };

            let client =
                cosmic_bwarden_core::api::Client::new(&config.base_url(), &config.identity_url());

            let mut pw_vec = cosmic_bwarden_core::locked::Vec::new();
            pw_vec.extend(password.as_bytes().iter().copied());
            let pw = cosmic_bwarden_core::locked::Password::new(pw_vec);

            let (kdf, iterations, memory, parallelism) = match client.prelogin(&email).await {
                Ok(res) => res,
                Err(e) => {
                    return Response::Error {
                        message: format!("prelogin failed: {}", e),
                    }
                }
            };

            let identity = match cosmic_bwarden_core::identity::Identity::new(
                &email,
                &pw,
                kdf,
                iterations,
                memory,
                parallelism,
            ) {
                Ok(id) => id,
                Err(e) => {
                    return Response::Error {
                        message: format!("identity derivation failed: {}", e),
                    }
                }
            };

            let (access_token, refresh_token, protected_key) = match client
                .login(
                    &email,
                    &device_id,
                    &identity.master_password_hash,
                    two_factor_token.as_deref(),
                    two_factor_provider,
                    two_factor_code.as_deref(),
                    device_verification_code.as_deref(),
                )
                .await
            {
                Ok(res) => res,
                Err(cosmic_bwarden_core::error::Error::TwoFactorRequired { providers, token }) => {
                    return Response::Error {
                        message: format!(
                            "two_factor_required:{}:{}",
                            token,
                            serde_json::to_string(&providers).unwrap()
                        ),
                    };
                }
                Err(cosmic_bwarden_core::error::Error::NewDeviceVerificationRequired) => {
                    return Response::Error {
                        message: "new_device_verification_required".to_string(),
                    };
                }
                Err(e) => {
                    return Response::Error {
                        message: format!("login failed: {}", e),
                    }
                }
            };

            if config.persist_session {
                if let Err(e) = keyring::store_tokens(
                    &config.server_name(),
                    &email,
                    &access_token,
                    &refresh_token,
                )
                .await
                {
                    log::error!("failed to store tokens in keyring: {}", e);
                }
            }

            let mut db = cosmic_bwarden_core::db::Db::load(&config.server_name(), &email)
                .unwrap_or_else(|_| cosmic_bwarden_core::db::Db::new());
            db.access_token = Some(Secret::from(access_token.clone()));
            db.refresh_token = Some(Secret::from(refresh_token));
            db.kdf = Some(kdf);
            db.iterations = Some(iterations);
            db.memory = memory;
            db.parallelism = parallelism;
            db.protected_key = Some(Secret::from(protected_key));

            match client.sync(&access_token).await {
                Ok((pk, ppk, pok, entries)) => {
                    db.protected_key = Some(Secret::from(pk));
                    db.protected_private_key = ppk.map(Secret::from);
                    db.protected_org_keys =
                        pok.into_iter().map(|(k, v)| (k, Secret::from(v))).collect();
                    db.entries = entries;
                }
                Err(e) => {
                    return Response::Error {
                        message: format!("initial sync failed: {}", e),
                    }
                }
            }

            if let Err(e) = db.save(&config.server_name(), &email) {
                return Response::Error {
                    message: format!("failed to save db: {}", e),
                };
            }
            if let Err(e) = config.save_legacy() {
                return Response::Error {
                    message: format!("failed to save config: {}", e),
                };
            }

            match cosmic_bwarden_core::vault::unlock(
                &email,
                &pw,
                kdf,
                iterations,
                memory,
                parallelism,
                db.protected_key.as_ref().map(|s| s.expose()).unwrap_or(""),
                db.protected_private_key.as_ref().map(|s| s.expose()),
                &db.protected_org_keys
                    .iter()
                    .map(|(k, v)| (k.clone(), v.expose().to_string()))
                    .collect::<std::collections::HashMap<_, _>>(),
            ) {
                Ok((keys, org_keys)) => {
                    let mut state_guard = state.lock().await;

                    // Populate pinned_ids from custom fields
                    let mut pinned_from_fields = std::collections::HashSet::new();
                    for entry in &db.entries {
                        for field in &entry.fields {
                            if let Some(name) = &field.name {
                                if let Ok(dec_name) = cosmic_bwarden_core::vault::decrypt(
                                    name,
                                    &keys,
                                    entry.key.as_deref(),
                                ) {
                                    if dec_name == "corbw-pinned" {
                                        if let Some(value) = &field.value {
                                            if let Ok(dec_value) =
                                                cosmic_bwarden_core::vault::decrypt(
                                                    value,
                                                    &keys,
                                                    entry.key.as_deref(),
                                                )
                                            {
                                                if dec_value == "true" {
                                                    pinned_from_fields.insert(entry.id.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    state_guard.keys = Some(keys);
                    state_guard.org_keys = Some(org_keys);
                    state_guard.master_password_hash = Some(identity.master_password_hash);

                    // Populate pinned_ids from custom fields
                    let mut db = db;
                    db.pinned_ids.clear();
                    for id in pinned_from_fields {
                        db.pinned_ids.insert(id);
                    }
                    state_guard.db = Some(db);

                    state_guard.broadcast(Event::Unlocked);
                    Response::Ack
                }
                Err(e) => Response::Error {
                    message: format!("unlock failed after login: {}", e),
                },
            }
        }
        Action::Unlock { password } => {
            let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                Ok(c) => c,
                Err(e) => {
                    return Response::Error {
                        message: format!("failed to load config: {}", e),
                    }
                }
            };
            let email = match config.email.as_ref() {
                Some(e) => e,
                None => {
                    return Response::Error {
                        message: "email not set in config. Please login.".to_string(),
                    }
                }
            };
            let mut db = match cosmic_bwarden_core::db::Db::load(&config.server_name(), email) {
                Ok(d) => d,
                Err(e) => {
                    return Response::Error {
                        message: format!("failed to load db: {}", e),
                    }
                }
            };

            if db.access_token.is_none() && config.persist_session {
                match keyring::get_tokens(&config.server_name(), email).await {
                    Ok(Some((at, rt))) => {
                        db.access_token = Some(Secret::from(at));
                        db.refresh_token = Some(Secret::from(rt));
                    }
                    _ => {}
                }
            }

            let mut pw_vec = cosmic_bwarden_core::locked::Vec::new();
            pw_vec.extend(password.as_bytes().iter().copied());
            let pw = cosmic_bwarden_core::locked::Password::new(pw_vec);

            let kdf = db.kdf.unwrap_or(cosmic_bwarden_core::api::KdfType::Pbkdf2);
            let iterations = db.iterations.unwrap_or(100000);

            let identity = match cosmic_bwarden_core::identity::Identity::new(
                email,
                &pw,
                kdf,
                iterations,
                db.memory,
                db.parallelism,
            ) {
                Ok(id) => id,
                Err(e) => {
                    return Response::Error {
                        message: format!("identity derivation failed: {}", e),
                    }
                }
            };

            match cosmic_bwarden_core::vault::unlock(
                email,
                &pw,
                kdf,
                iterations,
                db.memory,
                db.parallelism,
                db.protected_key.as_ref().map(|s| s.expose()).unwrap_or(""),
                db.protected_private_key.as_ref().map(|s| s.expose()),
                &db.protected_org_keys
                    .iter()
                    .map(|(k, v)| (k.clone(), v.expose().to_string()))
                    .collect::<std::collections::HashMap<_, _>>(),
            ) {
                Ok((keys, org_keys)) => {
                    let mut state_guard = state.lock().await;

                    // Populate pinned_ids from custom fields
                    let mut pinned_from_fields = std::collections::HashSet::new();
                    for entry in &db.entries {
                        for field in &entry.fields {
                            if let Some(name) = &field.name {
                                if let Ok(dec_name) = cosmic_bwarden_core::vault::decrypt(
                                    name,
                                    &keys,
                                    entry.key.as_deref(),
                                ) {
                                    if dec_name == "corbw-pinned" {
                                        if let Some(value) = &field.value {
                                            if let Ok(dec_value) =
                                                cosmic_bwarden_core::vault::decrypt(
                                                    value,
                                                    &keys,
                                                    entry.key.as_deref(),
                                                )
                                            {
                                                if dec_value == "true" {
                                                    pinned_from_fields.insert(entry.id.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    state_guard.keys = Some(keys);
                    state_guard.org_keys = Some(org_keys);
                    state_guard.master_password_hash = Some(identity.master_password_hash);

                    // Populate pinned_ids from custom fields
                    let mut db = db;
                    db.pinned_ids.clear();
                    for id in pinned_from_fields {
                        db.pinned_ids.insert(id);
                    }
                    state_guard.db = Some(db);

                    state_guard.broadcast(Event::Unlocked);
                    Response::Ack
                }
                Err(e) => Response::Error {
                    message: format!("unlock failed: {}", e),
                },
            }
        }
        Action::Lock => {
            let mut state = state.lock().await;
            state.lock();
            Response::Ack
        }
        Action::Logout => {
            let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                Ok(c) => c,
                Err(e) => {
                    return Response::Error {
                        message: format!("failed to load config: {}", e),
                    }
                }
            };
            if let Some(email) = &config.email {
                if config.persist_session {
                    let _ = keyring::delete_tokens(&config.server_name(), email).await;
                }
                let db = cosmic_bwarden_core::db::Db::new();
                let _ = db.save(&config.server_name(), email);
            }
            let mut state = state.lock().await;
            state.lock();
            Response::Ack
        }
        Action::Sync => {
            let res = with_refresh(state, |at| async move {
                let config = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy()?;
                let client = cosmic_bwarden_core::api::Client::new(
                    &config.base_url(),
                    &config.identity_url(),
                );
                client.sync(&at).await
            })
            .await;

            match res {
                Ok((protected_key, protected_private_key, protected_org_keys, entries)) => {
                    let mut state_guard = state.lock().await;
                    let keys = state_guard.keys.clone();
                    if let (Some(db), Some(keys)) = (&mut state_guard.db, &keys) {
                        db.protected_key = Some(Secret::from(protected_key));
                        db.protected_private_key = protected_private_key.map(Secret::from);
                        db.protected_org_keys = protected_org_keys
                            .into_iter()
                            .map(|(k, v)| (k, Secret::from(v)))
                            .collect();
                        db.entries = entries;

                        db.pinned_ids.clear();
                        for entry in &db.entries {
                            for field in &entry.fields {
                                if let Some(name) = &field.name {
                                    if let Ok(dec_name) = cosmic_bwarden_core::vault::decrypt(
                                        name,
                                        keys,
                                        entry.key.as_deref(),
                                    ) {
                                        if dec_name == "corbw-pinned" {
                                            if let Some(value) = &field.value {
                                                if let Ok(dec_value) =
                                                    cosmic_bwarden_core::vault::decrypt(
                                                        value,
                                                        keys,
                                                        entry.key.as_deref(),
                                                    )
                                                {
                                                    if dec_value == "true" {
                                                        db.pinned_ids.insert(entry.id.clone());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        let config =
                            cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy()
                                .unwrap();
                        let _ = db.save(&config.server_name(), config.email.as_ref().unwrap());
                        state_guard.name_cache.clear();
                        state_guard.username_cache.clear();
                        state_guard.broadcast(Event::VaultChanged);
                        Response::Ack
                    } else {
                        Response::Error {
                            message: "agent is locked".to_string(),
                        }
                    }
                }
                Err(e) => Response::Error {
                    message: format!("sync failed: {}", e),
                },
            }
        }
        Action::Subscribe => Response::Ack,
        Action::GetEntries { query, entry_type } => {
            let state = state.lock().await;
            if let (Some(db), Some(keys)) = (&state.db, &state.keys) {
                let mut entries = Vec::new();
                for entry in &db.entries {
                    if let Some(et) = entry_type {
                        match (et, &entry.data) {
                            (EntryType::Login, cosmic_bwarden_core::db::EntryData::Login { .. }) => (),
                            (EntryType::Card, cosmic_bwarden_core::db::EntryData::Card { .. }) => (),
                            (EntryType::Identity, cosmic_bwarden_core::db::EntryData::Identity { .. }) => (),
                            (EntryType::SecureNote, cosmic_bwarden_core::db::EntryData::SecureNote) => (),
                            (EntryType::SshKey, cosmic_bwarden_core::db::EntryData::SshKey { .. }) => (),
                            _ => continue,
                        }
                    }

                    let mut decrypted_entry = entry.clone();
                    if let Ok(decrypted_name) =
                        cosmic_bwarden_core::vault::decrypt(&entry.name, keys, entry.key.as_deref())
                    {
                        decrypted_entry.name = decrypted_name;
                    }
                    if let cosmic_bwarden_core::db::EntryData::Login { username, .. } =
                        &mut decrypted_entry.data
                    {
                        if let Some(u) = username {
                            if let Ok(dec_u) =
                                cosmic_bwarden_core::vault::decrypt(u, keys, entry.key.as_deref())
                            {
                                *username = Some(dec_u);
                            }
                        }
                    }
                    entries.push(decrypted_entry);
                }

                let entries = if let Some(q) = query {
                    let q = q.to_lowercase();
                    entries
                        .into_iter()
                        .filter(|e| {
                            if e.name.to_lowercase().contains(&q) || e.id == q {
                                return true;
                            }
                            if let cosmic_bwarden_core::db::EntryData::Login {
                                username: Some(u),
                                ..
                            } = &e.data
                            {
                                if u.to_lowercase().contains(&q) {
                                    return true;
                                }
                            }
                            false
                        })
                        .collect()
                } else {
                    entries
                };
                Response::Entries { entries }
            } else {
                Response::Error {
                    message: "agent is locked".to_string(),
                }
            }
        }
        Action::GetSidebarEntries { query, entry_type } => {
            let mut state_guard = state.lock().await;
            if let (Some(db), Some(keys)) = (&state_guard.db, &state_guard.keys) {
                let mut entries = Vec::new();
                let mut new_names = Vec::new();
                let mut new_usernames = Vec::new();

                for entry in &db.entries {
                    if let Some(et) = entry_type {
                        match (et, &entry.data) {
                            (EntryType::Login, cosmic_bwarden_core::db::EntryData::Login { .. }) => (),
                            (EntryType::Card, cosmic_bwarden_core::db::EntryData::Card { .. }) => (),
                            (EntryType::Identity, cosmic_bwarden_core::db::EntryData::Identity { .. }) => (),
                            (EntryType::SecureNote, cosmic_bwarden_core::db::EntryData::SecureNote) => (),
                            (EntryType::SshKey, cosmic_bwarden_core::db::EntryData::SshKey { .. }) => (),
                            _ => continue,
                        }
                    }

                    let decrypted_name = if let Some(name) = state_guard.name_cache.get(&entry.id) {
                        name.clone()
                    } else {
                        let name = match cosmic_bwarden_core::vault::decrypt(
                            &entry.name,
                            keys,
                            entry.key.as_deref(),
                        ) {
                            Ok(n) => n,
                            Err(_) => entry.name.clone(),
                        };
                        new_names.push((entry.id.clone(), name.clone()));
                        name
                    };

                    let username_dec = if let Some(u) = state_guard.username_cache.get(&entry.id) {
                        Some(u.clone())
                    } else {
                        let mut u_dec = None;
                        if let cosmic_bwarden_core::db::EntryData::Login { username, .. } =
                            &entry.data
                        {
                            if let Some(u) = username {
                                u_dec = Some(
                                    match cosmic_bwarden_core::vault::decrypt(
                                        u,
                                        keys,
                                        entry.key.as_deref(),
                                    ) {
                                        Ok(dec_u) => dec_u,
                                        Err(_) => u.clone(),
                                    },
                                );
                            }
                        }
                        if let Some(u) = &u_dec {
                            new_usernames.push((entry.id.clone(), u.clone()));
                        }
                        u_dec
                    };

                    entries.push((
                        SidebarEntry {
                            id: entry.id.clone(),
                            name: decrypted_name,
                            entry_type: match &entry.data {
                                cosmic_bwarden_core::db::EntryData::Login { .. } => EntryType::Login,
                                cosmic_bwarden_core::db::EntryData::Card { .. } => EntryType::Card,
                                cosmic_bwarden_core::db::EntryData::Identity { .. } => EntryType::Identity,
                                cosmic_bwarden_core::db::EntryData::SecureNote => EntryType::SecureNote,
                                cosmic_bwarden_core::db::EntryData::SshKey { .. } => EntryType::SshKey,
                            },
                            is_pinned: db.pinned_ids.contains(&entry.id),
                        },
                        username_dec,
                    ));
                }

                for (id, name) in new_names {
                    state_guard.name_cache.insert(id, name);
                }
                for (id, username) in new_usernames {
                    state_guard.username_cache.insert(id, username);
                }

                let entries = if let Some(q) = query {
                    let q = q.to_lowercase();
                    entries
                        .into_iter()
                        .filter(|(e, u)| {
                            e.name.to_lowercase().contains(&q)
                                || e.id == q
                                || u.as_ref()
                                    .map(|un| un.to_lowercase().contains(&q))
                                    .unwrap_or(false)
                        })
                        .map(|(e, _)| e)
                        .collect()
                } else {
                    entries.into_iter().map(|(e, _)| e).collect()
                };
                Response::SidebarEntries { entries }
            } else {
                Response::Error {
                    message: "agent is locked".to_string(),
                }
            }
        }
        Action::GetEntry { id, password } => {
            let state = state.lock().await;
            if let (Some(db), Some(keys)) = (&state.db, &state.keys) {
                if let Some(entry) = db.entries.iter().find(|e| e.id == id) {
                    if entry.master_password_reprompt() {
                        let password = match password {
                            Some(p) => p,
                            None => {
                                return Response::Error {
                                    message: "reprompt_required".to_string(),
                                }
                            }
                        };

                        let config =
                            match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                                Ok(c) => c,
                                Err(e) => {
                                    return Response::Error {
                                        message: format!("failed to load config: {}", e),
                                    }
                                }
                            };
                        let email = match config.email.as_ref() {
                            Some(e) => e,
                            None => {
                                return Response::Error {
                                    message: "email not set in config".to_string(),
                                }
                            }
                        };

                        let mut pw_vec = cosmic_bwarden_core::locked::Vec::new();
                        pw_vec.extend(password.as_bytes().iter().copied());
                        let pw = cosmic_bwarden_core::locked::Password::new(pw_vec);

                        let kdf = db.kdf.unwrap_or(cosmic_bwarden_core::api::KdfType::Pbkdf2);
                        let iterations = db.iterations.unwrap_or(100000);

                        let identity = match cosmic_bwarden_core::identity::Identity::new(
                            email,
                            &pw,
                            kdf,
                            iterations,
                            db.memory,
                            db.parallelism,
                        ) {
                            Ok(id) => id,
                            Err(e) => {
                                return Response::Error {
                                    message: format!("identity derivation failed: {}", e),
                                }
                            }
                        };

                        if let Some(stored_hash) = &state.master_password_hash {
                            if identity.master_password_hash.hash() != stored_hash.hash() {
                                return Response::Error {
                                    message: "incorrect password".to_string(),
                                };
                            }
                        } else {
                            return Response::Error {
                                message: "agent state inconsistent".to_string(),
                            };
                        }
                    }

                    let mut decrypted_entry = entry.clone();
                    if let Ok(decrypted_name) =
                        cosmic_bwarden_core::vault::decrypt(&entry.name, keys, entry.key.as_deref())
                    {
                        decrypted_entry.name = decrypted_name;
                    }
                    if let Some(notes) = &entry.notes {
                        if let Ok(dec_notes) = cosmic_bwarden_core::vault::decrypt(
                            notes.expose(),
                            keys,
                            entry.key.as_deref(),
                        ) {
                            decrypted_entry.notes = Some(Secret::from(dec_notes));
                        }
                    }

                    match &mut decrypted_entry.data {
                        cosmic_bwarden_core::db::EntryData::Login {
                            username,
                            password,
                            totp,
                            ..
                        } => {
                            if let Some(u) = username {
                                if let Ok(dec_u) = cosmic_bwarden_core::vault::decrypt(
                                    u,
                                    keys,
                                    entry.key.as_deref(),
                                ) {
                                    *username = Some(dec_u);
                                }
                            }
                            if let Some(p) = password {
                                if let Ok(dec_p) = cosmic_bwarden_core::vault::decrypt(
                                    p.expose(),
                                    keys,
                                    entry.key.as_deref(),
                                ) {
                                    *password = Some(Secret::from(dec_p));
                                }
                            }
                            if let Some(t) = totp {
                                if let Ok(dec_t) = cosmic_bwarden_core::vault::decrypt(
                                    t.expose(),
                                    keys,
                                    entry.key.as_deref(),
                                ) {
                                    *totp = Some(Secret::from(dec_t));
                                }
                            }
                        }
                        cosmic_bwarden_core::db::EntryData::SecureNote => {}
                        cosmic_bwarden_core::db::EntryData::SshKey {
                            private_key,
                            public_key,
                            ..
                        } => {
                            if let Some(pk) = private_key {
                                if let Ok(dec_pk) = cosmic_bwarden_core::vault::decrypt(
                                    pk.expose(),
                                    keys,
                                    entry.key.as_deref(),
                                ) {
                                    *private_key = Some(Secret::from(dec_pk));
                                }
                            }
                            if let Some(pubk) = public_key {
                                if let Ok(dec_pubk) = cosmic_bwarden_core::vault::decrypt(
                                    pubk,
                                    keys,
                                    entry.key.as_deref(),
                                ) {
                                    *public_key = Some(dec_pubk);
                                }
                            }
                        }
                        cosmic_bwarden_core::db::EntryData::Card {
                            number,
                            cardholder_name,
                            code,
                            ..
                        } => {
                            if let Some(n) = number {
                                if let Ok(dec_n) = cosmic_bwarden_core::vault::decrypt(
                                    n.expose(),
                                    keys,
                                    entry.key.as_deref(),
                                ) {
                                    *number = Some(Secret::from(dec_n));
                                }
                            }
                            if let Some(c) = cardholder_name {
                                if let Ok(dec_c) = cosmic_bwarden_core::vault::decrypt(
                                    c,
                                    keys,
                                    entry.key.as_deref(),
                                ) {
                                    *cardholder_name = Some(dec_c);
                                }
                            }
                            if let Some(cvv) = code {
                                if let Ok(dec_cvv) = cosmic_bwarden_core::vault::decrypt(
                                    cvv.expose(),
                                    keys,
                                    entry.key.as_deref(),
                                ) {
                                    *code = Some(Secret::from(dec_cvv));
                                }
                            }
                        }
                        cosmic_bwarden_core::db::EntryData::Identity {
                            first_name,
                            last_name,
                            username,
                            email,
                            ..
                        } => {
                            if let Some(fnm) = first_name {
                                if let Ok(dec_fnm) = cosmic_bwarden_core::vault::decrypt(
                                    fnm,
                                    keys,
                                    entry.key.as_deref(),
                                ) {
                                    *first_name = Some(dec_fnm);
                                }
                            }
                            if let Some(lnm) = last_name {
                                if let Ok(dec_lnm) = cosmic_bwarden_core::vault::decrypt(
                                    lnm,
                                    keys,
                                    entry.key.as_deref(),
                                ) {
                                    *last_name = Some(dec_lnm);
                                }
                            }
                            if let Some(u) = username {
                                if let Ok(dec_u) = cosmic_bwarden_core::vault::decrypt(
                                    u,
                                    keys,
                                    entry.key.as_deref(),
                                ) {
                                    *username = Some(dec_u);
                                }
                            }
                            if let Some(e) = email {
                                if let Ok(dec_e) = cosmic_bwarden_core::vault::decrypt(
                                    e,
                                    keys,
                                    entry.key.as_deref(),
                                ) {
                                    *email = Some(dec_e);
                                }
                            }
                        }
                    }

                    for field in &mut decrypted_entry.fields {
                        if let Some(name) = &field.name {
                            if let Ok(dec_name) = cosmic_bwarden_core::vault::decrypt(
                                name,
                                keys,
                                entry.key.as_deref(),
                            ) {
                                field.name = Some(dec_name);
                            }
                        }
                        if let Some(value) = &field.value {
                            if let Ok(dec_value) = cosmic_bwarden_core::vault::decrypt(
                                value.expose(),
                                keys,
                                entry.key.as_deref(),
                            ) {
                                field.value = Some(Secret::from(dec_value));
                            }
                        }
                    }

                    Response::Entry {
                        entry: decrypted_entry,
                    }
                } else {
                    Response::Error {
                        message: "entry not found".to_string(),
                    }
                }
            } else {
                Response::Error {
                    message: "agent is locked".to_string(),
                }
            }
        }
        Action::GetPassword { id, password } => {
            match handle_request(Action::GetEntry { id, password }, state).await {
                Response::Entry { entry } => {
                    let password = match entry.data {
                        cosmic_bwarden_core::db::EntryData::Login {
                            password: Some(p), ..
                        } => p.expose().to_string(),
                        cosmic_bwarden_core::db::EntryData::SshKey {
                            private_key: Some(pk),
                            ..
                        } => pk.expose().to_string(),
                        cosmic_bwarden_core::db::EntryData::Card {
                            number: Some(n), ..
                        } => n.expose().to_string(),
                        _ => {
                            return Response::Error {
                                message: "entry has no password".to_string(),
                            }
                        }
                    };
                    Response::Password { password }
                }
                r => r,
            }
        }
        Action::DeleteEntry { id } => {
            let res = with_refresh(state, |at| {
                let id = id.clone();
                async move {
                    let config = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy()?;
                    let client = cosmic_bwarden_core::api::Client::new(
                        &config.base_url(),
                        &config.identity_url(),
                    );
                    client.delete_cipher(&at, &id).await
                }
            })
            .await;

            match res {
                Ok(_) => {
                    let mut state_guard = state.lock().await;
                    if let Some(db) = &mut state_guard.db {
                        db.entries.retain(|e| e.id != id);
                        let config = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy()
                            .unwrap();
                        let _ = db.save(&config.server_name(), config.email.as_ref().unwrap());
                        state_guard.name_cache.remove(&id);
                        state_guard.username_cache.remove(&id);
                        state_guard.broadcast(Event::VaultChanged);
                        Response::Ack
                    } else {
                        Response::Error {
                            message: "agent is locked".to_string(),
                        }
                    }
                }
                Err(e) => Response::Error {
                    message: format!("delete failed: {}", e),
                },
            }
        }
        Action::UpdateEntry { entry } => {
            let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                Ok(c) => c,
                Err(e) => {
                    return Response::Error {
                        message: format!("failed to load config: {}", e),
                    }
                }
            };
            let keys = {
                let state_guard = state.lock().await;
                state_guard.keys.clone()
            };
            if let Some(keys) = keys {
                match update_entry_on_server(state, &entry, &config, &keys).await {
                    Ok(_) => handle_request(Action::Sync, state).await,
                    Err(e) => Response::Error {
                        message: format!("update failed: {}", e),
                    },
                }
            } else {
                Response::Error {
                    message: "agent is locked".to_string(),
                }
            }
        }
        Action::PinEntry { id } => {
            let mut state_guard = state.lock().await;
            if let Some(db) = &mut state_guard.db {
                db.pinned_ids.insert(id);
                let config =
                    cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy().unwrap();
                let _ = db.save(&config.server_name(), config.email.as_ref().unwrap());
                Response::Ack
            } else {
                Response::Error {
                    message: "agent is locked".to_string(),
                }
            }
        }
        Action::UnpinEntry { id } => {
            let mut state_guard = state.lock().await;
            if let Some(db) = &mut state_guard.db {
                db.pinned_ids.remove(&id);
                let config =
                    cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy().unwrap();
                let _ = db.save(&config.server_name(), config.email.as_ref().unwrap());
                Response::Ack
            } else {
                Response::Error {
                    message: "agent is locked".to_string(),
                }
            }
        }
        Action::AddEntry {
            name,
            entry_type,
            username,
            password,
            notes,
            fields,
        } => {
            let res = with_refresh(state, |at| {
                let name = name.clone();
                let username = username.clone();
                let password = password.clone();
                let notes = notes.clone();
                let fields = fields.clone();
                async move {
                    let config = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy()?;
                    let client = cosmic_bwarden_core::api::Client::new(
                        &config.base_url(),
                        &config.identity_url(),
                    );
                    
                    let keys = {
                        // We need keys to encrypt
                        // This is a bit complex here because we need to encrypt before sending
                        // but with_refresh only gives us the token.
                        // I'll assume for now that we can't easily encrypt here without state.
                        // Actually, I'll just return error for now and say use UpdateEntry with temp ID if UI does that.
                        return Err("AddEntry not fully implemented in agent yet, use UpdateEntry".to_string());
                    }
                }
            })
            .await;
            Response::Error { message: res.err().unwrap_or_default() }
        }
        Action::GetTopFrequent { limit, .. } => {
            let state_guard = state.lock().await;
            if let Some(db) = &state_guard.db {
                let mut entries = Vec::new();
                for id in db.pinned_ids.iter().take(limit) {
                    if let Some(entry) = db.entries.iter().find(|e| &e.id == id) {
                        let name = state_guard.name_cache.get(&entry.id).cloned().unwrap_or_else(|| {
                            // This shouldn't happen if they are pinned and we synced
                             entry.name.clone()
                        });
                        entries.push(SidebarEntry {
                            id: entry.id.clone(),
                            name,
                            entry_type: match &entry.data {
                                cosmic_bwarden_core::db::EntryData::Login { .. } => EntryType::Login,
                                cosmic_bwarden_core::db::EntryData::Card { .. } => EntryType::Card,
                                cosmic_bwarden_core::db::EntryData::Identity { .. } => EntryType::Identity,
                                cosmic_bwarden_core::db::EntryData::SecureNote => EntryType::SecureNote,
                                cosmic_bwarden_core::db::EntryData::SshKey { .. } => EntryType::SshKey,
                            },
                            is_pinned: true,
                        });
                    }
                }
                Response::SidebarEntries { entries }
            } else {
                Response::Error {
                    message: "agent is locked".to_string(),
                }
            }
        }
        Action::Quit => {
            log::info!("Quit requested");
            std::process::exit(0);
        }
        _ => Response::Error {
            message: "not implemented".to_string(),
        },
    }
}
