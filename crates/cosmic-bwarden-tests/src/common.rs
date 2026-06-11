use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};
use std::env;
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
    // This is set once per process
    // NOTE: Podman socket path may vary; adjust as needed
    // Setting env var before any container creation
    // Allows CI environments without Docker daemon
    // to run tests using Podman.
    pub _container: testcontainers::ContainerAsync<GenericImage>,
    pub agent_process: Option<std::process::Child>,
    pub vault_url: String,
    pub profile: String,
    pub _log_file: tempfile::NamedTempFile,
    pub log_path: PathBuf,
    pub _temp_dir: tempfile::TempDir,
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        if let Some(mut child) = self.agent_process.take() {
            let _ = child.kill();
        }
    }
}

pub async fn setup_env() -> Result<TestEnv> {
    // Prefer Podman if its socket is available. This avoids requiring the Docker daemon.
    // Testcontainers reads the DOCKER_HOST environment variable.
    if env::var_os("DOCKER_HOST").is_none() {
        let podman_socket = "/run/podman/podman.sock";
        if Path::new(podman_socket).exists() {
            env::set_var("DOCKER_HOST", format!("unix://{}", podman_socket));
        }
    }
    let node = GenericImage::new("vaultwarden/server", "latest")
        .with_wait_for(WaitFor::seconds(5))
        .with_exposed_port(80.tcp())
        .with_env_var("SIGNUPS_ALLOWED", "true")
        .with_env_var("I_REALLY_WANT_VOLATILE_STORAGE", "true")
        .with_env_var("EXPERIMENTAL_CLIENT_FEATURE_FLAGS", "ssh-key-vault-item,ssh-agent");

    let container = node.start().await?;
    let host_port = container.get_host_port_ipv4(80).await?;
    let vault_url = format!("http://localhost:{}", host_port);

    let profile = format!("test-{}", uuid::Uuid::new_v4());
    let temp_dir = tempfile::tempdir()?;
    let temp_path = temp_dir.path();

    let config_home = temp_path.join("config");
    let cache_home = temp_path.join("cache");
    let data_home = temp_path.join("data");

    std::fs::create_dir_all(&config_home)?;
    std::fs::create_dir_all(&cache_home)?;
    std::fs::create_dir_all(&data_home)?;

    // Set for current process too
    std::env::set_var("XDG_CONFIG_HOME", &config_home);
    std::env::set_var("XDG_CACHE_HOME", &cache_home);
    std::env::set_var("XDG_DATA_HOME", &data_home);

    let mut agent_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    agent_path.pop();
    agent_path.pop();
    agent_path.push("target/debug/cosmic-bwarden-agent");

    let mut log_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    log_path.pop();
    log_path.pop();
    log_path.push("agent_test.log");
    let log_file = std::fs::File::create(&log_path)?;

    let agent_process = Command::new(&agent_path)
        .env("COSMIC_BWARDEN_PROFILE", &profile)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("XDG_DATA_HOME", &data_home)
        .env("RUST_LOG", "debug")
        .stdout(log_file.try_clone()?)
        .stderr(log_file.try_clone()?)
        .spawn()?;

    // Wait for agent to start and create socket
    sleep(Duration::from_millis(1000)).await;

    Ok(TestEnv {
        _container: container,
        agent_process: Some(agent_process),
        vault_url,
        profile,
        _log_file: tempfile::NamedTempFile::new()?, // unused but keep for compat
        log_path,
        _temp_dir: temp_dir,
    })
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
        if !message.contains("not logged in") && !message.contains("locked") {
            anyhow::bail!("Expected not logged in error, got: {}", message);
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
