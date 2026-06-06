mod state;
mod ssh_agent;
mod keyring;

use cosmic_bwarden_core::db::Secret;
use cosmic_bwarden_core::protocol::{Action, Response};
use state::State;
use ssh_agent::SshAgent;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::Mutex;

async fn update_entry_on_server(
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

    let name_enc = match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(crypt_keys, entry.name.as_bytes()) {
        Ok(cs) => cs.to_string(),
        Err(e) => return Err(e.to_string()),
    };

    let mut notes_enc = None;
    if let Some(n) = &entry.notes {
        notes_enc = Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(crypt_keys, n.expose().as_bytes()) {
            Ok(cs) => cs.to_string(),
            Err(e) => return Err(e.to_string()),
        });
    }

    let (ty, login_payload, ssh_key_payload) = match &entry.data {
        cosmic_bwarden_core::db::EntryData::Login { username, password, totp, .. } => {
            let mut u_enc = None;
            if let Some(u) = username {
                u_enc = Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(crypt_keys, u.as_bytes()) {
                    Ok(cs) => cs.to_string(),
                    Err(e) => return Err(e.to_string()),
                });
            }
            let mut p_enc = None;
            if let Some(p) = password {
                p_enc = Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(crypt_keys, p.expose().as_bytes()) {
                    Ok(cs) => cs.to_string(),
                    Err(e) => return Err(e.to_string()),
                });
            }
            let mut totp_enc = None;
            if let Some(t) = totp {
                totp_enc = Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(crypt_keys, t.expose().as_bytes()) {
                    Ok(cs) => cs.to_string(),
                    Err(e) => return Err(e.to_string()),
                });
            }
            (1, Some(serde_json::json!({ "username": u_enc, "password": p_enc, "totp": totp_enc })), None)
        }
        cosmic_bwarden_core::db::EntryData::SecureNote => (2, None, None),
        cosmic_bwarden_core::db::EntryData::Identity { .. } => (4, None, None),
        cosmic_bwarden_core::db::EntryData::Card { .. } => (3, None, None),
        cosmic_bwarden_core::db::EntryData::SshKey { private_key, public_key, .. } => {
            let mut priv_enc = None;
            if let Some(pk) = private_key {
                priv_enc = Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(crypt_keys, pk.expose().as_bytes()) {
                    Ok(cs) => cs.to_string(),
                    Err(e) => return Err(e.to_string()),
                });
            }
            let mut pub_enc = None;
            if let Some(pk) = public_key {
                pub_enc = Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(crypt_keys, pk.as_bytes()) {
                    Ok(cs) => cs.to_string(),
                    Err(e) => return Err(e.to_string()),
                });
            }
            (5, None, Some(serde_json::json!({ "privateKey": priv_enc, "publicKey": pub_enc })))
        }
    };

    let mut fields_enc = Vec::new();
    for field in &entry.fields {
        let name_enc = if let Some(n) = &field.name {
            Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(crypt_keys, n.as_bytes()) {
                Ok(cs) => cs.to_string(),
                Err(e) => return Err(e.to_string()),
            })
        } else {
            None
        };
        let value_enc = if let Some(v) = &field.value {
            Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(crypt_keys, v.expose().as_bytes()) {
                Ok(cs) => cs.to_string(),
                Err(e) => return Err(e.to_string()),
            })
        } else {
            None
        };
        fields_enc.push(cosmic_bwarden_core::api::CipherField {
            ty: field.ty.map(|t| match t {
                cosmic_bwarden_core::api::FieldType::Text | cosmic_bwarden_core::api::FieldType::String => cosmic_bwarden_core::api::ApiFieldType::Text,
                cosmic_bwarden_core::api::FieldType::Boolean => cosmic_bwarden_core::api::ApiFieldType::Boolean,
                cosmic_bwarden_core::api::FieldType::Hidden => cosmic_bwarden_core::api::ApiFieldType::Hidden,
                cosmic_bwarden_core::api::FieldType::Linked => cosmic_bwarden_core::api::ApiFieldType::Linked,
            }),
            name: name_enc,
            value: value_enc,
            linked_id: field.linked_id.map(|l| match l {
                cosmic_bwarden_core::api::LinkedIdType::Username => cosmic_bwarden_core::api::ApiLinkedIdType::LoginUsername,
                cosmic_bwarden_core::api::LinkedIdType::Password => cosmic_bwarden_core::api::ApiLinkedIdType::LoginPassword,
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
            client.update_cipher(
                &at,
                &entry_id,
                &name_enc,
                ty,
                login_payload,
                ssh_key_payload,
                notes_enc.as_deref(),
                Some(reprompt),
                Some(fields_enc),
            ).await
        }
    }).await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    // Prevent core dumps and ptrace attachment
    #[cfg(target_os = "linux")]
    {
        unsafe {
            libc::prctl(libc::PR_SET_DUMPABLE, 0);
        }
    }

    let socket_path = cosmic_bwarden_core::dirs::socket_file();

    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(&socket_path)?;

    // Enforce 0600 permissions on the socket
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;

    log::info!("cosmic-bwarden-agent listening on {}", socket_path.display());

    let state = Arc::new(Mutex::new(State::new()));
    let ssh_agent = SshAgent::new(Arc::clone(&state));

    let state_for_agent = Arc::clone(&state);
    let agent_handle = tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    log::error!("failed to accept connection: {}", e);
                    continue;
                }
            };

            // Verify peer UID matches our own UID
            let my_uid = rustix::process::getuid();
            match socket.peer_addr() {
                Ok(_) => {
                    match socket.peer_cred() {
                        Ok(cred) if cred.uid() == my_uid.as_raw() => {
                            // Valid connection
                        }
                        Ok(cred) => {
                            log::warn!("rejected connection from unauthorized UID: {}", cred.uid());
                            continue;
                        }
                        Err(e) => {
                            log::error!("failed to get peer credentials: {}", e);
                            continue;
                        }
                    }
                }
                Err(e) => {
                    log::error!("failed to get peer address: {}", e);
                    continue;
                }
            }

            let state = Arc::clone(&state_for_agent);

            tokio::spawn(async move {
                loop {
                    let mut buf = vec![0u8; 4096];
                    let n = match socket.read(&mut buf).await {
                        Ok(0) => return,
                        Ok(n) => n,
                        Err(e) => {
                            log::error!("failed to read from socket: {}", e);
                            return;
                        }
                    };

                    let request: Action = match serde_json::from_slice(&buf[..n]) {
                        Ok(req) => req,
                        Err(e) => {
                            let response = Response::Error {
                                message: format!("invalid request: {}", e),
                            };
                            let _ = socket.write_all(&serde_json::to_vec(&response).unwrap()).await;
                            return;
                        }
                    };

                    let is_subscribe = matches!(request, Action::Subscribe);
                    let response = handle_request(request, &state).await;
                    
                    if let Response::Error { message } = &response {
                        log::error!("request failed: {}", message);
                    }
                    let response_bytes = serde_json::to_vec(&response).unwrap();
                    if let Err(e) = socket.write_all(&response_bytes).await {
                        log::error!("failed to write to socket: {}", e);
                        return;
                    }

                    if is_subscribe {
                        // Enter event streaming loop
                        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                        {
                            let mut state_guard = state.lock().await;
                            state_guard.subscribers.push(tx);
                        }
                        
                        while let Some(event) = rx.recv().await {
                            let response = Response::Event { event };
                            let response_bytes = serde_json::to_vec(&response).unwrap();
                            if let Err(e) = socket.write_all(&response_bytes).await {
                                log::debug!("subscriber disconnected: {}", e);
                                break;
                            }
                        }
                        return;
                    } else {
                        // For non-subscription requests, close the connection after one response
                        // to match the expectation of AgentClient::send (which uses read_to_end)
                        let _ = socket.shutdown().await;
                        return;
                    }
                }
            });
        }
    });

    let ssh_agent_handle = tokio::spawn(async move {
        if let Err(e) = ssh_agent.run().await {
            log::error!("ssh-agent error: {}", e);
        }
    });

    let state_for_logind = Arc::clone(&state);
    let logind_handle = tokio::spawn(async move {
        if let Err(e) = listen_to_logind(state_for_logind).await {
            log::error!("logind listener error: {}", e);
        }
    });

    tokio::select! {
        _ = agent_handle => {},
        _ = ssh_agent_handle => {},
        _ = logind_handle => {},
    }

    Ok(())
}

