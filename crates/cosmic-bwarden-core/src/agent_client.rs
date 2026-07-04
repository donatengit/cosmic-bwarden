use crate::error::{Error, Result};
use crate::protocol::{Action, Response};
use std::sync::LazyLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

struct PersistentConn {
    path: std::path::PathBuf,
    stream: Option<UnixStream>,
}

// One persistent connection per process. Serialises IPC access (one request
// in flight at a time) and eliminates per-request connect/accept/UID-check
// overhead. Auto-reconnects on IO error with one retry.
static CONN: LazyLock<Mutex<PersistentConn>> = LazyLock::new(|| {
    Mutex::new(PersistentConn {
        path: std::path::PathBuf::new(),
        stream: None,
    })
});

pub struct AgentClient {
    socket_path: std::path::PathBuf,
}

impl Default for AgentClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentClient {
    pub fn new() -> Self {
        Self {
            socket_path: crate::dirs::socket_file(),
        }
    }

    pub fn new_with_socket(path: std::path::PathBuf) -> Self {
        Self { socket_path: path }
    }

    pub async fn send(&self, action: Action) -> Result<Response> {
        let mut guard = CONN.lock().await;

        // Force reconnect if the socket path has changed (e.g. test isolation).
        if guard.path != self.socket_path {
            guard.stream = None;
            guard.path = self.socket_path.clone();
        }

        for attempt in 0..2u8 {
            if guard.stream.is_none() {
                guard.stream = Some(
                    UnixStream::connect(&self.socket_path)
                        .await
                        .map_err(|e| Error::Other(format!("failed to connect to agent: {e}")))?,
                );
            }

            match Self::do_send(guard.stream.as_mut().unwrap(), &action).await {
                Ok(response) => return Ok(response),
                Err(_) if attempt == 0 => {
                    // Stale connection — drop it and try once more with a fresh one.
                    guard.stream = None;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn do_send(socket: &mut UnixStream, action: &Action) -> Result<Response> {
        let request_bytes = postcard::to_allocvec(action)
            .map_err(|e| Error::Other(format!("failed to serialize request: {e}")))?;
        let len = request_bytes.len() as u32;
        socket
            .write_all(&len.to_le_bytes())
            .await
            .map_err(|e| Error::Other(format!("socket write error: {e}")))?;
        socket
            .write_all(&request_bytes)
            .await
            .map_err(|e| Error::Other(format!("socket write error: {e}")))?;

        let mut len_buf = [0u8; 4];
        socket
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| Error::Other(format!("socket read error: {e}")))?;
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        socket
            .read_exact(&mut buf)
            .await
            .map_err(|e| Error::Other(format!("socket read error: {e}")))?;

        postcard::from_bytes(&buf)
            .map_err(|e| Error::Other(format!("failed to deserialize response: {e}")))
    }
}
