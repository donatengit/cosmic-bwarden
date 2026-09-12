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
        let len = crate::ipc_frame_len(u32::from_le_bytes(len_buf))?;
        let mut buf = vec![0u8; len];
        socket
            .read_exact(&mut buf)
            .await
            .map_err(|e| Error::Other(format!("socket read error: {e}")))?;

        postcard::from_bytes(&buf)
            .map_err(|e| Error::Other(format!("failed to deserialize response: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Action;

    #[tokio::test]
    async fn do_send_rejects_oversized_response_length() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let (client, mut server) = tokio::net::UnixStream::pair().unwrap();
        tokio::spawn(async move {
            let mut len_buf = [0u8; 4];
            if server.read_exact(&mut len_buf).await.is_err() {
                return;
            }
            let n = u32::from_le_bytes(len_buf) as usize;
            let mut req = vec![0u8; n];
            let _ = server.read_exact(&mut req).await;
            let _ = server.write_all(&u32::MAX.to_le_bytes()).await;
        });
        let mut stream = client;
        let err = AgentClient::do_send(&mut stream, &Action::Version).await;
        assert!(
            err.is_err(),
            "claimed u32::MAX response length must not allocate"
        );
        let msg = format!("{err:?}");
        assert!(
            msg.contains("exceeds cap") || msg.contains("IPC frame"),
            "error should name the cap, got {msg}"
        );
    }
}
