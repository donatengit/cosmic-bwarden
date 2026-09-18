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
        write_action(socket, action).await?;
        read_response(socket).await
    }
}

/// Frame and send one action (`u32` LE length prefix + postcard body).
async fn write_action(socket: &mut UnixStream, action: &Action) -> Result<()> {
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
    Ok(())
}

/// Read one framed response, refusing a length prefix that would allocate more
/// than [`crate::MAX_IPC_FRAME_BYTES`] (`crate::ipc_frame_len` owns the cap).
async fn read_response(socket: &mut UnixStream) -> Result<Response> {
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

/// A dedicated `Subscribe` connection, for clients that need live
/// lock/unlock/vault-change state.
///
/// `Subscribe` hijacks its socket: the agent answers `Ack` and from then on
/// writes nothing but `Response::Event`, never reading another request. That is
/// why this cannot ride [`AgentClient`]'s persistent request/response socket —
/// subscribing there would poison every later request in the process.
pub struct EventStream {
    socket: UnixStream,
}

impl EventStream {
    /// Subscribe on the agent socket for this profile
    /// ([`crate::dirs::socket_file`], so `COSMARDEN_PROFILE` applies).
    pub async fn connect() -> Result<Self> {
        Self::connect_with_socket(crate::dirs::socket_file()).await
    }

    /// [`Self::connect`] against an explicit socket path.
    pub async fn connect_with_socket(path: std::path::PathBuf) -> Result<Self> {
        let mut socket = UnixStream::connect(&path)
            .await
            .map_err(|e| Error::Other(format!("failed to connect to agent: {e}")))?;
        write_action(&mut socket, &Action::Subscribe).await?;
        match read_response(&mut socket).await? {
            Response::Ack => Ok(Self { socket }),
            other => Err(Error::Other(format!("agent refused Subscribe: {other:?}"))),
        }
    }

    /// The next event. An `Err` means the stream is finished — the agent
    /// restarted, exited, or closed the socket — so the caller reconnects and
    /// re-reads state rather than treating this as a fatal error.
    pub async fn next_event(&mut self) -> Result<crate::protocol::Event> {
        loop {
            match read_response(&mut self.socket).await? {
                Response::Event { event } => return Ok(event),
                // `Ack` and any other frame are noise on an events-only
                // socket: keep reading instead of dropping the subscription.
                _ => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Action;

    /// Read one framed `Action` from the client end (the mirror of
    /// [`write_action`]).
    async fn read_action(socket: &mut UnixStream) -> Action {
        use tokio::io::AsyncReadExt as _;

        let mut len_buf = [0u8; 4];
        socket.read_exact(&mut len_buf).await.expect("read length");
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        socket.read_exact(&mut buf).await.expect("read body");
        postcard::from_bytes(&buf).expect("parse action")
    }

    /// A fake agent that accepts one connection, asserts the client sent
    /// `Subscribe`, answers `Ack`, then writes the frames it is given.
    /// Returns the socket path of the fake agent.
    async fn fake_agent(frames: Vec<Response>) -> (tempfile::TempDir, std::path::PathBuf) {
        use tokio::io::AsyncWriteExt as _;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("socket");
        let listener = tokio::net::UnixListener::bind(&path).expect("bind fake agent");
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            assert!(
                matches!(read_action(&mut socket).await, Action::Subscribe),
                "client must send Subscribe first"
            );
            for response in std::iter::once(Response::Ack).chain(frames) {
                let bytes = postcard::to_allocvec(&response).expect("serialize");
                let _ = socket.write_all(&(bytes.len() as u32).to_le_bytes()).await;
                let _ = socket.write_all(&bytes).await;
            }
            // Hold the connection open until the client drops it.
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });
        (dir, path)
    }

    #[tokio::test]
    async fn event_stream_subscribes_and_yields_events() {
        use crate::protocol::Event;

        let (_dir, path) = fake_agent(vec![
            Response::Event {
                event: Event::Unlocked,
            },
            Response::Event {
                event: Event::VaultChanged,
            },
        ])
        .await;

        let mut stream = EventStream::connect_with_socket(path)
            .await
            .expect("subscribe");
        assert!(
            matches!(stream.next_event().await, Ok(Event::Unlocked)),
            "first event must be the first one the agent wrote"
        );
        assert!(matches!(stream.next_event().await, Ok(Event::VaultChanged)));
    }

    /// `Ack` is not an event: the stream skips it rather than handing it to the
    /// caller (the UI used to have to filter it by hand).
    #[tokio::test]
    async fn a_second_ack_is_skipped_not_returned() {
        use crate::protocol::Event;

        let (_dir, path) = fake_agent(vec![
            Response::Ack,
            Response::Event {
                event: Event::Locked,
            },
        ])
        .await;

        let mut stream = EventStream::connect_with_socket(path)
            .await
            .expect("subscribe");
        assert!(matches!(stream.next_event().await, Ok(Event::Locked)));
    }

    /// A closed socket is an error the caller can act on (reconnect), never a
    /// panic or a busy loop. The peer end is dropped before the first read, so
    /// the read is immediately EOF.
    #[tokio::test]
    async fn a_closed_stream_errors() {
        let (client, server) = tokio::net::UnixStream::pair().expect("socket pair");
        drop(server);
        let mut stream = EventStream { socket: client };
        assert!(
            stream.next_event().await.is_err(),
            "EOF must surface as an error, not hang"
        );
    }

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
