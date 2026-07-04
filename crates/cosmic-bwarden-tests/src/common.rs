use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};
use std::env;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    GenericImage, ImageExt,
};
use tokio::time::{sleep, Duration};

pub struct TestEnv {
    // Ensure testcontainers uses Podman if available
    pub _container: testcontainers::ContainerAsync<GenericImage>,
    pub agent_process: Option<std::process::Child>,
    pub vault_url: String,
    pub profile: String,
    pub _log_file: tempfile::NamedTempFile,
    pub log_path: PathBuf,
    pub _temp_dir: tempfile::TempDir,
    pub socket_path: PathBuf,
    pub ssh_socket_path: PathBuf,
    pub config_path: PathBuf,
    pub agent_path: PathBuf,
    pub config_home: PathBuf,
    pub cache_home: PathBuf,
    pub data_home: PathBuf,
    pub runtime_home: PathBuf,
}

impl TestEnv {
    pub fn start_agent(&self) -> Result<std::process::Child> {
        self.start_agent_with_env(&[])
    }

    /// Start the agent binary with additional environment variables.
    pub fn start_agent_with_env(&self, extra_env: &[(&str, &str)]) -> Result<std::process::Child> {
        let log_file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.log_path)?;

        let mut cmd = Command::new(&self.agent_path);
        cmd.arg("--socket")
            .arg(&self.socket_path)
            .arg("--ssh-socket")
            .arg(&self.ssh_socket_path)
            .arg("--config")
            .arg(&self.config_path)
            .env("COSMIC_BWARDEN_PROFILE", &self.profile)
            .env("XDG_CONFIG_HOME", &self.config_home)
            .env("XDG_CACHE_HOME", &self.cache_home)
            .env("XDG_DATA_HOME", &self.data_home)
            .env("XDG_RUNTIME_DIR", &self.runtime_home)
            .env("RUST_LOG", "debug")
            .stdout(log_file.try_clone()?)
            .stderr(log_file.try_clone()?);

        for (key, val) in extra_env {
            cmd.env(key, val);
        }

        Ok(cmd.spawn()?)
    }

    pub fn cli_cmd(&self) -> Command {
        let mut cli_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cli_path.pop();
        cli_path.pop();
        cli_path.push("target/debug/cosmic-bwarden-cli");

        let mut cmd = Command::new(cli_path);
        cmd.arg("--socket")
            .arg(&self.socket_path)
            .arg("--config")
            .arg(&self.config_path)
            .env("COSMIC_BWARDEN_PROFILE", &self.profile)
            .env("XDG_CONFIG_HOME", &self.config_home)
            .env("XDG_CACHE_HOME", &self.cache_home)
            .env("XDG_DATA_HOME", &self.data_home)
            .env("XDG_RUNTIME_DIR", &self.runtime_home);
        cmd
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        if let Some(mut child) = self.agent_process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Like `setup_env` but does NOT start the agent process.
/// The caller is responsible for starting the agent (possibly with a custom
/// binary or extra environment variables) and assigning it to `env.agent_process`.
pub async fn setup_env_no_agent() -> Result<TestEnv> {
    if env::var_os("DOCKER_HOST").is_none() {
        let mut candidates = vec!["/run/podman/podman.sock".to_string()];
        if let Ok(runtime_dir) = env::var("XDG_RUNTIME_DIR") {
            candidates.push(format!("{}/podman/podman.sock", runtime_dir));
        }
        for sock in &candidates {
            if Path::new(sock.as_str()).exists() {
                env::set_var("DOCKER_HOST", format!("unix://{}", sock));
                break;
            }
        }
    }
    let node = GenericImage::new("vaultwarden/server", "latest")
        .with_wait_for(WaitFor::seconds(5))
        .with_exposed_port(80.tcp())
        .with_env_var("SIGNUPS_ALLOWED", "true")
        .with_env_var("I_REALLY_WANT_VOLATILE_STORAGE", "true")
        .with_env_var(
            "EXPERIMENTAL_CLIENT_FEATURE_FLAGS",
            "ssh-key-vault-item,ssh-agent",
        );

    let container = node.start().await?;
    let host_port = container.get_host_port_ipv4(80).await?;
    let vault_url = format!("http://localhost:{}", host_port);

    // Poll Vaultwarden's /alive endpoint instead of trusting the fixed
    // WaitFor::seconds(5) above — under load 5 s is not always enough, and a
    // half-started server produced intermittent test failures (see
    // docs/review/00_ground_truth.md F9).
    let http = reqwest::Client::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        match http.get(format!("{vault_url}/alive")).send().await {
            Ok(res) if res.status().is_success() => break,
            _ if tokio::time::Instant::now() >= deadline => {
                anyhow::bail!("Vaultwarden not ready after 30s at {vault_url}");
            }
            _ => sleep(Duration::from_millis(250)).await,
        }
    }

    let profile = format!("test-{}", uuid::Uuid::new_v4());
    let temp_dir = tempfile::tempdir()?;
    let temp_path = temp_dir.path();

    let config_home = temp_path.join("config");
    let cache_home = temp_path.join("cache");
    let data_home = temp_path.join("data");
    let runtime_home = temp_path.join("runtime");

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&config_home)?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&cache_home)?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&data_home)?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&runtime_home)?;

    let mut agent_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    agent_path.pop();
    agent_path.pop();
    agent_path.push("target/debug/cosmic-bwarden-agent");

    let mut log_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    log_path.pop();
    log_path.pop();
    log_path.push("agent_test.log");
    let _log_file_handle = std::fs::File::create(&log_path)?;

    let socket_path = runtime_home.join("socket");
    let ssh_socket_path = runtime_home.join("ssh-agent-socket");
    let config_path = config_home.join("config.json");

    Ok(TestEnv {
        _container: container,
        agent_process: None,
        vault_url,
        profile,
        _log_file: tempfile::NamedTempFile::new()?,
        log_path,
        _temp_dir: temp_dir,
        socket_path,
        ssh_socket_path,
        config_path,
        agent_path,
        config_home,
        cache_home,
        data_home,
        runtime_home,
    })
}

