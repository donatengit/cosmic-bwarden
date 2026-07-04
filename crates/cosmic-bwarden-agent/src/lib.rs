mod handler;
mod keyring;
mod logind;
mod server;
mod ssh_agent;
mod state;
mod timeout;
#[cfg(feature = "tpm")]
mod tpm;

#[cfg(feature = "browser-host")]
mod browser_host;

use clap::Parser;
use cosmic_bwarden_core::protocol::{Action, Response};
use handler::handle_request;
use logind::listen_to_logind;
use ssh_agent::SshAgent;
use state::State;
use std::path::PathBuf;
use std::sync::Arc;
use timeout::AutolockTimer;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::Mutex;

#[derive(Parser)]
#[command(author, version = cosmic_bwarden_core::version(), about = "cosmic-bwarden-agent: Secure background agent")]
struct Cli {
    /// Path to the configuration file. Overrides default and environment.
    #[arg(long, env = "COSMIC_BWARDEN_CONFIG")]
    config: Option<PathBuf>,

    /// Path to the Unix socket for IPC. Overrides config, default and environment.
    #[arg(long, env = "COSMIC_BWARDEN_SOCKET")]
    socket: Option<PathBuf>,

    /// Path to the SSH agent Unix socket. Overrides config, default and environment.
    #[arg(long, env = "COSMIC_BWARDEN_SSH_SOCKET")]
    ssh_socket: Option<PathBuf>,

    #[arg(hide = true)]
    browser_host: Option<String>,
}

