pub(crate) mod identities;
pub(crate) mod sign;

pub use sign::DEFAULT_SIGN_WAIT;

use crate::state::State;
use ssh_agent_lib::agent::{Agent, Session};
use ssh_agent_lib::proto::Extension;
use ssh_agent_lib::ssh_key::Signature;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Per-connection session factory. `ssh-agent-lib`'s blanket `Agent` impl ignores
/// the accepted socket, so we implement `Agent` explicitly to enforce a
/// `SO_PEERCRED` same-UID check on every connection — matching the main IPC
/// socket. The 0600 socket + 0700 parent dir already restrict access, but we
/// verify the peer credential too rather than relying on filesystem perms alone.
struct SshAgentFactory {
    state: Arc<Mutex<State>>,
    sign_wait: Duration,
}

impl Agent<tokio::net::UnixListener> for SshAgentFactory {
    fn new_session(&mut self, socket: &tokio::net::UnixStream) -> impl Session {
        let my_uid = rustix::process::getuid().as_raw();
        let authorized = match socket.peer_cred() {
            Ok(cred) if cred.uid() == my_uid => true,
            Ok(cred) => {
                log::warn!(
                    "ssh-agent: rejected connection from unauthorized UID: {}",
                    cred.uid()
                );
                false
            }
            Err(e) => {
                log::error!("ssh-agent: failed to get peer credentials: {}", e);
                false
            }
        };
        SshAgent {
            state: Arc::clone(&self.state),
            authorized,
            sign_wait: self.sign_wait,
        }
    }
}

#[derive(Clone)]
pub struct SshAgent {
    state: Arc<Mutex<State>>,
    /// False when the connecting peer's UID did not match ours; such a session
    /// answers every request as if the agent were empty/locked.
    authorized: bool,
    sign_wait: Duration,
}

impl SshAgent {
    pub fn new(state: Arc<Mutex<State>>) -> Self {
        Self::with_sign_wait(state, DEFAULT_SIGN_WAIT)
    }

    pub fn with_sign_wait(state: Arc<Mutex<State>>, sign_wait: Duration) -> Self {
        Self {
            state,
            authorized: true,
            sign_wait,
        }
    }

    #[cfg(test)]
    pub fn unauthorized(state: Arc<Mutex<State>>) -> Self {
        Self {
            state,
            authorized: false,
            sign_wait: DEFAULT_SIGN_WAIT,
        }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
        let socket = cosmic_bwarden_core::dirs::ssh_agent_socket_file();
        let _ = std::fs::remove_file(&socket);
        if let Some(parent) = socket.parent() {
            // 0700 parent dir (matches dirs::make_all); default create_dir_all
            // would use 0755 when the socket path is overridden to a fresh dir.
            let _ = std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent);
        }

        let listener = tokio::net::UnixListener::bind(&socket)?;

        // Enforce 0600 permissions on the socket, matching the main IPC
        // socket (main.rs) and the security model of a real ssh-agent.
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;

        let factory = SshAgentFactory {
            state: self.state,
            sign_wait: self.sign_wait,
        };
        ssh_agent_lib::agent::listen(listener, factory).await?;
        Ok(())
    }
}

#[ssh_agent_lib::async_trait]
impl ssh_agent_lib::agent::Session for SshAgent {
    async fn request_identities(
        &mut self,
    ) -> Result<Vec<ssh_agent_lib::proto::Identity>, ssh_agent_lib::error::AgentError> {
        if !self.authorized {
            return Ok(Vec::new());
        }
        identities::list_identities(&self.state).await
    }

    async fn extension(
        &mut self,
        _extension: Extension,
    ) -> Result<Option<Extension>, ssh_agent_lib::error::AgentError> {
        // Return Ok(None) so the library sends SSH_AGENT_SUCCESS without logging
        // an error. SSH clients routinely probe for extensions (command 27) and
        // handle a silent "not supported" response correctly.
        Ok(None)
    }

    async fn sign(
        &mut self,
        request: ssh_agent_lib::proto::SignRequest,
    ) -> Result<Signature, ssh_agent_lib::error::AgentError> {
        if !self.authorized {
            return Err(ssh_agent_lib::error::AgentError::other(
                cosmic_bwarden_core::error::Error::Other("unauthorized peer".to_string()),
            ));
        }
        sign::sign_request(&self.state, request, self.sign_wait).await
    }
}

#[cfg(test)]
mod tests;
