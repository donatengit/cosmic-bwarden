use crate::protocol::{Action, Response};
use crate::error::{Error, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

pub struct AgentClient {
    socket_path: std::path::PathBuf,
}

impl AgentClient {
    pub fn new() -> Self {
        Self {
            socket_path: crate::dirs::socket_file(),
        }
    }

    pub async fn send(&self, action: Action) -> Result<Response> {
        let mut socket = UnixStream::connect(&self.socket_path).await.map_err(|e| Error::Other(format!("failed to connect to agent: {}", e)))?;
        let request_bytes = serde_json::to_vec(&action).map_err(|e| Error::Other(format!("failed to serialize request: {}", e)))?;
        socket.write_all(&request_bytes).await.map_err(|e| Error::Other(format!("failed to write to socket: {}", e)))?;
        
        let mut buf = Vec::new();
        socket.read_to_end(&mut buf).await.map_err(|e| Error::Other(format!("failed to read from socket: {}", e)))?;
        
        serde_json::from_slice(&buf).map_err(|e| Error::Other(format!("failed to deserialize response: {}", e)))
    }
}
