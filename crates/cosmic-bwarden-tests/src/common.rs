use anyhow::{Context, Result};
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};
use std::env;
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    GenericImage, ImageExt,
};
use tokio::time::sleep;

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

    pub fn cli_path(&self) -> PathBuf {
        let mut cli_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cli_path.pop();
        cli_path.pop();
        cli_path.push("target/debug/cosmic-bwarden-cli");
        cli_path
    }

    /// Shared environment for every CLI invocation (socket, profile, XDG
    /// redirects into the test's temp dirs).
    fn apply_cli_env(&self, cmd: &mut Command) {
        cmd.env("COSMIC_BWARDEN_PROFILE", &self.profile)
            .env("XDG_CONFIG_HOME", &self.config_home)
            .env("XDG_CACHE_HOME", &self.cache_home)
            .env("XDG_DATA_HOME", &self.data_home)
            .env("XDG_RUNTIME_DIR", &self.runtime_home);
    }

    pub fn cli_cmd(&self) -> Command {
        let mut cmd = Command::new(self.cli_path());
        cmd.arg("--socket")
            .arg(&self.socket_path)
            .arg("--config")
            .arg(&self.config_path);
        self.apply_cli_env(&mut cmd);
        cmd
    }

    /// Runs the CLI with `stdin_content` piped in as its terminal input.
    ///
    /// The CLI reads the master password with `rpassword`, which on unix opens
    /// `/dev/tty` directly — there is no stdin fallback, so a bare pipe cannot
    /// supply the password (and would hang on a developer machine that has a
    /// controlling terminal). util-linux `script -qefc` allocates a pty, makes
    /// it the child's controlling terminal, forwards stdin to it, and returns
    /// the child's exit code.
    ///
    /// Returns `(success, stdout, stderr)`.
    pub fn run_cli_with_tty(
        &self,
        args: &[&str],
        stdin_content: &str,
    ) -> Result<(bool, String, String)> {
        // Rebuild the same invocation `cli_cmd()` would produce, as one
        // command line for `script -c` (which runs it via /bin/sh).
        let mut argv: Vec<String> = Vec::with_capacity(args.len() + 5);
        argv.push(self.cli_path().to_string_lossy().into_owned());
        argv.push("--socket".to_string());
        argv.push(self.socket_path.to_string_lossy().into_owned());
        argv.push("--config".to_string());
        argv.push(self.config_path.to_string_lossy().into_owned());
        argv.extend(args.iter().map(|a| (*a).to_string()));
        let cmdline = argv
            .iter()
            .map(|a| shell_quote(a))
            .collect::<Vec<_>>()
            .join(" ");

        let mut cmd = Command::new("script");
        // `-E never` keeps the typed password out of the typescript/stdout even
        // before rpassword turns the slave's echo off.
        cmd.args(["-qE", "never", "-efc", &cmdline, "/dev/null"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        self.apply_cli_env(&mut cmd);

        let mut child = cmd
            .spawn()
            .context("failed to spawn `script` (util-linux) to run the CLI on a pty")?;
        let mut stdin = child.stdin.take().expect("script stdin");
        let stdout_pipe = child.stdout.take().expect("script stdout");
        let stderr_pipe = child.stderr.take().expect("script stderr");
        stdin.write_all(stdin_content.as_bytes())?;
        drop(stdin); // EOF on script's stdin: a hypothetical second prompt
                     // resolves with an empty line instead of blocking.

        // Drain the pipes on reader threads so the watchdog below can watch
        // only `try_wait` without risking a pipe-buffer deadlock.
        let out_reader = std::thread::spawn(move || std::io::read_to_string(stdout_pipe));
        let err_reader = std::thread::spawn(move || std::io::read_to_string(stderr_pipe));

        let deadline = Instant::now() + CLI_TTY_TIMEOUT;
        let status = loop {
            if let Some(status) = child
                .try_wait()
                .context("failed to wait for `script` (util-linux)")?
            {
                break status;
            }
            if Instant::now() >= deadline {
                // Kill `script`: its pipes close (reader threads finish) and
                // the pty master closes, which takes the CLI child down with
                // SIGHUP/EIO instead of leaving it blocked on a prompt.
                let _ = child.kill();
                let _ = child.wait();
                let stdout = out_reader
                    .join()
                    .map(|r| r.unwrap_or_default())
                    .unwrap_or_default();
                let stderr = err_reader
                    .join()
                    .map(|r| r.unwrap_or_default())
                    .unwrap_or_default();
                anyhow::bail!(
                    "CLI invocation timed out after {}s and was killed\nstdout:\n{}\nstderr:\n{}",
                    CLI_TTY_TIMEOUT.as_secs(),
                    stdout.trim(),
                    stderr.trim(),
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let stdout = out_reader
            .join()
            .map_err(|_| anyhow::anyhow!("stdout reader thread panicked"))?
            .context("failed to read `script` stdout")?;
        let stderr = err_reader
            .join()
            .map_err(|_| anyhow::anyhow!("stderr reader thread panicked"))?
            .context("failed to read `script` stderr")?;
        Ok((status.success(), stdout, stderr))
    }
}

/// Hard limit for one interactive CLI invocation under the pty. Register and
/// login finish in seconds; this only exists so a stuck prompt or a dead agent
/// fails the test with a diagnosis instead of hanging the whole suite.
const CLI_TTY_TIMEOUT: Duration = Duration::from_secs(60);

/// POSIX single-quote a string for `/bin/sh -c`; leaves plain tokens alone so
/// the command line stays readable in logs.
fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-/:=".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
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
            anyhow::bail!(
                "agent socket {} not created within {timeout:?}",
                path.display()
            );
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