pub async fn setup_env() -> Result<TestEnv> {
    let mut env = setup_env_no_agent().await?;
    let agent_process = env.start_agent()?;
    env.agent_process = Some(agent_process);
    // Poll for the socket instead of a fixed 1 s sleep — a slow-starting agent
    // made tests flaky, and a fast one wastes most of the second.
    wait_for_socket(&env.socket_path, Duration::from_secs(10)).await?;
    Ok(env)
}

/// Wait until `path` exists (agent has bound its socket), polling every 50 ms.
pub async fn wait_for_socket(path: &Path, timeout: Duration) -> Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    while !path.exists() {
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("agent socket {} not created within {timeout:?}", path.display());
        }
        sleep(Duration::from_millis(50)).await;
    }
    Ok(())
}

pub async fn register_user(url: &str, email: &str, password: &str) -> Result<()> {
    let client = reqwest::Client::new();

    use cosmic_bwarden_core::api::KdfType;
    use cosmic_bwarden_core::identity::Identity;
    use cosmic_bwarden_core::locked;

    let password_locked = locked::Password::from_string(password);

    // Vaultwarden default for new users is PBKDF2 with 600,000 iterations
    let kdf_type = KdfType::Pbkdf2;
    let kdf_iterations = 600_000;
    let identity = Identity::new(
        email,
        &password_locked,
        kdf_type,
        kdf_iterations,
        None,
        None,
    )?;

    use cosmic_bwarden_core::cipherstring::CipherString;

    let protected_key = CipherString::encrypt_symmetric(&identity.keys, identity.keys.data())?;

    let register_payload = serde_json::json!({
        "email": email,
        "masterPasswordHash": cosmic_bwarden_core::base64::encode(identity.master_password_hash.hash()),
        "masterPasswordHint": "",
        "key": protected_key.to_string(),
        "name": "Test User",
        "kdf": 0, // PBKDF2
        "kdfIterations": kdf_iterations
    });

    // Try /identity/accounts/register first as it's more modern in Vaultwarden
    let res = client
        .post(format!("{}/identity/accounts/register", url))
        .json(&register_payload)
        .send()
        .await?;

    if !res.status().is_success() {
        // Try legacy endpoint if modern one fails
        let res = client
            .post(format!("{}/api/accounts/register", url))
            .json(&register_payload)
            .send()
            .await?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await?;
            anyhow::bail!("Registration failed: {} - {}", status, text);
        }
    }

    Ok(())
}

pub async fn lock_unlock_cycle(client: &AgentClient, password: &str) -> Result<()> {
    client.send(Action::Lock).await?;
    let res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    if let Response::Error { message } = res {
        if !message.contains("locked") {
            anyhow::bail!("Expected locked error, got: {}", message);
        }
    } else {
        anyhow::bail!("Expected error response when locked, got: {:?}", res);
    }

    client
        .send(Action::Unlock {
            password: password.to_string(),
        })
        .await?;
    Ok(())
}

pub async fn logout_login_cycle(
    client: &AgentClient,
    email: &str,
    password: &str,
    server_url: &str,
) -> Result<()> {
    client.send(Action::Logout).await?;
    let res = client
        .send(Action::GetEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    if let Response::Error { message } = res {
        if !message.contains("no API session token") && !message.contains("locked") {
            anyhow::bail!("Expected session-token or locked error, got: {}", message);
        }
    } else {
        anyhow::bail!("Expected error response when logged out, got: {:?}", res);
    }

    client
        .send(Action::Login {
            email: email.to_string(),
            password: password.to_string(),
            server_url: Some(server_url.to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;
    Ok(())
}