async fn listen_to_logind(state: Arc<Mutex<State>>) -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;
    
    // Subscribe to Session Lock signal
    // org.freedesktop.login1.Session.Lock
    
    // Use zbus Proxy for easier signal handling if possible, or raw MatchRule
    // For simplicity, we can use a basic Stream of messages
    let mut stream = zbus::MessageStream::from(&connection);
    
    // We also want PrepareForShutdown
    // interface='org.freedesktop.login1.Manager', member='PrepareForShutdown'

    use futures_util::StreamExt;
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(m) => {
                let header = m.header();
                let interface = header.interface();
                let member = header.member();
                
                if (interface.map(|i| i.as_str()) == Some("org.freedesktop.login1.Session") && member.map(|m| m.as_str()) == Some("Lock")) ||
                   (interface.map(|i| i.as_str()) == Some("org.freedesktop.login1.Manager") && member.map(|m| m.as_str()) == Some("PrepareForShutdown")) {
                    log::info!("received lock/shutdown signal from logind, locking vault");
                    let mut state_guard = state.lock().await;
                    state_guard.lock();
                }
            }
            Err(e) => log::error!("logind message error: {}", e),
        }
    }
    
    Ok(())
}

async fn with_refresh<F, Fut, T>(
    state: &Arc<Mutex<State>>,
    f: F,
) -> Result<T, String>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = cosmic_bwarden_core::error::Result<T>>,
{
    let config = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy().map_err(|e| e.to_string())?;
    
    let (access_token, refresh_token, base_url, identity_url) = {
        let mut state_guard = state.lock().await;
        let db = state_guard.db.as_mut().ok_or("agent is locked")?;
        
        if db.access_token.is_none() && config.persist_session {
            if let Ok(Some((at, rt))) = keyring::get_tokens(&config.server_name(), config.email.as_ref().unwrap()).await {
                db.access_token = Some(Secret::from(at));
                db.refresh_token = Some(Secret::from(rt));
            }
        }

        (
            db.access_token.as_ref().ok_or("not logged in")?.expose().to_string(),
            db.refresh_token.as_ref().ok_or("no refresh token")?.expose().to_string(),
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
                                ).await;
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

async fn handle_request(action: Action, state: &Arc<Mutex<State>>) -> Response {
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
                    Response::Config { config, needs_login, is_locked }
                }
                Err(e) => Response::Error { message: format!("failed to load config: {}", e) },
            }
        }
        Action::Register { email, password, server_url } => {
            let mut config = cosmic_bwarden_core::config::CosmicBWardenConfig::default();
            config.base_url = Some(server_url);
            config.email = Some(email.clone());
            
            let client = cosmic_bwarden_core::api::Client::new(&config.base_url(), &config.identity_url());
            
            let mut pw_vec = cosmic_bwarden_core::locked::Vec::new();
            pw_vec.extend(password.as_bytes().iter().copied());
            let pw = cosmic_bwarden_core::locked::Password::new(pw_vec);

            // Vaultwarden default for new users is PBKDF2 with 600,000 iterations
            let kdf_type = cosmic_bwarden_core::api::KdfType::Pbkdf2;
            let kdf_iterations = 600_000;
            
            let identity = match cosmic_bwarden_core::identity::Identity::new(&email, &pw, kdf_type, kdf_iterations, None, None) {
                Ok(id) => id,
                Err(e) => return Response::Error { message: format!("identity derivation failed: {}", e) },
            };

            let protected_key = match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&identity.keys, identity.keys.data()) {
                Ok(cs) => cs.to_string(),
                Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
            };

            match client.register(
                &email,
                &cosmic_bwarden_core::base64::encode(identity.master_password_hash.hash()),
                &protected_key,
                kdf_type,
                kdf_iterations,
            ).await {
                Ok(_) => Response::Ack,
                Err(e) => Response::Error { message: format!("registration failed: {}", e) },
            }
        }
        Action::Login { email, password, server_url, remember_me, two_factor_token, two_factor_provider, two_factor_code, device_verification_code } => {
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
                Err(e) => return Response::Error { message: format!("failed to get device id: {}", e) },
            };

            println!("[DEBUG] Using device ID: {}", device_id);
            println!("[DEBUG] Using server URL: {}", config.base_url());

            let client = cosmic_bwarden_core::api::Client::new(&config.base_url(), &config.identity_url());
            
            let mut pw_vec = cosmic_bwarden_core::locked::Vec::new();
            pw_vec.extend(password.as_bytes().iter().copied());
            let pw = cosmic_bwarden_core::locked::Password::new(pw_vec);

            let (kdf, iterations, memory, parallelism) = match client.prelogin(&email).await {
                Ok(res) => res,
                Err(e) => return Response::Error { message: format!("prelogin failed: {}", e) },
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
                Err(e) => return Response::Error { message: format!("identity derivation failed: {}", e) },
            };

            let (access_token, refresh_token, protected_key) = match client.login(
                &email,
                &device_id,
                &identity.master_password_hash,
                two_factor_token.as_deref(),
                two_factor_provider,
                two_factor_code.as_deref(),
                device_verification_code.as_deref(),
            ).await {
                Ok(res) => res,
                Err(cosmic_bwarden_core::error::Error::TwoFactorRequired { providers, token }) => {
                    return Response::Error { message: format!("two_factor_required:{}:{}", token, serde_json::to_string(&providers).unwrap()) };
                }
                Err(cosmic_bwarden_core::error::Error::NewDeviceVerificationRequired) => {
                    return Response::Error { message: "new_device_verification_required".to_string() };
                }
                Err(e) => return Response::Error { message: format!("login failed: {}", e) },
            };

            if config.persist_session {
                if let Err(e) = keyring::store_tokens(&config.server_name(), &email, &access_token, &refresh_token).await {
                    log::error!("failed to store tokens in keyring: {}", e);
                }
            }

            let mut db = cosmic_bwarden_core::db::Db::load(&config.server_name(), &email).unwrap_or_else(|_| cosmic_bwarden_core::db::Db::new());
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
                    db.protected_org_keys = pok.into_iter().map(|(k, v)| (k, Secret::from(v))).collect();
                    db.entries = entries;
                }
                Err(e) => return Response::Error { message: format!("initial sync failed: {}", e) },
            }

            if let Err(e) = db.save(&config.server_name(), &email) {
                log::error!("failed to save db: {}", e);
                return Response::Error { message: format!("failed to save db: {}", e) };
            }
            log::debug!("DB saved successfully");
            log::debug!("Saving config with email: {:?}", config.email);
            if let Err(e) = config.save_legacy() {
                log::error!("failed to save config: {}", e);
                return Response::Error { message: format!("failed to save config: {}", e) };
            }
            log::debug!("Config saved successfully");

            // Now unlock
            log::debug!("Starting vault unlock");
            match cosmic_bwarden_core::vault::unlock(
                &email,
                &pw,
                kdf,
                iterations,
                memory,
                parallelism,
                db.protected_key.as_ref().map(|s| s.expose()).unwrap_or(""),
                db.protected_private_key.as_ref().map(|s| s.expose()),
                &db.protected_org_keys.iter().map(|(k, v)| (k.clone(), v.expose().to_string())).collect::<std::collections::HashMap<_,_>>(),
            ) {
                Ok((keys, org_keys)) => {
                    log::debug!("Vault unlock successful");
                    let mut state_guard = state.lock().await;
                    
                    // Populate pinned_ids from custom fields
                    let mut pinned_from_fields = std::collections::HashSet::new();
                    for entry in &db.entries {
                        for field in &entry.fields {
                            if let Some(name) = &field.name {
                                if let Ok(dec_name) = cosmic_bwarden_core::vault::decrypt(name, &keys, entry.key.as_deref()) {
                                    if dec_name == "corbw-pinned" {
                                        if let Some(value) = &field.value {
                                            if let Ok(dec_value) = cosmic_bwarden_core::vault::decrypt(value, &keys, entry.key.as_deref()) {
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

                    state_guard.broadcast(cosmic_bwarden_core::protocol::Event::Unlocked);
                    Response::Ack
                }
                Err(e) => Response::Error { message: format!("unlock failed after login: {}", e) },
            }
        }
        Action::Unlock { password } => {
            let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                Ok(c) => c,
                Err(e) => return Response::Error { message: format!("failed to load config: {}", e) },
            };
            let email = match config.email.as_ref() {
                Some(e) => e,
                None => return Response::Error { message: "email not set in config. Please login.".to_string() },
            };
            let mut db = match cosmic_bwarden_core::db::Db::load(&config.server_name(), email) {
                Ok(d) => d,
                Err(e) => return Response::Error { message: format!("failed to load db: {}", e) },
            };

            if db.access_token.is_none() && config.persist_session {
                match keyring::get_tokens(&config.server_name(), email).await {
                    Ok(Some((at, rt))) => {
                        db.access_token = Some(Secret::from(at));
                        db.refresh_token = Some(Secret::from(rt));
                    }
                    Ok(None) => {
                        log::debug!("no tokens found in keyring for {}", email);
                    }
                    Err(e) => {
                        log::error!("failed to get tokens from keyring: {}", e);
                    }
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
                Err(e) => return Response::Error { message: format!("identity derivation failed: {}", e) },
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
                &db.protected_org_keys.iter().map(|(k, v)| (k.clone(), v.expose().to_string())).collect::<std::collections::HashMap<_,_>>(),
            ) {
                Ok((keys, org_keys)) => {
                    let mut state_guard = state.lock().await;
                    
                    // Populate pinned_ids from custom fields
                    let mut pinned_from_fields = std::collections::HashSet::new();
                    for entry in &db.entries {
                        for field in &entry.fields {
                            if let Some(name) = &field.name {
                                if let Ok(dec_name) = cosmic_bwarden_core::vault::decrypt(name, &keys, entry.key.as_deref()) {
                                    if dec_name == "corbw-pinned" {
                                        if let Some(value) = &field.value {
                                            if let Ok(dec_value) = cosmic_bwarden_core::vault::decrypt(value, &keys, entry.key.as_deref()) {
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

                    state_guard.broadcast(cosmic_bwarden_core::protocol::Event::Unlocked);
                    Response::Ack
                }
                Err(e) => Response::Error { message: format!("unlock failed: {}", e) },
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
                Err(e) => return Response::Error { message: format!("failed to load config: {}", e) },
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
                let client = cosmic_bwarden_core::api::Client::new(&config.base_url(), &config.identity_url());
                client.sync(&at).await
            }).await;

            match res {
                Ok((protected_key, protected_private_key, protected_org_keys, entries)) => {
                    log::info!("Sync successful, got {} entries", entries.len());
                    let mut state_guard = state.lock().await;
                    let keys = state_guard.keys.clone();
                    if let (Some(db), Some(keys)) = (&mut state_guard.db, &keys) {
                        db.protected_key = Some(Secret::from(protected_key));
                        db.protected_private_key = protected_private_key.map(Secret::from);
                        db.protected_org_keys = protected_org_keys.into_iter().map(|(k, v)| (k, Secret::from(v))).collect();
                        db.entries = entries;

                        // Populate pinned_ids from corbw-pinned field
                        db.pinned_ids.clear();
                        for entry in &db.entries {
                            for field in &entry.fields {
                                if let Some(name) = &field.name {
                                    if let Ok(dec_name) = cosmic_bwarden_core::vault::decrypt(name, keys, entry.key.as_deref()) {
                                        if dec_name == "corbw-pinned" {
                                            if let Some(value) = &field.value {
                                                if let Ok(dec_value) = cosmic_bwarden_core::vault::decrypt(value, keys, entry.key.as_deref()) {
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

                        let config = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy().unwrap();
                        let _ = db.save(&config.server_name(), config.email.as_ref().unwrap());
                        state_guard.name_cache.clear();
                        state_guard.username_cache.clear();
                        state_guard.broadcast(cosmic_bwarden_core::protocol::Event::VaultChanged);
                        Response::Ack
                    } else {
                        Response::Error { message: "agent is locked".to_string() }
                    }
                }
                Err(e) => Response::Error { message: format!("sync failed: {}", e) },
            }
        }
        Action::Subscribe => {
            Response::Ack
        }
        Action::GetEntries { query, entry_type } => {
            let state = state.lock().await;
            if let (Some(db), Some(keys)) = (&state.db, &state.keys) {
                let mut entries = Vec::new();
                for entry in &db.entries {
                    // Type filter
                    if let Some(et) = entry_type {
                        match (et, &entry.data) {
                            (cosmic_bwarden_core::protocol::EntryType::Login, cosmic_bwarden_core::db::EntryData::Login { .. }) => (),
                            (cosmic_bwarden_core::protocol::EntryType::Card, cosmic_bwarden_core::db::EntryData::Card { .. }) => (),
                            (cosmic_bwarden_core::protocol::EntryType::Identity, cosmic_bwarden_core::db::EntryData::Identity { .. }) => (),
                            (cosmic_bwarden_core::protocol::EntryType::SecureNote, cosmic_bwarden_core::db::EntryData::SecureNote) => (),
                            (cosmic_bwarden_core::protocol::EntryType::SshKey, cosmic_bwarden_core::db::EntryData::SshKey { .. }) => (),
                            _ => continue,
                        }
                    }

                    let mut decrypted_entry = entry.clone();
                    if let Ok(decrypted_name) = cosmic_bwarden_core::vault::decrypt(&entry.name, keys, entry.key.as_deref()) {
                        decrypted_entry.name = decrypted_name;
                    }
                    // Also decrypt username for display if it's a login
                    if let cosmic_bwarden_core::db::EntryData::Login { username, .. } = &mut decrypted_entry.data {
                        if let Some(u) = username {
                            if let Ok(dec_u) = cosmic_bwarden_core::vault::decrypt(u, keys, entry.key.as_deref()) {
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
                    // Type filter
                    if let Some(et) = entry_type {
                        match (et, &entry.data) {
                            (cosmic_bwarden_core::protocol::EntryType::Login, cosmic_bwarden_core::db::EntryData::Login { .. }) => (),
                            (cosmic_bwarden_core::protocol::EntryType::Card, cosmic_bwarden_core::db::EntryData::Card { .. }) => (),
                            (cosmic_bwarden_core::protocol::EntryType::Identity, cosmic_bwarden_core::db::EntryData::Identity { .. }) => (),
                            (cosmic_bwarden_core::protocol::EntryType::SecureNote, cosmic_bwarden_core::db::EntryData::SecureNote) => (),
                            (cosmic_bwarden_core::protocol::EntryType::SshKey, cosmic_bwarden_core::db::EntryData::SshKey { .. }) => (),
                            _ => continue,
                        }
                    }

                    let decrypted_name = if let Some(name) = state_guard.name_cache.get(&entry.id) {
                        name.clone()
                    } else {
                        let name = match cosmic_bwarden_core::vault::decrypt(&entry.name, keys, entry.key.as_deref()) {
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
                        if let cosmic_bwarden_core::db::EntryData::Login { username, .. } = &entry.data {
                            if let Some(u) = username {
                                u_dec = Some(match cosmic_bwarden_core::vault::decrypt(u, keys, entry.key.as_deref()) {
                                    Ok(dec_u) => dec_u,
                                    Err(_) => u.clone(),
                                });
                            }
                        }
                        if let Some(u) = &u_dec {
                            new_usernames.push((entry.id.clone(), u.clone()));
                        }
                        u_dec
                    };

                    entries.push((cosmic_bwarden_core::protocol::SidebarEntry {
                        id: entry.id.clone(),
                        name: decrypted_name,
                        entry_type: match &entry.data {
                            cosmic_bwarden_core::db::EntryData::Login { .. } => cosmic_bwarden_core::protocol::EntryType::Login,
                            cosmic_bwarden_core::db::EntryData::Card { .. } => cosmic_bwarden_core::protocol::EntryType::Card,
                            cosmic_bwarden_core::db::EntryData::Identity { .. } => cosmic_bwarden_core::protocol::EntryType::Identity,
                            cosmic_bwarden_core::db::EntryData::SecureNote => cosmic_bwarden_core::protocol::EntryType::SecureNote,
                            cosmic_bwarden_core::db::EntryData::SshKey { .. } => cosmic_bwarden_core::protocol::EntryType::SshKey,
                        },
                        is_pinned: db.pinned_ids.contains(&entry.id),
                    }, username_dec));
                }

                // Update cache
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
                            e.name.to_lowercase().contains(&q) || 
                            e.id == q ||
                            u.as_ref().map(|un| un.to_lowercase().contains(&q)).unwrap_or(false)
                        })
                        .map(|(e, _)| e)
                        .collect()
                } else {
                    entries.into_iter().map(|(e, _)| e).collect()
                };
                Response::SidebarEntries { entries }
            } else {
                Response::Error { message: "agent is locked".to_string() }
            }
        }
        Action::GetEntry { id, password } => {
            let state = state.lock().await;
            if let (Some(db), Some(keys)) = (&state.db, &state.keys) {
                if let Some(entry) = db.entries.iter().find(|e| e.id == id) {
                    if entry.master_password_reprompt() {
                        let password = match password {
                            Some(p) => p,
                            None => return Response::Error { message: "reprompt_required".to_string() },
                        };
                        
                        let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                            Ok(c) => c,
                            Err(e) => return Response::Error { message: format!("failed to load config: {}", e) },
                        };
                        let email = match config.email.as_ref() {
                            Some(e) => e,
                            None => return Response::Error { message: "email not set in config".to_string() },
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
                            Err(e) => return Response::Error { message: format!("identity derivation failed: {}", e) },
                        };

                        if let Some(stored_hash) = &state.master_password_hash {
                            if identity.master_password_hash.hash() != stored_hash.hash() {
                                return Response::Error { message: "incorrect password".to_string() };
                            }
                        } else {
                            return Response::Error { message: "agent state inconsistent".to_string() };
                        }
                    }

                    let mut decrypted_entry = entry.clone();
                    if let Ok(decrypted_name) = cosmic_bwarden_core::vault::decrypt(&entry.name, keys, entry.key.as_deref()) {
                        decrypted_entry.name = decrypted_name;
                    }
                    if let Some(notes) = &entry.notes {
                        if let Ok(dec_notes) = cosmic_bwarden_core::vault::decrypt(notes.expose(), keys, entry.key.as_deref()) {
                            decrypted_entry.notes = Some(Secret::from(dec_notes));
                        }
                    }

                    match &mut decrypted_entry.data {
                        cosmic_bwarden_core::db::EntryData::Login { username, password, totp, .. } => {
                            if let Some(u) = username {
                                if let Ok(dec_u) = cosmic_bwarden_core::vault::decrypt(u, keys, entry.key.as_deref()) {
                                    *username = Some(dec_u);
                                }
                            }
                            if let Some(p) = password {
                                if let Ok(dec_p) = cosmic_bwarden_core::vault::decrypt(p.expose(), keys, entry.key.as_deref()) {
                                    *password = Some(Secret::from(dec_p));
                                }
                            }
                            if let Some(t) = totp {
                                if let Ok(dec_t) = cosmic_bwarden_core::vault::decrypt(t.expose(), keys, entry.key.as_deref()) {
                                    *totp = Some(Secret::from(dec_t));
                                }
                            }
                        }
                        cosmic_bwarden_core::db::EntryData::SecureNote => {}
                        cosmic_bwarden_core::db::EntryData::SshKey { private_key, public_key, .. } => {
                            if let Some(pk) = private_key {
                                if let Ok(dec_pk) = cosmic_bwarden_core::vault::decrypt(pk.expose(), keys, entry.key.as_deref()) {
                                    *private_key = Some(Secret::from(dec_pk));
                                }
                            }
                            if let Some(pubk) = public_key {
                                if let Ok(dec_pubk) = cosmic_bwarden_core::vault::decrypt(pubk, keys, entry.key.as_deref()) {
                                    *public_key = Some(dec_pubk);
                                }
                            }
                        }
                        cosmic_bwarden_core::db::EntryData::Card { number, cardholder_name, code, .. } => {
                            if let Some(n) = number {
                                if let Ok(dec_n) = cosmic_bwarden_core::vault::decrypt(n.expose(), keys, entry.key.as_deref()) {
                                    *number = Some(Secret::from(dec_n));
                                }
                            }
                            if let Some(c) = cardholder_name {
                                if let Ok(dec_c) = cosmic_bwarden_core::vault::decrypt(c, keys, entry.key.as_deref()) {
                                    *cardholder_name = Some(dec_c);
                                }
                            }
                            if let Some(cvv) = code {
                                if let Ok(dec_cvv) = cosmic_bwarden_core::vault::decrypt(cvv.expose(), keys, entry.key.as_deref()) {
                                    *code = Some(Secret::from(dec_cvv));
                                }
                            }
                        }
                        cosmic_bwarden_core::db::EntryData::Identity { first_name, last_name, username, email, .. } => {
                            if let Some(fnm) = first_name {
                                if let Ok(dec_fnm) = cosmic_bwarden_core::vault::decrypt(fnm, keys, entry.key.as_deref()) {
                                    *first_name = Some(dec_fnm);
                                }
                            }
                            if let Some(lnm) = last_name {
                                if let Ok(dec_lnm) = cosmic_bwarden_core::vault::decrypt(lnm, keys, entry.key.as_deref()) {
                                    *last_name = Some(dec_lnm);
                                }
                            }
                            if let Some(u) = username {
                                if let Ok(dec_u) = cosmic_bwarden_core::vault::decrypt(u, keys, entry.key.as_deref()) {
                                    *username = Some(dec_u);
                                }
                            }
                            if let Some(e) = email {
                                if let Ok(dec_e) = cosmic_bwarden_core::vault::decrypt(e, keys, entry.key.as_deref()) {
                                    *email = Some(dec_e);
                                }
                            }
                        }
                    }

                    for field in &mut decrypted_entry.fields {
                        if let Some(name) = &field.name {
                            if let Ok(dec_name) = cosmic_bwarden_core::vault::decrypt(name, keys, entry.key.as_deref()) {
                                field.name = Some(dec_name);
                            }
                        }
                        if let Some(value) = &field.value {
                            if let Ok(dec_value) = cosmic_bwarden_core::vault::decrypt(value.expose(), keys, entry.key.as_deref()) {
                                field.value = Some(Secret::from(dec_value));
                            }
                        }
                    }

                    Response::Entry { entry: decrypted_entry }
                } else {
                    Response::Error { message: "entry not found".to_string() }
                }
            } else {
                Response::Error { message: "agent is locked".to_string() }
            }
        }
        Action::GetPassword { id, password } => {
            let state = state.lock().await;
            if let (Some(db), Some(keys)) = (&state.db, &state.keys) {
                if let Some(entry) = db.entries.iter().find(|e| e.id == id) {
                    if entry.master_password_reprompt() {
                        let password = match password {
                            Some(p) => p,
                            None => return Response::Error { message: "reprompt_required".to_string() },
                        };
                        
                        let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                            Ok(c) => c,
                            Err(e) => return Response::Error { message: format!("failed to load config: {}", e) },
                        };
                        let email = match config.email.as_ref() {
                            Some(e) => e,
                            None => return Response::Error { message: "email not set in config".to_string() },
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
                            Err(e) => return Response::Error { message: format!("identity derivation failed: {}", e) },
                        };

                        if let Some(stored_hash) = &state.master_password_hash {
                            if identity.master_password_hash.hash() != stored_hash.hash() {
                                return Response::Error { message: "incorrect password".to_string() };
                            }
                        } else {
                            return Response::Error { message: "agent state inconsistent".to_string() };
                        }
                    }

                    let cipherstring = match &entry.data {
                        cosmic_bwarden_core::db::EntryData::Login { password, .. } => password.as_ref().map(|s| s.expose()),
                        _ => None,
                    };

                    if let Some(cs) = cipherstring {
                        match cosmic_bwarden_core::vault::decrypt(cs, keys, entry.key.as_deref()) {
                            Ok(password) => {
                                Response::Password { password }
                            }
                            Err(e) => Response::Error {
                                message: format!("decryption failed: {}", e),
                            },
                        }
                    } else {
                        Response::Error {
                            message: "entry has no password".to_string(),
                        }
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
        Action::CopyToClipboard { id } => {
            let mut s = state.lock().await;
            if let (Some(db), Some(keys)) = (&s.db, &s.keys) {
                if let Some(entry) = db.entries.iter().find(|e| e.id == id) {
                    let cipherstring = match &entry.data {
                        cosmic_bwarden_core::db::EntryData::Login { password, .. } => password.as_ref().map(|s| s.expose()),
                        _ => None,
                    };

                    if let Some(cs) = cipherstring {
                        match cosmic_bwarden_core::vault::decrypt(cs, keys, entry.key.as_deref()) {
                            Ok(password) => {
                                if let Some(cb) = &mut s.clipboard {
                                    if let Err(e) = cb.set_text(password) {
                                        return Response::Error { message: format!("failed to set clipboard: {}", e) };
                                    }
                                    s.clipboard_gen += 1;
                                    let gen = s.clipboard_gen;
                                    let state_clone = Arc::clone(state);
                                    tokio::spawn(async move {
                                        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                                        let mut s = state_clone.lock().await;
                                        if s.clipboard_gen == gen {
                                            if let Some(cb) = &mut s.clipboard {
                                                let _ = cb.set_text("");
                                            }
                                        }
                                    });
                                    s.record_copy(&id);
                                    if let (Some(db), Some(config)) = (&s.db, cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy().ok()) {
                                        if let Some(email) = &config.email {
                                            let _ = db.save(&config.server_name(), email);
                                        }
                                    }
                                    Response::Ack
                                } else {
                                    Response::Error { message: "clipboard not available".to_string() }
                                }
                            }
                            Err(e) => Response::Error { message: format!("decryption failed: {}", e) },
                        }
                    } else {
                        Response::Error { message: "entry has no password".to_string() }
                    }
                } else {
                    Response::Error { message: "entry not found".to_string() }
                }
            } else {
                Response::Error { message: "agent is locked".to_string() }
            }
        }
        Action::RecordCopy { id } => {
            let mut state_guard = state.lock().await;
            state_guard.record_copy(&id);
            if let (Some(db), Some(config)) = (&state_guard.db, cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy().ok()) {
                if let Some(email) = &config.email {
                    let _ = db.save(&config.server_name(), email);
                }
            }
            Response::Ack
        }
        Action::PinEntry { id } => {
            let (config, keys, entry) = {
                let state_guard = state.lock().await;
                let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                    Ok(c) => c,
                    Err(e) => return Response::Error { message: format!("failed to load config: {}", e) },
                };
                let keys = match &state_guard.keys {
                    Some(k) => k.clone(),
                    None => return Response::Error { message: "agent is locked".to_string() },
                };
                let db = match &state_guard.db {
                    Some(db) => db,
                    None => return Response::Error { message: "no database loaded".to_string() },
                };
                let entry = match db.entries.iter().find(|e| e.id == id) {
                    Some(e) => e.clone(),
                    None => return Response::Error { message: "entry not found".to_string() },
                };
                (config, keys, entry)
            };

            let mut updated_entry = entry.decrypt(&keys);
            updated_entry.set_field("corbw-pinned", "true", cosmic_bwarden_core::api::FieldType::Boolean);

            // Re-use UpdateEntry logic but as a separate block or call
            let res = update_entry_on_server(state, &updated_entry, &config, &keys).await;

            match res {
                Ok(_) => {
                    let mut state_guard = state.lock().await;
                    if let Some(db) = &mut state_guard.db {
                        // We must keep the ENCRYPTED version in db.entries
                        // but set_field on the encrypted entry is hard because set_field expects plaintext
                        // So we just rely on the next Sync to get the correct encrypted version from server
                        // OR we encrypt the field here.
                        // For now, let's just trigger a sync or update local state-only pinned_ids
                        db.pinned_ids.insert(id.clone());
                        let _ = db.save(&config.server_name(), config.email.as_ref().unwrap());
                    }
                    Response::Ack
                }
                Err(e) => Response::Error { message: format!("pin entry failed: {}", e) },
            }
        }
        Action::UnpinEntry { id } => {
            let (config, keys, entry) = {
                let state_guard = state.lock().await;
                let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                    Ok(c) => c,
                    Err(e) => return Response::Error { message: format!("failed to load config: {}", e) },
                };
                let keys = match &state_guard.keys {
                    Some(k) => k.clone(),
                    None => return Response::Error { message: "agent is locked".to_string() },
                };
                let db = match &state_guard.db {
                    Some(db) => db,
                    None => return Response::Error { message: "no database loaded".to_string() },
                };
                let entry = match db.entries.iter().find(|e| e.id == id) {
                    Some(e) => e.clone(),
                    None => return Response::Error { message: "entry not found".to_string() },
                };
                (config, keys, entry)
            };

            let mut updated_entry = entry.decrypt(&keys);
            updated_entry.remove_field("corbw-pinned");

            let res = update_entry_on_server(state, &updated_entry, &config, &keys).await;

            match res {
                Ok(_) => {
                    let mut state_guard = state.lock().await;
                    if let Some(db) = &mut state_guard.db {
                        db.pinned_ids.remove(&id);
                        db.usage_counts.remove(&id);
                        let _ = db.save(&config.server_name(), config.email.as_ref().unwrap());
                    }
                    Response::Ack
                }
                Err(e) => Response::Error { message: format!("unpin entry failed: {}", e) },
            }
        }
        Action::GetTopFrequent { limit, .. } => {
            let mut state_guard = state.lock().await;
            if let (Some(db), Some(keys)) = (&state_guard.db, &state_guard.keys) {
                let top_ids = state_guard.top_pinned(limit);
                let mut new_names = Vec::new();
                let mut entries = Vec::new();

                for id in top_ids {
                    if let Some(entry) = db.entries.iter().find(|e| e.id == id) {
                        let decrypted_name = if let Some(name) = state_guard.name_cache.get(&entry.id) {
                            name.clone()
                        } else {
                            let name = match cosmic_bwarden_core::vault::decrypt(&entry.name, keys, entry.key.as_deref()) {
                                Ok(n) => n,
                                Err(_) => entry.name.clone(),
                            };
                            new_names.push((entry.id.clone(), name.clone()));
                            name
                        };
                        entries.push(cosmic_bwarden_core::protocol::SidebarEntry {
                            id: entry.id.clone(),
                            name: decrypted_name,
                            entry_type: match &entry.data {
                                cosmic_bwarden_core::db::EntryData::Login { .. } => cosmic_bwarden_core::protocol::EntryType::Login,
                                cosmic_bwarden_core::db::EntryData::Card { .. } => cosmic_bwarden_core::protocol::EntryType::Card,
                                cosmic_bwarden_core::db::EntryData::Identity { .. } => cosmic_bwarden_core::protocol::EntryType::Identity,
                                cosmic_bwarden_core::db::EntryData::SecureNote => cosmic_bwarden_core::protocol::EntryType::SecureNote,
                                cosmic_bwarden_core::db::EntryData::SshKey { .. } => cosmic_bwarden_core::protocol::EntryType::SshKey,
                            },
                            is_pinned: true,
                        });
                    }
                }

                // Update cache
                for (id, name) in new_names {
                    state_guard.name_cache.insert(id, name);
                }

                Response::SidebarEntries { entries }
            } else {
                Response::Error { message: "agent is locked".to_string() }
            }
        }
        Action::AddEntry { name, entry_type, username, password, notes, fields } => {
            let (config, keys) = {
                let state = state.lock().await;
                let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                    Ok(c) => c,
                    Err(e) => return Response::Error { message: format!("failed to load config: {}", e) },
                };
                let keys = match &state.keys {
                    Some(k) => k.clone(),
                    None => return Response::Error { message: "agent is locked".to_string() },
                };
                (config, keys)
            };

            let ty = match entry_type {
                cosmic_bwarden_core::protocol::EntryType::Login => 1,
                cosmic_bwarden_core::protocol::EntryType::Card => 3,
                cosmic_bwarden_core::protocol::EntryType::Identity => 4,
                cosmic_bwarden_core::protocol::EntryType::SecureNote => 2,
                cosmic_bwarden_core::protocol::EntryType::SshKey => 5,
            };

            let name_enc = match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, name.as_bytes()) {
                Ok(cs) => cs.to_string(),
                Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
            };
            let username_enc = match username {
                Some(u) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, u.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let password_enc = match password {
                Some(p) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, p.expose().as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let notes_enc = match notes {
                Some(n) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, n.expose().as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };

            let mut fields_enc = Vec::new();
            for field in fields {
                let f_name_enc = if let Some(n) = field.name {
                    Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, n.as_bytes()) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                    })
                } else {
                    None
                };
                let f_value_enc = if let Some(v) = field.value {
                    Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, v.expose().as_bytes()) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                    })
                } else {
                    None
                };

                fields_enc.push(cosmic_bwarden_core::api::CipherField {
                    ty: field.ty.map(|t| match t {
                        cosmic_bwarden_core::api::FieldType::Text | cosmic_bwarden_core::api::FieldType::String => cosmic_bwarden_core::api::ApiFieldType::Text,
                        cosmic_bwarden_core::api::FieldType::Boolean => cosmic_bwarden_core::api::ApiFieldType::Boolean,
                        cosmic_bwarden_core::api::FieldType::Hidden => cosmic_bwarden_core::api::ApiFieldType::Hidden,
                        cosmic_bwarden_core::api::FieldType::Linked => cosmic_bwarden_core::api::ApiFieldType::Linked,
                    }),
                    name: f_name_enc,
                    value: f_value_enc,
                    linked_id: field.linked_id.map(|l| match l {
                        cosmic_bwarden_core::api::LinkedIdType::Username => cosmic_bwarden_core::api::ApiLinkedIdType::LoginUsername,
                        cosmic_bwarden_core::api::LinkedIdType::Password => cosmic_bwarden_core::api::ApiLinkedIdType::LoginPassword,
                        _ => cosmic_bwarden_core::api::ApiLinkedIdType::LoginUsername,
                    }),
                });
            }

            let res = with_refresh(state, |at| {
                let name_enc = name_enc.clone();
                let username_enc = username_enc.clone();
                let password_enc = password_enc.clone();
                let notes_enc = notes_enc.clone();
                let fields_enc = fields_enc.clone();
                let base_url = config.base_url();
                let identity_url = config.identity_url();
                async move {
                    let client = cosmic_bwarden_core::api::Client::new(&base_url, &identity_url);
                    client.add_cipher(
                        &at,
                        ty,
                        &name_enc,
                        username_enc.as_deref(),
                        password_enc.as_deref(),
                        notes_enc.as_deref(),
                        Some(fields_enc),
                    ).await
                }
            }).await;

            match res {
                Ok(_) => Response::Ack,
                Err(e) => Response::Error { message: format!("add entry failed: {}", e) },
            }
        }
        Action::AddSecureNote { name, notes, fields } => {
            let (config, keys) = {
                let state = state.lock().await;
                let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                    Ok(c) => c,
                    Err(e) => return Response::Error { message: format!("failed to load config: {}", e) },
                };
                let keys = match &state.keys {
                    Some(k) => k.clone(),
                    None => return Response::Error { message: "agent is locked".to_string() },
                };
                (config, keys)
            };

            let name_enc = match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, name.as_bytes()) {
                Ok(cs) => cs.to_string(),
                Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
            };
            let notes_enc = match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, notes.expose().as_bytes()) {
                Ok(cs) => cs.to_string(),
                Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
            };


            let mut fields_enc = Vec::new();
            for field in fields {
                let f_name_enc = if let Some(n) = field.name {
                    Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, n.as_bytes()) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                    })
                } else {
                    None
                };
                let f_value_enc = if let Some(v) = field.value {
                    Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, v.expose().as_bytes()) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                    })
                } else {
                    None
                };

                fields_enc.push(cosmic_bwarden_core::api::CipherField {
                    ty: field.ty.map(|t| match t {
                        cosmic_bwarden_core::api::FieldType::Text | cosmic_bwarden_core::api::FieldType::String => cosmic_bwarden_core::api::ApiFieldType::Text,
                        cosmic_bwarden_core::api::FieldType::Boolean => cosmic_bwarden_core::api::ApiFieldType::Boolean,
                        cosmic_bwarden_core::api::FieldType::Hidden => cosmic_bwarden_core::api::ApiFieldType::Hidden,
                        cosmic_bwarden_core::api::FieldType::Linked => cosmic_bwarden_core::api::ApiFieldType::Linked,
                    }),
                    name: f_name_enc,
                    value: f_value_enc,
                    linked_id: field.linked_id.map(|l| match l {
                        cosmic_bwarden_core::api::LinkedIdType::Username => cosmic_bwarden_core::api::ApiLinkedIdType::LoginUsername,
                        cosmic_bwarden_core::api::LinkedIdType::Password => cosmic_bwarden_core::api::ApiLinkedIdType::LoginPassword,
                        _ => cosmic_bwarden_core::api::ApiLinkedIdType::LoginUsername,
                    }),
                });
            }

            let res = with_refresh(state, |at| {
                let name_enc = name_enc.clone();
                let notes_enc = notes_enc.clone();
                let fields_enc = fields_enc.clone();
                let base_url = config.base_url();
                let identity_url = config.identity_url();
                async move {
                    let client = cosmic_bwarden_core::api::Client::new(&base_url, &identity_url);
                    client.add_cipher(&at, 2, &name_enc, None, None, Some(&notes_enc), Some(fields_enc)).await
                }
            }).await;


            match res {
                Ok(_) => Response::Ack,
                Err(e) => Response::Error { message: format!("add secure note failed: {}", e) },
            }
        }
        Action::AddCard { name, cardholder_name, number, brand, exp_month, exp_year, code, notes, fields } => {
            let (config, keys) = {
                let state = state.lock().await;
                let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                    Ok(c) => c,
                    Err(e) => return Response::Error { message: format!("failed to load config: {}", e) },
                };
                let keys = match &state.keys {
                    Some(k) => k.clone(),
                    None => return Response::Error { message: "agent is locked".to_string() },
                };
                (config, keys)
            };

            let name_enc = match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, name.as_bytes()) {
                Ok(cs) => cs.to_string(),
                Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
            };
            let cardholder_name_enc = match cardholder_name {
                Some(n) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, n.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let number_enc = match number {
                Some(n) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, n.expose().as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let brand_enc = match brand {
                Some(b) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, b.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let exp_month_enc = match exp_month {
                Some(m) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, m.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let exp_year_enc = match exp_year {
                Some(y) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, y.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let code_enc = match code {
                Some(c) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, c.expose().as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let notes_enc = match notes {
                Some(n) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, n.expose().as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };

            let mut fields_enc = Vec::new();
            for field in fields {
                let f_name_enc = if let Some(n) = field.name {
                    Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, n.as_bytes()) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                    })
                } else {
                    None
                };
                let f_value_enc = if let Some(v) = field.value {
                    Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, v.expose().as_bytes()) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                    })
                } else {
                    None
                };

                fields_enc.push(cosmic_bwarden_core::api::CipherField {
                    ty: field.ty.map(|t| match t {
                        cosmic_bwarden_core::api::FieldType::Text | cosmic_bwarden_core::api::FieldType::String => cosmic_bwarden_core::api::ApiFieldType::Text,
                        cosmic_bwarden_core::api::FieldType::Boolean => cosmic_bwarden_core::api::ApiFieldType::Boolean,
                        cosmic_bwarden_core::api::FieldType::Hidden => cosmic_bwarden_core::api::ApiFieldType::Hidden,
                        cosmic_bwarden_core::api::FieldType::Linked => cosmic_bwarden_core::api::ApiFieldType::Linked,
                    }),
                    name: f_name_enc,
                    value: f_value_enc,
                    linked_id: field.linked_id.map(|l| match l {
                        cosmic_bwarden_core::api::LinkedIdType::Username => cosmic_bwarden_core::api::ApiLinkedIdType::LoginUsername,
                        cosmic_bwarden_core::api::LinkedIdType::Password => cosmic_bwarden_core::api::ApiLinkedIdType::LoginPassword,
                        _ => cosmic_bwarden_core::api::ApiLinkedIdType::LoginUsername,
                    }),
                });
            }

            let res = with_refresh(state, |at| {
                let name_enc = name_enc.clone();
                let cardholder_name_enc = cardholder_name_enc.clone();
                let number_enc = number_enc.clone();
                let brand_enc = brand_enc.clone();
                let exp_month_enc = exp_month_enc.clone();
                let exp_year_enc = exp_year_enc.clone();
                let code_enc = code_enc.clone();
                let notes_enc = notes_enc.clone();
                let fields_enc = fields_enc.clone();
                let base_url = config.base_url();
                let identity_url = config.identity_url();
                async move {
                    let client = cosmic_bwarden_core::api::Client::new(&base_url, &identity_url);
                    client.add_card(
                        &at,
                        &name_enc,
                        cardholder_name_enc.as_deref(),
                        brand_enc.as_deref(),
                        number_enc.as_deref(),
                        exp_month_enc.as_deref(),
                        exp_year_enc.as_deref(),
                        code_enc.as_deref(),
                        notes_enc.as_deref(),
                        Some(fields_enc)
                    ).await
                }
            }).await;

            match res {
                Ok(_) => Response::Ack,
                Err(e) => Response::Error { message: format!("add card failed: {}", e) },
            }
            }
            Action::AddIdentity { name, first_name, last_name, address1, city, state: region, postal_code, country, email, phone, notes, fields } => {
            let (config, keys) = {
                let state = state.lock().await;
                let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                    Ok(c) => c,
                    Err(e) => return Response::Error { message: format!("failed to load config: {}", e) },
                };
                let keys = match &state.keys {
                    Some(k) => k.clone(),
                    None => return Response::Error { message: "agent is locked".to_string() },
                };
                (config, keys)
            };

            let name_enc = match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, name.as_bytes()) {
                Ok(cs) => cs.to_string(),
                Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
            };
            let first_name_enc = match first_name {
                Some(n) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, n.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let last_name_enc = match last_name {
                Some(n) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, n.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let address1_enc = match address1 {
                Some(a) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, a.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let city_enc = match city {
                Some(c) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, c.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let region_enc = match region {
                Some(r) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, r.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let postal_code_enc = match postal_code {
                Some(p) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, p.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let country_enc = match country {
                Some(c) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, c.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let email_enc = match email {
                Some(e) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, e.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let phone_enc = match phone {
                Some(p) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, p.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let notes_enc = match notes {
                Some(n) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, n.expose().as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };

            let mut fields_enc = Vec::new();
            for field in fields {
                let f_name_enc = if let Some(n) = field.name {
                    Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, n.as_bytes()) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                    })
                } else {
                    None
                };
                let f_value_enc = if let Some(v) = field.value {
                    Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, v.expose().as_bytes()) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                    })
                } else {
                    None
                };

                fields_enc.push(cosmic_bwarden_core::api::CipherField {
                    ty: field.ty.map(|t| match t {
                        cosmic_bwarden_core::api::FieldType::Text | cosmic_bwarden_core::api::FieldType::String => cosmic_bwarden_core::api::ApiFieldType::Text,
                        cosmic_bwarden_core::api::FieldType::Boolean => cosmic_bwarden_core::api::ApiFieldType::Boolean,
                        cosmic_bwarden_core::api::FieldType::Hidden => cosmic_bwarden_core::api::ApiFieldType::Hidden,
                        cosmic_bwarden_core::api::FieldType::Linked => cosmic_bwarden_core::api::ApiFieldType::Linked,
                    }),
                    name: f_name_enc,
                    value: f_value_enc,
                    linked_id: field.linked_id.map(|l| match l {
                        cosmic_bwarden_core::api::LinkedIdType::Username => cosmic_bwarden_core::api::ApiLinkedIdType::LoginUsername,
                        cosmic_bwarden_core::api::LinkedIdType::Password => cosmic_bwarden_core::api::ApiLinkedIdType::LoginPassword,
                        _ => cosmic_bwarden_core::api::ApiLinkedIdType::LoginUsername,
                    }),
                });
            }

            let res = with_refresh(state, |at| {
                let name_enc = name_enc.clone();
                let first_name_enc = first_name_enc.clone();
                let last_name_enc = last_name_enc.clone();
                let address1_enc = address1_enc.clone();
                let city_enc = city_enc.clone();
                let region_enc = region_enc.clone();
                let postal_code_enc = postal_code_enc.clone();
                let country_enc = country_enc.clone();
                let email_enc = email_enc.clone();
                let phone_enc = phone_enc.clone();
                let notes_enc = notes_enc.clone();
                let fields_enc = fields_enc.clone();
                let base_url = config.base_url();
                let identity_url = config.identity_url();
                async move {
                    let client = cosmic_bwarden_core::api::Client::new(&base_url, &identity_url);
                    client.add_identity(
                        &at,
                        &name_enc,
                        first_name_enc.as_deref(),
                        last_name_enc.as_deref(),
                        address1_enc.as_deref(),
                        city_enc.as_deref(),
                        region_enc.as_deref(),
                        postal_code_enc.as_deref(),
                        country_enc.as_deref(),
                        email_enc.as_deref(),
                        phone_enc.as_deref(),
                        notes_enc.as_deref(),
                        Some(fields_enc)
                    ).await
                }
            }).await;

            match res {
                Ok(_) => Response::Ack,
                Err(e) => Response::Error { message: format!("add identity failed: {}", e) },
            }
        }
        Action::AddSshKey { name, private_key, public_key, notes, fields } => {
            let (config, keys) = {
                let state = state.lock().await;
                let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                    Ok(c) => c,
                    Err(e) => return Response::Error { message: format!("failed to load config: {}", e) },
                };
                let keys = match &state.keys {
                    Some(k) => k.clone(),
                    None => return Response::Error { message: "agent is locked".to_string() },
                };
                (config, keys)
            };

            let name_enc = match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, name.as_bytes()) {
                Ok(cs) => cs.to_string(),
                Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
            };
            let private_key_enc = match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, private_key.expose().as_bytes()) {
                Ok(cs) => cs.to_string(),
                Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
            };
            let public_key_enc = match public_key {
                Some(pk) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, pk.as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };
            let notes_enc = match notes {
                Some(n) => match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, n.expose().as_bytes()) {
                    Ok(cs) => Some(cs.to_string()),
                    Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                },
                None => None,
            };

            let mut fields_enc = Vec::new();
            for field in fields {
                let f_name_enc = if let Some(n) = field.name {
                    Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, n.as_bytes()) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                    })
                } else {
                    None
                };
                let f_value_enc = if let Some(v) = field.value {
                    Some(match cosmic_bwarden_core::cipherstring::CipherString::encrypt_symmetric(&keys, v.expose().as_bytes()) {
                        Ok(cs) => cs.to_string(),
                        Err(e) => return Response::Error { message: format!("encryption failed: {}", e) },
                    })
                } else {
                    None
                };

                fields_enc.push(cosmic_bwarden_core::api::CipherField {
                    ty: field.ty.map(|t| match t {
                        cosmic_bwarden_core::api::FieldType::Text | cosmic_bwarden_core::api::FieldType::String => cosmic_bwarden_core::api::ApiFieldType::Text,
                        cosmic_bwarden_core::api::FieldType::Boolean => cosmic_bwarden_core::api::ApiFieldType::Boolean,
                        cosmic_bwarden_core::api::FieldType::Hidden => cosmic_bwarden_core::api::ApiFieldType::Hidden,
                        cosmic_bwarden_core::api::FieldType::Linked => cosmic_bwarden_core::api::ApiFieldType::Linked,
                    }),
                    name: f_name_enc,
                    value: f_value_enc,
                    linked_id: field.linked_id.map(|l| match l {
                        cosmic_bwarden_core::api::LinkedIdType::Username => cosmic_bwarden_core::api::ApiLinkedIdType::LoginUsername,
                        cosmic_bwarden_core::api::LinkedIdType::Password => cosmic_bwarden_core::api::ApiLinkedIdType::LoginPassword,
                        _ => cosmic_bwarden_core::api::ApiLinkedIdType::LoginUsername,
                    }),
                });
            }

            let res = with_refresh(state, |at| {
                let name_enc = name_enc.clone();
                let private_key_enc = private_key_enc.clone();
                let public_key_enc = public_key_enc.clone();
                let notes_enc = notes_enc.clone();
                let fields_enc = fields_enc.clone();
                let base_url = config.base_url();
                let identity_url = config.identity_url();
                async move {
                    let client = cosmic_bwarden_core::api::Client::new(&base_url, &identity_url);
                    client.add_ssh_key(&at, &name_enc, &private_key_enc, public_key_enc.as_deref(), notes_enc.as_deref(), Some(fields_enc)).await
                }
            }).await;

            match res {
                Ok(_) => Response::Ack,
                Err(e) => Response::Error { message: format!("add ssh key failed: {}", e) },
            }
        }
        Action::Quit => {
            std::process::exit(0);
        }
        Action::DeleteEntry { id } => {
            let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                Ok(c) => c,
                Err(e) => return Response::Error { message: format!("failed to load config: {}", e) },
            };

            let entry_id = id.clone();
            let res = with_refresh(state, |at| {
                let entry_id = entry_id.clone();
                let base_url = config.base_url();
                let identity_url = config.identity_url();
                async move {
                    let client = cosmic_bwarden_core::api::Client::new(&base_url, &identity_url);
                    client.delete_cipher(&at, &entry_id).await
                }
            }).await;

            match res {
                Ok(_) => {
                    let mut state_guard = state.lock().await;
                    if let Some(db) = &mut state_guard.db {
                        db.entries.retain(|e| e.id != id);
                        db.pinned_ids.remove(&id);
                        db.usage_counts.remove(&id);
                        let _ = db.save(&config.server_name(), config.email.as_ref().unwrap());
                        state_guard.name_cache.remove(&id);
                        state_guard.username_cache.remove(&id);
                    }
                    Response::Ack
                }
                Err(e) => Response::Error { message: format!("delete entry failed: {}", e) },
            }
        }
        Action::UpdateEntry { entry } => {
            let (config, keys) = {
                let state = state.lock().await;
                let config = match cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                    Ok(c) => c,
                    Err(e) => return Response::Error { message: format!("failed to load config: {}", e) },
                };
                let keys = match &state.keys {
                    Some(k) => k.clone(),
                    None => return Response::Error { message: "agent is locked".to_string() },
                };
                (config, keys)
            };

            let entry_id = entry.id.clone();
            let res = update_entry_on_server(state, &entry, &config, &keys).await;

            match res {
                Ok(_) => {
                    // Trigger a sync to update the local encrypted cache securely
                    let sync_res = with_refresh(state, |at| async move {
                        let config = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy()?;
                        let client = cosmic_bwarden_core::api::Client::new(&config.base_url(), &config.identity_url());
                        client.sync(&at).await
                    }).await;

                    if let Ok((protected_key, protected_private_key, protected_org_keys, entries)) = sync_res {
                        let mut state_guard = state.lock().await;
                        let keys = state_guard.keys.clone();
                        if let (Some(db), Some(keys)) = (&mut state_guard.db, &keys) {
                            db.protected_key = Some(Secret::from(protected_key));
                            db.protected_private_key = protected_private_key.map(Secret::from);
                            db.protected_org_keys = protected_org_keys.into_iter().map(|(k, v)| (k, Secret::from(v))).collect();
                            db.entries = entries;

                            // Re-populate pinned_ids
                            db.pinned_ids.clear();
                            for entry in &db.entries {
                                for field in &entry.fields {
                                    if let Some(name) = &field.name {
                                        if let Ok(dec_name) = cosmic_bwarden_core::vault::decrypt(name, keys, entry.key.as_deref()) {
                                            if dec_name == "corbw-pinned" {
                                                if let Some(value) = &field.value {
                                                    if let Ok(dec_value) = cosmic_bwarden_core::vault::decrypt(value, keys, entry.key.as_deref()) {
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
                            let _ = db.save(&config.server_name(), config.email.as_ref().unwrap());
                            state_guard.name_cache.remove(&entry_id);
                            state_guard.username_cache.remove(&entry_id);
                        }
                    }
                    Response::Ack
                }
                Err(e) => Response::Error { message: format!("update entry failed: {}", e) },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_bwarden_core::db::{Db, Entry, EntryData};
    use cosmic_bwarden_core::protocol::{Action, Response};
    use cosmic_bwarden_core::locked;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn test_search_by_username() {
        let mut state = State::new();
        
        // Mock keys
        let mut key_data = locked::Vec::new();
        key_data.extend([0u8; 64].iter().copied());
        state.keys = Some(locked::Keys::new(key_data));
        
        // Mock DB with some entries
        let mut db = Db::new();
        db.entries = vec![
            Entry {
                id: "1".to_string(),
                name: "Site A".to_string(),
                data: EntryData::Login {
                    username: Some("user123".to_string()),
                    password: None,
                    totp: None,
                    uris: vec![],
                },
                org_id: None,
                folder: None,
                folder_id: None,
                fields: vec![],
                notes: None,
                history: vec![],
                key: None,
                master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
            },
            Entry {
                id: "2".to_string(),
                name: "Other Site".to_string(),
                data: EntryData::Login {
                    username: Some("anotheruser".to_string()),
                    password: None,
                    totp: None,
                    uris: vec![],
                },
                org_id: None,
                folder: None,
                folder_id: None,
                fields: vec![],
                notes: None,
                history: vec![],
                key: None,
                master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
            },
        ];
        state.db = Some(db);
        
        let state = Arc::new(Mutex::new(state));
        
        // Search by name
        let action = Action::GetEntries {
            query: Some("Site A".to_string()),
            entry_type: None,
        };
        let response = handle_request(action, &state).await;
        if let Response::Entries { entries } = response {
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].id, "1");
        } else {
            panic!("Expected Entries response");
        }
        
        // Search by username
        let action = Action::GetEntries {
            query: Some("user123".to_string()),
            entry_type: None,
        };
        let response = handle_request(action, &state).await;
        if let Response::Entries { entries } = response {
            assert_eq!(entries.len(), 1, "Should find entry by username");
            assert_eq!(entries[0].id, "1");
        } else {
            panic!("Expected Entries response");
        }

        // Search by partial username
        let action = Action::GetEntries {
            query: Some("123".to_string()),
            entry_type: None,
        };
        let response = handle_request(action, &state).await;
        if let Response::Entries { entries } = response {
            assert_eq!(entries.len(), 1, "Should find entry by partial username");
            assert_eq!(entries[0].id, "1");
        } else {
            panic!("Expected Entries response");
        }
    }

    #[tokio::test]
    async fn test_sidebar_entries_search() {
        let mut state = State::new();
        
        // Mock keys
        let mut key_data = locked::Vec::new();
        key_data.extend([0u8; 64].iter().copied());
        state.keys = Some(locked::Keys::new(key_data));
        
        // Mock DB with some entries
        let mut db = Db::new();
        db.entries = vec![
            Entry {
                id: "1".to_string(),
                name: "Site A".to_string(),
                data: EntryData::Login {
                    username: Some("user123".to_string()),
                    password: None,
                    totp: None,
                    uris: vec![],
                },
                org_id: None,
                folder: None,
                folder_id: None,
                fields: vec![],
                notes: None,
                history: vec![],
                key: None,
                master_password_reprompt: cosmic_bwarden_core::api::CipherRepromptType::None,
            },
        ];
        state.db = Some(db);
        
        let state = Arc::new(Mutex::new(state));
        
        // Search by name
        let action = Action::GetSidebarEntries {
            query: Some("Site A".to_string()),
            entry_type: None,
        };
        let response = handle_request(action, &state).await;
        if let Response::SidebarEntries { entries } = response {
            assert_eq!(entries.len(), 1);
        } else {
            panic!("Expected SidebarEntries response");
        }
        
        // Search by username
        let action = Action::GetSidebarEntries {
            query: Some("user123".to_string()),
            entry_type: None,
        };
        let response = handle_request(action, &state).await;
        if let Response::SidebarEntries { entries } = response {
            assert_eq!(entries.len(), 1, "Sidebar search should find by username");
        } else {
            panic!("Expected SidebarEntries response");
        }
    }
}
