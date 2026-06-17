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

    pub fn new_with_socket(path: std::path::PathBuf) -> Self {
        Self {
            socket_path: path,
        }
    }

    pub async fn send(&self, action: Action) -> Result<Response> {
        let mut socket = UnixStream::connect(&self.socket_path).await.map_err(|e| Error::Other(format!("failed to connect to agent: {}", e)))?;
        
        let request_bytes = postcard::to_allocvec(&action).map_err(|e| Error::Other(format!("failed to serialize request: {}", e)))?;
        let len = request_bytes.len() as u32;
        socket.write_all(&len.to_le_bytes()).await.map_err(|e| Error::Other(format!("failed to write length to socket: {}", e)))?;
        socket.write_all(&request_bytes).await.map_err(|e| Error::Other(format!("failed to write to socket: {}", e)))?;
        
        let mut len_buf = [0u8; 4];
        socket.read_exact(&mut len_buf).await.map_err(|e| Error::Other(format!("failed to read length from socket: {}", e)))?;
        let len = u32::from_le_bytes(len_buf) as usize;
        
        let mut buf = vec![0u8; len];
        socket.read_exact(&mut buf).await.map_err(|e| Error::Other(format!("failed to read from socket: {}", e)))?;
        
        postcard::from_bytes(&buf).map_err(|e| Error::Other(format!("failed to deserialize response: {}", e)))
    }
}
