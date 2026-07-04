use crate::common::setup_env;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::Action;
use std::path::PathBuf;
use std::process::Command;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_cli_socket_override() -> anyhow::Result<()> {
    let env = setup_env().await?;

    let socket = env.socket_path.clone();
    assert!(socket.exists());

    // 1. Try to connect with a different (non-existent) socket -> should fail
    let wrong_socket = env._temp_dir.path().join("wrong_socket");
    let client_wrong = AgentClient::new_with_socket(wrong_socket);
    let res = client_wrong.send(Action::GetConfig).await;
    assert!(res.is_err());

    // 2. Connect with the correct socket override -> should succeed
    let client_correct = AgentClient::new_with_socket(socket);
    let res = client_correct.send(Action::GetConfig).await;
    assert!(res.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_config_socket_override() -> anyhow::Result<()> {
    // We need to start a NEW agent with a config that specifies a socket_path
    let temp_dir = tempfile::tempdir()?;
    let config_path = temp_dir.path().join("config.json");
    let custom_socket = temp_dir.path().join("custom_socket");

    let config_json = serde_json::json!({
        "socket_path": custom_socket.to_string_lossy()
    });
    std::fs::write(&config_path, serde_json::to_string(&config_json)?)?;

    let mut agent_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    agent_path.pop();
    agent_path.pop();
    agent_path.push("target/debug/cosmic-bwarden-agent");

    let mut child = Command::new(&agent_path)
        .arg("--config")
        .arg(&config_path)
        .spawn()?;

    // Wait for agent to start
    let mut success = false;
    for _ in 0..20 {
        sleep(Duration::from_millis(200)).await;
        if custom_socket.exists() {
            success = true;
            break;
        }
    }

    let _ = child.kill();
    assert!(
        success,
        "Agent should have created the socket specified in the config file"
    );

    Ok(())
}

#[tokio::test]
async fn test_override_priority() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let config_path = temp_dir.path().join("config.json");
    let config_socket = temp_dir.path().join("config_socket");
    let cli_socket = temp_dir.path().join("cli_socket");

    let config_json = serde_json::json!({
        "socket_path": config_socket.to_string_lossy()
    });
    std::fs::write(&config_path, serde_json::to_string(&config_json)?)?;

    let mut agent_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    agent_path.pop();
    agent_path.pop();
    agent_path.push("target/debug/cosmic-bwarden-agent");

    let mut child = Command::new(&agent_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--socket")
        .arg(&cli_socket)
        .spawn()?;

    // Wait for agent to start
    let mut cli_success = false;
    for _ in 0..20 {
        sleep(Duration::from_millis(200)).await;
        if cli_socket.exists() {
            cli_success = true;
            break;
        }
    }

    assert!(
        cli_success,
        "Agent should have created the socket specified in CLI, overriding config"
    );
    assert!(
        !config_socket.exists(),
        "Agent should NOT have created the socket specified in config when CLI override is present"
    );

    let _ = child.kill();
    Ok(())
}

// Helper for AgentClient to use custom socket path in tests
// I need to add this method to AgentClient or make socket_path field public
// Let's modify AgentClient first.
