//! Container-free regression tests for the agent's IPC hardening invariants
//! (AGENTS.md "Security Invariants"): socket file modes and the request-size
//! cap. These spawn the agent binary directly with an isolated profile — no
//! Vaultwarden needed, so they are fast and immune to container flakes.

use anyhow::Result;
use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::time::Duration;

struct BareAgent {
    child: std::process::Child,
    socket_path: PathBuf,
    ssh_socket_path: PathBuf,
    _temp_dir: tempfile::TempDir,
}

impl Drop for BareAgent {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Spawn the debug agent with all state under a fresh temp dir. No container,
/// no account — enough for socket/framing checks.
async fn spawn_bare_agent() -> Result<BareAgent> {
    let temp_dir = tempfile::tempdir()?;
    let base = temp_dir.path();
    for sub in ["config", "cache", "data", "runtime"] {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(base.join(sub))?;
    }
    let socket_path = base.join("runtime/socket");
    let ssh_socket_path = base.join("runtime/ssh-agent-socket");

    let mut agent_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    agent_path.pop();
    agent_path.pop();
    agent_path.push("target/debug/cosmic-bwarden-agent");

    let child = std::process::Command::new(&agent_path)
        .arg("--socket")
        .arg(&socket_path)
        .arg("--ssh-socket")
        .arg(&ssh_socket_path)
        .arg("--config")
        .arg(base.join("config/config.json"))
        .env("COSMIC_BWARDEN_PROFILE", format!("test-{}", uuid::Uuid::new_v4()))
        .env("XDG_CONFIG_HOME", base.join("config"))
        .env("XDG_CACHE_HOME", base.join("cache"))
        .env("XDG_DATA_HOME", base.join("data"))
        .env("XDG_RUNTIME_DIR", base.join("runtime"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    crate::common::wait_for_socket(&socket_path, Duration::from_secs(10)).await?;
    crate::common::wait_for_socket(&ssh_socket_path, Duration::from_secs(10)).await?;

    Ok(BareAgent {
        child,
        socket_path,
        ssh_socket_path,
        _temp_dir: temp_dir,
    })
}

/// Invariant: both Unix sockets are created with mode 0600 in a 0700 dir.
#[tokio::test]
async fn test_socket_file_modes() -> Result<()> {
    let agent = spawn_bare_agent().await?;

    for sock in [&agent.socket_path, &agent.ssh_socket_path] {
        let mode = std::fs::metadata(sock)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{} must be 0600, got {:o}", sock.display(), mode);
    }
    let parent = agent.socket_path.parent().unwrap();
    let dir_mode = std::fs::metadata(parent)?.permissions().mode() & 0o777;
    assert_eq!(dir_mode, 0o700, "socket dir must be 0700, got {dir_mode:o}");

    Ok(())
}

/// Invariant: a length prefix above the 8 MiB cap must close the connection
/// without allocating or crashing the agent.
#[tokio::test]
async fn test_oversized_request_is_rejected() -> Result<()> {
    let agent = spawn_bare_agent().await?;

    let mut stream = tokio::net::UnixStream::connect(&agent.socket_path).await?;
    let oversized: u32 = 9 * 1024 * 1024;
    stream.write_all(&oversized.to_le_bytes()).await?;

    // Agent must drop the connection (EOF) rather than wait for 9 MiB.
    let mut buf = [0u8; 16];
    let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await??;
    assert_eq!(n, 0, "expected EOF after oversized length prefix, got {n} bytes");

    // And the agent itself must still be alive and serving new connections.
    let mut stream2 = tokio::net::UnixStream::connect(&agent.socket_path).await?;
    let garbage = [0xFFu8; 8];
    stream2.write_all(&(garbage.len() as u32).to_le_bytes()).await?;
    stream2.write_all(&garbage).await?;
    let n = tokio::time::timeout(Duration::from_secs(5), stream2.read(&mut buf)).await??;
    assert!(n > 0, "agent should answer (with an error) after surviving the oversized request");

    Ok(())
}

/// Invariant: undecodable request bytes get an error response, not a hang or
/// a crash.
#[tokio::test]
async fn test_garbage_request_gets_error_response() -> Result<()> {
    let agent = spawn_bare_agent().await?;

    let mut stream = tokio::net::UnixStream::connect(&agent.socket_path).await?;
    let garbage = [0xABu8; 32];
    stream.write_all(&(garbage.len() as u32).to_le_bytes()).await?;
    stream.write_all(&garbage).await?;

    let mut len_buf = [0u8; 4];
    tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut len_buf)).await??;
    let len = u32::from_le_bytes(len_buf) as usize;
    assert!(len > 0 && len < 64 * 1024, "implausible response length {len}");
    let mut body = vec![0u8; len];
    tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut body)).await??;

    let response: cosmic_bwarden_core::protocol::Response = postcard::from_bytes(&body)?;
    assert!(
        matches!(response, cosmic_bwarden_core::protocol::Response::Error { .. }),
        "expected Error response to garbage request, got {response:?}"
    );

    Ok(())
}