/// Agent entry point. Both the `cosmic-bwarden-agent` and (TPM-enabled)
/// `cosmic-bwarden-agent-tpm` binaries are thin wrappers around this; the two
/// bins exist only so the TPM E2E suite can locate a build it knows has TPM
/// support. Keeping the logic here (rather than in `main.rs`) lets both bins
/// share one source file without tripping cargo's "file in multiple targets".
pub async fn run() -> anyhow::Result<()> {
    // Default to `info` when RUST_LOG is unset: env_logger's built-in default
    // is `error` only, which hid every warn!-level failure (e.g. rejected
    // server API calls) from journalctl under the systemd service.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Cli::parse();

    // Apply CLI overrides to dirs early
    if let Some(config_path) = &args.config {
        cosmic_bwarden_core::dirs::set_config_override(config_path.clone());
    }
    if let Some(socket_path) = &args.socket {
        cosmic_bwarden_core::dirs::set_socket_override(socket_path.clone());
    }
    if let Some(ssh_socket_path) = &args.ssh_socket {
        cosmic_bwarden_core::dirs::set_ssh_socket_override(ssh_socket_path.clone());
    }

    // Load configuration to check for additional overrides
    let config =
        cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy().unwrap_or_default();

    // Config overrides apply ONLY if CLI/Env was not set
    if args.socket.is_none() && std::env::var("COSMIC_BWARDEN_SOCKET").is_err() {
        if let Some(path) = config.socket_path {
            cosmic_bwarden_core::dirs::set_socket_override(PathBuf::from(path));
        }
    }
    if args.ssh_socket.is_none() && std::env::var("COSMIC_BWARDEN_SSH_SOCKET").is_err() {
        if let Some(path) = config.ssh_agent_socket_path {
            cosmic_bwarden_core::dirs::set_ssh_socket_override(PathBuf::from(path));
        }
    }

    if let Some(bh) = args.browser_host {
        if bh == "browser-host" {
            #[cfg(feature = "browser-host")]
            {
                return browser_host::run().await;
            }
            #[cfg(not(feature = "browser-host"))]
            {
                anyhow::bail!("browser-host feature is not enabled in this build");
            }
        }
    }

    // Prevent core dumps and ptrace attachment
    #[cfg(target_os = "linux")]
    {
        unsafe {
            libc::prctl(libc::PR_SET_DUMPABLE, 0);
        }
    }

    // Ensure XDG cache/runtime/data dirs exist with restrictive (0700)
    // permissions before binding any sockets inside them.
    cosmic_bwarden_core::dirs::make_all()?;

    let socket_path = cosmic_bwarden_core::dirs::socket_file();

    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    if let Some(parent) = socket_path.parent() {
        // 0700 parent dir (matches ssh_agent.rs and dirs::make_all). A plain
        // create_dir_all would use the umask default (0755) when
        // COSMIC_BWARDEN_SOCKET points at a fresh directory, leaving the socket's
        // directory world-traversable. The socket itself is 0600 + peer-cred, so
        // this is defence in depth, but the two socket paths should be consistent.
        use std::os::unix::fs::DirBuilderExt as _;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }

    let listener = UnixListener::bind(&socket_path)?;

    // Enforce 0600 permissions on the socket
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;

    log::info!(
        "cosmic-bwarden-agent listening on {}",
        socket_path.display()
    );

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::unbounded_channel();
    let state = Arc::new(Mutex::new(State::new()));

    // Autolock timer — checks every 5 minutes whether the inactivity threshold
    // has been exceeded. timer_handle is cloned into the request loop.
    let autolock_timer = AutolockTimer::new(config.lock_timeout);
    let timer_handle = autolock_timer.handle();
    {
        let mut state_guard = state.lock().await;
        state_guard.shutdown_tx = Some(shutdown_tx);

        // Detect whether a TPM sealed blob exists for the configured account.
        // Done at startup so request_unlock() knows whether to broadcast
        // PinRequested or UnlockRequested before the first unlock attempt.
        if let Ok(cfg) = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
            if cfg.tpm_enabled {
                if let Some(email) = &cfg.email {
                    let blob_path =
                        cosmic_bwarden_core::dirs::tpm_blob_file(&cfg.server_name(), email);
                    state_guard.tpm_configured = blob_path.exists();
                }
            }
        }
    }
    let ssh_agent = SshAgent::new(Arc::clone(&state));

    let state_for_agent = Arc::clone(&state);
    let timer_handle_for_agent = timer_handle.clone();
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
            let timer_handle = timer_handle_for_agent.clone();

            tokio::spawn(async move {
                loop {
                    let mut len_buf = [0u8; 4];
                    if let Err(e) = socket.read_exact(&mut len_buf).await {
                        if e.kind() != std::io::ErrorKind::UnexpectedEof {
                            log::error!("failed to read length from socket: {}", e);
                        }
                        return;
                    }
                    let len = u32::from_le_bytes(len_buf) as usize;
                    log::debug!("Read request length: {}", len);
                    // Cap request size so a malformed/hostile length prefix can't
                    // drive an unbounded allocation (matches the browser host cap).
                    const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
                    if len > MAX_REQUEST_BYTES {
                        log::error!("request length {} exceeds cap {}", len, MAX_REQUEST_BYTES);
                        return;
                    }
                    let mut buf = vec![0u8; len];
                    if let Err(e) = socket.read_exact(&mut buf).await {
                        log::error!("failed to read from socket: {}", e);
                        return;
                    }

                    let request: Action = match postcard::from_bytes(&buf) {
                        Ok(req) => {
                            log::debug!("Parsed request: {:?}", req);
                            req
                        }
                        Err(e) => {
                            log::error!("failed to deserialize request: {}", e);
                            let response = Response::Error {
                                message: format!("invalid request: {}", e),
                            };
                            let response_bytes = postcard::to_allocvec(&response).unwrap();
                            let len = response_bytes.len() as u32;
                            let _ = socket.write_all(&len.to_le_bytes()).await;
                            let _ = socket.write_all(&response_bytes).await;
                            return;
                        }
                    };

                    let is_subscribe = matches!(request, Action::Subscribe);
                    // Actions that should NOT reset the inactivity timer.
                    let is_non_activity = matches!(
                        &request,
                        Action::Lock
                            | Action::Logout
                            | Action::Quit
                            | Action::Subscribe
                            | Action::Version
                            | Action::GetConfig
                            | Action::UpdateLockTimeout { .. }
                    );
                    // UpdateLockTimeout is handled here; all other actions go to dispatch.
                    let response = if let Action::UpdateLockTimeout { seconds } = request {
                        timer_handle.set_duration(seconds);
                        Response::Ack
                    } else {
                        handle_request(request, &state).await
                    };
                    log::debug!("Response: {:?}", response);

                    // Reset the inactivity timer on any successful vault operation.
                    if !is_non_activity && !matches!(&response, Response::Error { .. }) {
                        timer_handle.reset();
                    }

                    if let Response::Error { message } = &response {
                        log::warn!("request failed: {}", message);
                    }
                    let response_bytes = postcard::to_allocvec(&response).unwrap();
                    let len = response_bytes.len() as u32;
                    log::debug!("Writing response length: {}", len);
                    if let Err(e) = socket.write_all(&len.to_le_bytes()).await {
                        log::error!("failed to write length to socket: {}", e);
                        return;
                    }
                    if let Err(e) = socket.write_all(&response_bytes).await {
                        log::error!("failed to write to socket: {}", e);
                        return;
                    }

                    if is_subscribe {
                        // Enter long-lived event streaming loop; connection is
                        // dedicated to this subscriber from here on.
                        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                        {
                            let mut state_guard = state.lock().await;
                            // Deliver any pending deep-link entry to this new subscriber
                            // (covers the case where the vault window wasn't open yet
                            // when SetPendingEntry was called).
                            if let Some(id) = state_guard.pending_entry_id.take() {
                                let _ =
                                    tx.send(cosmic_bwarden_core::protocol::Event::OpenEntry { id });
                            }
                            state_guard.subscribers.push(tx);
                        }

                        while let Some(event) = rx.recv().await {
                            let response = Response::Event { event };
                            let response_bytes = postcard::to_allocvec(&response).unwrap();
                            let len = response_bytes.len() as u32;
                            if let Err(e) = socket.write_all(&len.to_le_bytes()).await {
                                log::debug!("subscriber disconnected (length): {}", e);
                                break;
                            }
                            if let Err(e) = socket.write_all(&response_bytes).await {
                                log::debug!("subscriber disconnected (body): {}", e);
                                break;
                            }
                        }
                        return;
                    }
                    // Non-subscribe: loop back and read the next request on the
                    // same connection. The client keeps it alive between calls.
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

    // Autolock task: polls every 5 minutes for inactivity threshold.
    let state_for_timer = Arc::clone(&state);
    let timer_handle_for_lock = tokio::spawn(autolock_timer.run(state_for_timer));

    tokio::select! {
        _ = agent_handle => {},
        _ = ssh_agent_handle => {},
        _ = logind_handle => {},
        _ = timer_handle_for_lock => {},
        _ = shutdown_rx.recv() => {
            log::info!("Shutting down agent gracefully");
        },
    }

    Ok(())
}
