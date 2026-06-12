use anyhow::Result;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    ContainerAsync, GenericImage, ImageExt,
};
use tokio::time::sleep;

/// Generates a real SSH keypair on disk via `ssh-keygen` and returns
/// `(private_key_openssh_pem, public_key_line)`.
pub fn generate_ssh_keypair(
    key_type: &str,
    bits: Option<u32>,
    tmp_dir: &Path,
) -> Result<(String, String)> {
    let key_path = tmp_dir.join(format!("id_{key_type}"));

    let mut cmd = Command::new("ssh-keygen");
    cmd.arg("-t").arg(key_type);
    if let Some(bits) = bits {
        cmd.arg("-b").arg(bits.to_string());
    }
    cmd.arg("-f")
        .arg(&key_path)
        .arg("-N")
        .arg("")
        .arg("-C")
        .arg("e2e-test-key");

    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("ssh-keygen failed for key type {key_type}");
    }

    let private_key = std::fs::read_to_string(&key_path)?;
    let public_key = std::fs::read_to_string(key_path.with_extension("pub"))?;
    Ok((private_key, public_key.trim().to_string()))
}

/// Starts a real `sshd` container with `authorized_public_key` as the only
/// authorized key for `testuser`, password auth disabled. Returns the
/// container with port 2222 exposed once it accepts TCP connections.
pub async fn start_sshd_container(authorized_public_key: &str) -> Result<ContainerAsync<GenericImage>> {
    let image = GenericImage::new("linuxserver/openssh-server", "latest")
        .with_wait_for(WaitFor::seconds(8))
        .with_exposed_port(2222.tcp())
        .with_env_var("PUBLIC_KEY", authorized_public_key)
        .with_env_var("USER_NAME", "testuser")
        .with_env_var("PASSWORD_ACCESS", "false");

    let container = image.start().await?;
    let port = container.get_host_port_ipv4(2222).await?;

    let mut ready = false;
    for _ in 0..30 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            ready = true;
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }
    if !ready {
        anyhow::bail!("sshd container did not become ready on port {port}");
    }

    Ok(container)
}

/// Polls until `path` exists or `timeout` elapses.
pub async fn wait_for_socket(path: &Path, timeout: Duration) -> Result<()> {
    let start = std::time::Instant::now();
    while !path.exists() {
        if start.elapsed() > timeout {
            anyhow::bail!("socket {} did not appear within {:?}", path.display(), timeout);
        }
        sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}

/// Asserts that `path` has exactly the given permission bits (e.g. `0o600`
/// for the ssh-agent socket, `0o700` for its parent runtime directory).
pub fn assert_permissions(path: &Path, expected_mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path)?.permissions().mode() & 0o777;
    if mode != expected_mode {
        anyhow::bail!(
            "expected {} to have mode {:o}, got {:o}",
            path.display(),
            expected_mode,
            mode
        );
    }
    Ok(())
}

/// Runs `ssh-add -l` against the given agent socket.
pub fn run_ssh_add_list(sock: &Path) -> Result<Output> {
    Ok(Command::new("ssh-add")
        .arg("-l")
        .env("SSH_AUTH_SOCK", sock)
        .output()?)
}

/// Runs `remote_cmd` over a real SSH connection authenticated via the given
/// agent socket. Uses `BatchMode=yes` so the client never prompts and fails
/// fast if pubkey auth via the agent doesn't succeed.
pub fn run_ssh_command(sock: &Path, port: u16, user: &str, remote_cmd: &str) -> Result<Output> {
    Ok(Command::new("ssh")
        .args([
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=10",
            "-p",
            &port.to_string(),
            &format!("{user}@127.0.0.1"),
            remote_cmd,
        ])
        .env("SSH_AUTH_SOCK", sock)
        .output()?)
}

/// Asserts that the cosmic-bwarden ssh-agent socket either does (or does
/// not, per `expect_success`) allow a real SSH login against the given
/// `sshd` port.
pub fn assert_ssh_access(sock: &Path, port: u16, user: &str, expect_success: bool) -> Result<()> {
    let add_output = run_ssh_add_list(sock)?;
    let ssh_output = run_ssh_command(sock, port, user, "echo E2E_SSH_OK")?;
    let ssh_stdout = String::from_utf8_lossy(&ssh_output.stdout);
    let ssh_stderr = String::from_utf8_lossy(&ssh_output.stderr);

    if expect_success {
        if !add_output.status.success() {
            let add_stdout = String::from_utf8_lossy(&add_output.stdout);
            let add_stderr = String::from_utf8_lossy(&add_output.stderr);
            anyhow::bail!(
                "expected ssh-add -l to list an identity, got stdout={add_stdout:?} stderr={add_stderr:?}"
            );
        }
        if !ssh_output.status.success() || !ssh_stdout.contains("E2E_SSH_OK") {
            anyhow::bail!(
                "expected ssh command to succeed, got status={:?} stdout={ssh_stdout:?} stderr={ssh_stderr:?}",
                ssh_output.status
            );
        }
    } else {
        if add_output.status.success() {
            let add_stdout = String::from_utf8_lossy(&add_output.stdout);
            anyhow::bail!("expected ssh-add -l to report no identities, got stdout={add_stdout:?}");
        }
        if ssh_output.status.success() || ssh_stdout.contains("E2E_SSH_OK") {
            anyhow::bail!(
                "expected ssh command to fail, got status={:?} stdout={ssh_stdout:?} stderr={ssh_stderr:?}",
                ssh_output.status
            );
        }
    }

    Ok(())
}
