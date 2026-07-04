use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::Action;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn run() -> anyhow::Result<()> {
    let client = AgentClient::new();
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    log::info!("Browser host started, waiting for messages on stdin");

    loop {
        // Read 4-byte length prefix (native endian)
        let mut len_buf = [0u8; 4];
        if let Err(e) = stdin.read_exact(&mut len_buf).await {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                log::info!("Browser closed connection (EOF)");
                break;
            }
            log::error!("Failed to read length prefix: {}", e);
            return Err(e.into());
        }
        let len = u32::from_le_bytes(len_buf) as usize;

        // Safety limit for browser messages (e.g. 1MB)
        if len > 1024 * 1024 {
            log::error!("Message too large: {} bytes", len);
            anyhow::bail!("Message too large");
        }

        // Read JSON payload
        let mut buf = vec![0u8; len];
        if let Err(e) = stdin.read_exact(&mut buf).await {
            log::error!("Failed to read message body: {}", e);
            return Err(e.into());
        }
        
        let action: Action = match serde_json::from_slice(&buf) {
            Ok(a) => a,
            Err(e) => {
                // Log the serde error only — never the raw body. A near-miss
                // message (valid-ish JSON carrying a password/pin that failed to
                // match the Action schema) would otherwise leak verbatim into the
                // journal under RUST_LOG=debug.
                log::error!("Failed to deserialize action from JSON: {}", e);

                let error_response = serde_json::json!({
                    "Error": { "message": format!("Invalid protocol message: {}", e) }
                });
                let response_json = serde_json::to_vec(&error_response)?;
                let len = response_json.len() as u32;
                if let Err(e) = stdout.write_all(&len.to_le_bytes()).await
                    .and(stdout.write_all(&response_json).await)
                    .and(stdout.flush().await)
                {
                    log::error!("failed to send parse-error response to browser: {}", e);
                }
                continue;
            }
        };
        
        log::debug!("Forwarding action to agent: {:?}", action);
        
        // Send to agent
        match client.send(action).await {
            Ok(response) => {
                // Send back to browser
                let response_json = serde_json::to_vec(&response)?;
                let len = response_json.len() as u32;
                if let Err(e) = stdout.write_all(&len.to_le_bytes()).await {
                    log::error!("Failed to write length prefix to stdout: {}", e);
                    return Err(e.into());
                }
                if let Err(e) = stdout.write_all(&response_json).await {
                    log::error!("Failed to write response JSON to stdout: {}", e);
                    return Err(e.into());
                }
                if let Err(e) = stdout.flush().await {
                    log::error!("Failed to flush stdout: {}", e);
                    return Err(e.into());
                }
            }
            Err(e) => {
                log::error!("Agent communication error: {}", e);
                // We should probably return a JSON error to the browser
                let error_response = serde_json::json!({
                    "Error": { "message": format!("Agent error: {}", e) }
                });
                let response_json = serde_json::to_vec(&error_response)?;
                let len = response_json.len() as u32;
                if let Err(e) = stdout.write_all(&len.to_le_bytes()).await
                    .and(stdout.write_all(&response_json).await)
                    .and(stdout.flush().await)
                {
                    log::error!("failed to send agent-error response to browser: {}", e);
                }
            }
        }
    }

    Ok(())
}
