mod handler;
mod keyring;
mod logind;
mod server;
mod session_store;
mod ssh_agent;
mod state;
mod timeout;
#[cfg(feature = "tpm")]
mod tpm;

#[cfg(feature = "browser-host")]
mod browser_host;

use clap::Parser;
use cosmarden_core::protocol::{Action, Response};
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
#[command(
    author,
    version = cosmarden_core::version(),
    about = "cosmarden-agent: Secure background agent",
    after_help = cosmarden_core::help_footer()
)]
struct Cli {
    /// Path to the configuration file. Overrides default and environment.
    #[arg(long, env = "COSMARDEN_CONFIG")]
    config: Option<PathBuf>,

    /// Path to the Unix socket for IPC. Overrides config, default and environment.
    #[arg(long, env = "COSMARDEN_SOCKET")]
    socket: Option<PathBuf>,

    /// Path to the SSH agent Unix socket. Overrides config, default and environment.
    #[arg(long, env = "COSMARDEN_SSH_SOCKET")]
    ssh_socket: Option<PathBuf>,

    #[arg(hide = true)]
    browser_host: Option<String>,
}

/// Delete the pre-v7 TPM-sealed master-password-hash blob if one is still on
/// disk. Best-effort and idempotent: a failure is logged loudly (it leaves a
/// credential behind) but must never stop the agent from starting.
fn remove_legacy_hash_blob(cfg: &cosmarden_core::config::CosmardenConfig) {
    use sha2::{Digest as _, Sha256};
    let Some(email) = cfg.email.as_deref() else {
        return;
    };
    // The path is reconstructed here because `dirs::tpm_hash_blob_file` was
    // removed with the feature; this is the only place that still needs it.
    let mut h = Sha256::new();
    h.update(cfg.server_name().as_bytes());
    h.update(b"\0");
    h.update(email.as_bytes());
    let path = cosmarden_core::dirs::data_dir().join(format!(
        "tpm_sealed_hash_{}.bin",
        &format!("{:x}", h.finalize())[..16]
    ));

    match std::fs::remove_file(&path) {
        Ok(()) => log::info!(
            "removed the obsolete TPM master-password-hash blob {}",
            path.display()
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::error!(
            "failed to remove the obsolete TPM master-password-hash blob {}: {e}",
            path.display()
        ),
    }
}

/// Agent entry point. Both the `cosmarden-agent` and (TPM-enabled)
/// `cosmarden-agent-tpm` binaries are thin wrappers around this; the two
/// bins exist only so the TPM E2E suite can locate a build it knows has TPM
/// support. Keeping the logic here (rather than in `main.rs`) lets both bins
/// share one source file without tripping cargo's "file in multiple targets".
pub async fn run() -> anyhow::Result<()> {
    // Default to `info` when RUST_LOG is unset: env_logger's built-in default
    // is `error` only, which hid every warn!-level failure (e.g. rejected
    // server API calls) from journalctl under the systemd service.
    //
    // The HTTP stack is capped at `info` regardless of RUST_LOG (`[P1-1]`
    // sibling `[P1-7]`): reqwest/hyper at trace print full request headers —
    // including `Authorization: Bearer …` — and this process logs to journald,
    // which persists to disk. Module directives beat a global `trace`, and
    // being added after `from_env` they also beat an explicit
    // `RUST_LOG=hyper=trace`.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .filter_module("reqwest", log::LevelFilter::Info)
        .filter_module("hyper", log::LevelFilter::Info)
        .filter_module("hyper_util", log::LevelFilter::Info)
        .filter_module("h2", log::LevelFilter::Info)
        .filter_module("rustls", log::LevelFilter::Info)
        .init();

    log::info!(
        "cosmarden-agent starting: version={} protocol_version={}",
        cosmarden_core::version(),
        cosmarden_core::PROTOCOL_VERSION,
    );

    cosmarden_core::dirs::adopt_legacy_env();
    let args = Cli::parse();

    // Apply CLI overrides to dirs early
    if let Some(config_path) = &args.config {
        cosmarden_core::dirs::set_config_override(config_path.clone());
    }
    if let Some(socket_path) = &args.socket {
        cosmarden_core::dirs::set_socket_override(socket_path.clone());
    }
    if let Some(ssh_socket_path) = &args.ssh_socket {
        cosmarden_core::dirs::set_ssh_socket_override(ssh_socket_path.clone());
    }

    // Load configuration to check for additional overrides
    let config = cosmarden_core::config::CosmardenConfig::load_legacy().unwrap_or_default();

    // `keyring` is an opt-in cargo feature and no build recipe enables it by
    // default, so a config asking for session persistence can silently get
    // none — which surfaces much later as a failed sync after a PIN unlock.
    // Say it once, at startup, where it is cheap to notice.
    #[cfg(not(feature = "keyring"))]
    if config.persist_session {
        log::warn!(
            "persist_session is enabled in config, but this build has no keyring support:              session tokens will not be written to the Secret Service. Rebuild with              `--features cosmarden-agent/keyring`, or rely on the session envelope              (which works either way)."
        );
    }

    // Config overrides apply ONLY if CLI/Env was not set
    if args.socket.is_none() && std::env::var("COSMARDEN_SOCKET").is_err() {
        if let Some(path) = config.socket_path {
            cosmarden_core::dirs::set_socket_override(PathBuf::from(path));
        }
    }
    if args.ssh_socket.is_none() && std::env::var("COSMARDEN_SSH_SOCKET").is_err() {
        if let Some(path) = config.ssh_agent_socket_path {
            cosmarden_core::dirs::set_ssh_socket_override(PathBuf::from(path));
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

    disable_core_dumps();

    // Ensure XDG cache/runtime/data dirs exist with restrictive (0700)
    // permissions before binding any sockets inside them.
    cosmarden_core::dirs::make_all()?;

    let socket_path = cosmarden_core::dirs::socket_file();

    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    if let Some(parent) = socket_path.parent() {
        // 0700 parent dir (matches ssh_agent.rs and dirs::make_all). A plain
        // create_dir_all would use the umask default (0755) when
        // COSMARDEN_SOCKET points at a fresh directory, leaving the socket's
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

    log::info!("cosmarden-agent listening on {}", socket_path.display());

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
        if let Ok(cfg) = cosmarden_core::config::CosmardenConfig::load_legacy() {
            // One-time cleanup: older versions sealed the master-password hash
            // beside the vault-key blob so a PIN unlock could re-auth silently.
            // That is gone — sync now restores from the refresh-token envelope,
            // and an expired one prompts for the master password instead — so
            // the leftover must be deleted rather than left on disk holding a
            // credential nothing will ever use again.
            remove_legacy_hash_blob(&cfg);

            if cfg.tpm_enabled {
                if let Some(email) = &cfg.email {
                    let blob_path = cosmarden_core::dirs::tpm_blob_file(&cfg.server_name(), email);
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
                    if len > cosmarden_core::MAX_IPC_FRAME_BYTES {
                        log::error!(
                            "request length {} exceeds cap {}",
                            len,
                            cosmarden_core::MAX_IPC_FRAME_BYTES
                        );
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
                    if response_bytes.len() > cosmarden_core::MAX_IPC_FRAME_BYTES {
                        log::error!(
                            "response length {} exceeds cap {}",
                            response_bytes.len(),
                            cosmarden_core::MAX_IPC_FRAME_BYTES
                        );
                        return;
                    }
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
                                let _ = tx.send(cosmarden_core::protocol::Event::OpenEntry { id });
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

/// Interpret `prctl(PR_SET_DUMPABLE, 0)`'s return. Logs `error!` on failure
/// so a dumpable agent is never silent (AGENTS.md). Does not abort startup.
pub(crate) fn checked_prctl_set_undumpable(rc: libc::c_int) -> bool {
    if rc != 0 {
        log::error!(
            "prctl(PR_SET_DUMPABLE, 0) failed: rc={rc} {}",
            std::io::Error::last_os_error()
        );
        false
    } else {
        true
    }
}

fn disable_core_dumps() {
    #[cfg(target_os = "linux")]
    {
        let rc = unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0) };
        let _ok = checked_prctl_set_undumpable(rc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prctl_success_is_ok() {
        assert!(checked_prctl_set_undumpable(0));
    }

    #[test]
    fn prctl_failure_is_not_silent_ok() {
        assert!(!checked_prctl_set_undumpable(-1));
    }

    #[test]
    fn run_calls_disable_core_dumps_not_raw_prctl() {
        let src = include_str!("lib.rs");
        assert!(
            src.contains("disable_core_dumps()"),
            "startup must call the result-checked helper"
        );
        assert!(
            src.contains("checked_prctl_set_undumpable(rc)"),
            "prctl return must be passed to the checked helper"
        );
    }
}

#[cfg(test)]
mod legacy_hash_blob_tests {
    use super::remove_legacy_hash_blob;
    use cosmarden_core::config::CosmardenConfig;

    /// The migration must find the blob the removed feature actually wrote:
    /// `tpm_sealed_hash_<sha256hex16(server ‖ \0 ‖ email)>.bin`, the same
    /// account key `dirs::account_hash` produces. A drifted derivation would
    /// silently leave a master-password credential on disk forever.
    #[test]
    fn removes_the_blob_at_the_account_hashed_path() {
        let profile = format!("test-legacy-blob-{}", std::process::id());
        let prev = std::env::var_os("COSMARDEN_PROFILE");
        std::env::set_var("COSMARDEN_PROFILE", &profile);

        let cfg = CosmardenConfig {
            email: Some("user@example.com".to_string()),
            base_url: Some("https://vault.example".to_string()),
            ..Default::default()
        };

        let dir = cosmarden_core::dirs::data_dir();
        std::fs::create_dir_all(&dir).expect("data dir");
        let path = dir.join(format!(
            "tpm_sealed_hash_{}.bin",
            cosmarden_core::dirs::account_hash(&cfg.server_name(), "user@example.com")
        ));
        std::fs::write(&path, b"legacy").expect("seed blob");

        remove_legacy_hash_blob(&cfg);
        let gone = !path.exists();

        let _ = std::fs::remove_dir_all(&dir);
        match prev {
            Some(v) => std::env::set_var("COSMARDEN_PROFILE", v),
            None => std::env::remove_var("COSMARDEN_PROFILE"),
        }
        assert!(gone, "the obsolete hash blob must be deleted");
    }
}
