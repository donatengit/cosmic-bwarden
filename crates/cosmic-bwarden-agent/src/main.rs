mod handler;
mod keyring;
mod logind;
mod server;
mod ssh_agent;
mod state;

#[cfg(feature = "browser-host")]
mod browser_host;

use cosmic_bwarden_core::protocol::{Action, Response};
use handler::handle_request;
use logind::listen_to_logind;
use ssh_agent::SshAgent;
use state::State;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::Mutex;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "browser-host" {
        #[cfg(feature = "browser-host")]
        {
            return browser_host::run().await;
        }
        #[cfg(not(feature = "browser-host"))]
        {
            anyhow::bail!("browser-host feature is not enabled in this build");
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
        std::fs::create_dir_all(parent)?;
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
    {
        let mut state_guard = state.lock().await;
        state_guard.shutdown_tx = Some(shutdown_tx);
    }
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
                    let mut len_buf = [0u8; 4];
                    if let Err(e) = socket.read_exact(&mut len_buf).await {
                        if e.kind() != std::io::ErrorKind::UnexpectedEof {
                            log::error!("failed to read length from socket: {}", e);
                        }
                        return;
                    }
                    let len = u32::from_le_bytes(len_buf) as usize;
                    log::debug!("Read request length: {}", len);
                    let mut buf = vec![0u8; len];
                    if let Err(e) = socket.read_exact(&mut buf).await {
                        log::error!("failed to read from socket: {}", e);
                        return;
                    }

                    let request: Action = match postcard::from_bytes(&buf) {
                        Ok(req) => {
                            log::debug!("Parsed request: {:?}", req);
                            req
                        },
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
                    let response = handle_request(request, &state).await;
                    log::debug!("Response: {:?}", response);

                    if let Response::Error { message } = &response {
                        log::error!("request failed: {}", message);
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
                        // Enter event streaming loop
                        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                        {
                            let mut state_guard = state.lock().await;
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
                    } else {
                        // For non-subscription requests, close the connection after one response
                        // to match the expectation of AgentClient::send
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
        _ = shutdown_rx.recv() => {
            log::info!("Shutting down agent gracefully");
        },
    }

    Ok(())
}
