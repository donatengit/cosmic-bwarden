use crate::common::setup_env;
use cosmarden_core::agent_client::AgentClient;
use cosmarden_core::protocol::Action;
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::time::{sleep, Duration};

/// State root for a spawn in this module: `config/cache/data/runtime` at 0700
/// under a fresh temp dir.
fn state_root() -> anyhow::Result<tempfile::TempDir> {
    let temp_dir = tempfile::tempdir()?;
    for sub in ["config", "cache", "data", "runtime"] {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(temp_dir.path().join(sub))?;
    }
    Ok(temp_dir)
}

/// Build an agent `Command` whose entire environment is set explicitly.
///
/// These tests used to spawn with no environment at all, which meant the child
/// *inherited* `COSMARDEN_PROFILE` from this process — and ~59 sibling
/// tests set that var process-globally without ever restoring it. Depending on
/// test ordering the agent therefore ran either under some other test's UUID
/// profile or, when nothing had set it yet, under the **live**
/// `cosmarden` profile, writing into the developer's real
/// `~/.cache` / `~/.local/share` / `/run/user/$UID`.
///
/// `env_clear` is what makes the spawn independent of that ordering. Every var
/// the agent needs is then re-added explicitly. `HOME` is not optional: the
/// `directories` crate falls back to the passwd entry when `$HOME` is unset,
/// so a missing XDG var would still resolve into the real home rather than
/// failing loudly.
fn isolated_agent_cmd(agent_path: &Path, base: &Path) -> Command {
    let mut cmd = Command::new(agent_path);
    cmd.env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", base)
        .env(
            "COSMARDEN_PROFILE",
            format!("test-{}", uuid::Uuid::new_v4()),
        )
        .env("XDG_CONFIG_HOME", base.join("config"))
        .env("XDG_CACHE_HOME", base.join("cache"))
        .env("XDG_DATA_HOME", base.join("data"))
        .env("XDG_RUNTIME_DIR", base.join("runtime"));
    cmd
}

fn agent_binary() -> PathBuf {
    let mut agent_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    agent_path.pop();
    agent_path.pop();
    agent_path.push("target/debug/cosmarden-agent");
    agent_path
}

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
    let temp_dir = state_root()?;
    let config_path = temp_dir.path().join("config.json");
    let custom_socket = temp_dir.path().join("custom_socket");

    let config_json = serde_json::json!({
        "socket_path": custom_socket.to_string_lossy()
    });
    std::fs::write(&config_path, serde_json::to_string(&config_json)?)?;

    let mut child = isolated_agent_cmd(&agent_binary(), temp_dir.path())
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
    let _ = child.wait();
    assert!(
        success,
        "Agent should have created the socket specified in the config file"
    );

    Ok(())
}

#[tokio::test]
async fn test_override_priority() -> anyhow::Result<()> {
    let temp_dir = state_root()?;
    let config_path = temp_dir.path().join("config.json");
    let config_socket = temp_dir.path().join("config_socket");
    let cli_socket = temp_dir.path().join("cli_socket");

    let config_json = serde_json::json!({
        "socket_path": config_socket.to_string_lossy()
    });
    std::fs::write(&config_path, serde_json::to_string(&config_json)?)?;

    let mut child = isolated_agent_cmd(&agent_binary(), temp_dir.path())
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
    let _ = child.wait();
    Ok(())
}

/// Pins the invariant that an agent spawned by this module writes nothing into
/// the developer's real `~/.config`, `~/.cache`, `~/.local/share`,
/// `/run/user/$UID`, or `$TMPDIR`.
///
/// Deliberately self-contained rather than an assertion bolted onto the other
/// tests: it spawns the agent itself, so it holds regardless of test ordering.
/// That matters because the bug it guards against was ordering-dependent — the
/// old spawns inherited `COSMARDEN_PROFILE` from this process, so which
/// profile leaked depended on which sibling test had run first.
///
/// This test only *reads* the real home. It removes nothing.
#[tokio::test]
async fn agent_spawn_writes_nothing_to_the_real_home() -> anyhow::Result<()> {
    let before = crate::state_guard::RealHomeSnapshot::take();

    let temp_dir = state_root()?;
    let socket = temp_dir.path().join("runtime/socket");
    let mut child = isolated_agent_cmd(&agent_binary(), temp_dir.path())
        .arg("--socket")
        .arg(&socket)
        .arg("--config")
        .arg(temp_dir.path().join("config/config.json"))
        .spawn()?;

    // Wait for the agent to get far enough to have called `dirs::make_all()`
    // — that is the call that would create the profile dirs.
    let mut started = false;
    for _ in 0..40 {
        sleep(Duration::from_millis(250)).await;
        if socket.exists() {
            started = true;
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    assert!(started, "agent did not bind its socket at {socket:?}");
    before.assert_nothing_added("an isolated agent spawn");

    // The dirs must exist somewhere — under the tempdir, proving the agent did
    // run make_all() and the assertion above was not vacuous.
    for sub in ["cache", "data", "runtime"] {
        let dir = temp_dir.path().join(sub);
        let has_profile_dir = std::fs::read_dir(&dir)?.flatten().any(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("cosmarden-test-")
        });
        assert!(
            has_profile_dir,
            "expected a redirected profile dir under {}; if this fails the \
             leak assertion above proves nothing",
            dir.display()
        );
    }

    Ok(())
}
