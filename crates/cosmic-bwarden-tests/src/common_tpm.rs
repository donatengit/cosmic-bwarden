//! Test infrastructure for TPM E2E tests.
//!
//! Provides `TpmTestEnv`, which extends the normal `TestEnv` with:
//!   - A `swtpm` software-TPM emulator process (via unix-socket).
//!   - The `cosmic-bwarden-agent-tpm` binary (compiled with `--features tpm`).
//!   - `TSS2_TCTI` set to point at the emulator socket.
//!
//! Tests call `TpmTestEnv::setup().await` and receive `Ok(Some(env))` when all
//! prerequisites are met, or `Ok(None)` when any prerequisite is absent —
//! in which case the calling test should return `Ok(())` immediately (skip).
//!
//! Prerequisites:
//!   - `swtpm` binary in PATH  (package: `swtpm` on Debian/Fedora/Arch)
//!   - `swtpm_setup` binary in PATH  (same package)
//!   - `target/debug/cosmic-bwarden-agent-tpm` built with
//!     `cargo build -p cosmic-bwarden-agent --features tpm`
//!   - Vaultwarden container accessible via Docker / Podman socket

use crate::common::TestEnv;
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use tokio::time::{sleep, Duration};

// ── swtpm process guard ────────────────────────────────────────────────────

pub struct SwtpmGuard {
    process: Child,
    pub server_socket: PathBuf,
    _state_dir: tempfile::TempDir,
}

impl Drop for SwtpmGuard {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// Returns the `TSS2_TCTI` connection string for the given server socket.
pub fn swtpm_tcti(server_socket: &Path) -> String {
    format!("swtpm:path={}", server_socket.display())
}

/// Start a software TPM2 emulator.  Returns `None` if `swtpm` / `swtpm_setup`
/// are not found in PATH, printing a notice so CI output is clear.
pub fn start_swtpm() -> Option<SwtpmGuard> {
    // Both binaries must be present
    if which("swtpm").is_none() {
        eprintln!("[tpm-smoke] swtpm not found in PATH — TPM tests will be skipped");
        return None;
    }
    if which("swtpm_setup").is_none() {
        eprintln!("[tpm-smoke] swtpm_setup not found in PATH — TPM tests will be skipped");
        return None;
    }

    let state_dir = tempfile::TempDir::new().ok()?;
    let state_path = state_dir.path();

    // Initialise a fresh TPM2 state (EK only; full provisioning not needed for seal/unseal).
    let init = Command::new("swtpm_setup")
        .args([
            "--tpmstate",
            state_path.to_str()?,
            "--tpm2",
            "--createek",
            "--not-overwrite",
        ])
        .output()
        .ok()?;

    if !init.status.success() {
        eprintln!(
            "[tpm-smoke] swtpm_setup failed ({}): {}",
            init.status,
            String::from_utf8_lossy(&init.stderr)
        );
        return None;
    }

    let ctrl_socket = state_path.join("swtpm-ctrl.sock");
    let server_socket = state_path.join("swtpm-server.sock");

    let process = Command::new("swtpm")
        .args([
            "socket",
            "--tpmstate",
            &format!("dir={}", state_path.display()),
            "--ctrl",
            &format!("type=unixio,path={}", ctrl_socket.display()),
            "--server",
            &format!("type=unixio,path={}", server_socket.display()),
            "--tpm2",
            "--flags",
            "not-need-init,startup-clear",
            "--log",
            "level=0",
        ])
        .spawn()
        .ok()?;

    Some(SwtpmGuard {
        process,
        server_socket,
        _state_dir: state_dir,
    })
}

fn which(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths).find_map(|dir| {
            let full = dir.join(name);
            full.is_file().then_some(full)
        })
    })
}

// ── TPM-enabled agent binary ───────────────────────────────────────────────

/// Path to the TPM-enabled agent binary.
/// Can be overridden with `COSMIC_BWARDEN_TPM_AGENT` env var.
pub fn tpm_agent_path() -> Option<PathBuf> {
    // Allow CI / dev to point at a custom build
    if let Ok(p) = env::var("COSMIC_BWARDEN_TPM_AGENT") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
        eprintln!(
            "[tpm-smoke] COSMIC_BWARDEN_TPM_AGENT={} not found",
            pb.display()
        );
        return None;
    }

    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // cosmic-bwarden-tests
    p.pop(); // crates
    p.push("target/debug/cosmic-bwarden-agent-tpm");

    if p.exists() {
        Some(p)
    } else {
        eprintln!(
            "[tpm-smoke] {} not found — build with: \
             cargo build -p cosmic-bwarden-agent --features tpm\n\
             (produces target/debug/cosmic-bwarden-agent-tpm)",
            p.display()
        );
        None
    }
}

// ── Combined environment ───────────────────────────────────────────────────

pub struct TpmTestEnv {
    pub inner: TestEnv,
    _swtpm: SwtpmGuard,
    /// TSS2_TCTI connection string for this test's swtpm instance. The agent
    /// process uses it; tests that talk to the TPM directly (PCR mutation)
    /// or restart the agent reuse it.
    tcti: String,
}

impl TpmTestEnv {
    /// Build a complete TPM test environment.
    ///
    /// Returns `Ok(None)` if any prerequisite (swtpm, TPM agent binary) is
    /// missing.  The caller should treat `None` as a deliberate skip.
    pub async fn setup() -> Result<Option<Self>> {
        // Check swtpm first (cheap) before spinning up the container.
        let swtpm = match start_swtpm() {
            Some(s) => s,
            None => return Ok(None),
        };

        let agent_path = match tpm_agent_path() {
            Some(p) => p,
            None => return Ok(None),
        };

        // Give swtpm a moment to bind its sockets.
        sleep(Duration::from_millis(300)).await;

        let tcti = swtpm_tcti(&swtpm.server_socket);

        // Bring up Vaultwarden container + XDG dirs (reuse common helper but
        // swap agent binary and inject TSS2_TCTI before the agent starts).
        let mut env = crate::common::setup_env_no_agent().await?;
        env.agent_path = agent_path;

        // Restart agent with the TPM binary and TCTI set.
        let process = env.start_agent_with_env(&[("TSS2_TCTI", tcti.as_str())])?;
        env.agent_process = Some(process);

        // Wait for the agent socket to appear.
        sleep(Duration::from_millis(1000)).await;

        Ok(Some(TpmTestEnv {
            inner: env,
            _swtpm: swtpm,
            tcti,
        }))
    }

    /// Convenience: create a client connected to the TPM agent socket.
    pub fn client(&self) -> AgentClient {
        AgentClient::new_with_socket(self.inner.socket_path.clone())
    }

    /// Vault URL (Vaultwarden container).
    pub fn vault_url(&self) -> &str {
        &self.inner.vault_url
    }

    /// Kill the current agent process and start a fresh one with the same
    /// binary, environment (incl. TSS2_TCTI), and socket path. Simulates an
    /// agent restart — used to exercise startup-time TPM state detection.
    pub fn restart_agent(&mut self) -> Result<()> {
        if let Some(mut child) = self.inner.agent_process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // The old socket file dies with the process; give the kernel a beat
        // so the new agent can bind cleanly.
        let tcti = self.tcti.clone();
        let process = self
            .inner
            .start_agent_with_env(&[("TSS2_TCTI", tcti.as_str())])?;
        self.inner.agent_process = Some(process);
        Ok(())
    }

    /// Extend PCR 7 with a marker digest using both SHA-1 and SHA-256 banks,
    /// simulating a firmware/BIOS update changing the machine state. Providing
    /// both banks is deliberate: PCR_Extend ignores digests for banks not
    /// allocated to the PCR, so this works whatever swtpm's bank config is —
    /// while a missing allocated bank would fail the extend.
    #[cfg(feature = "tpm-smoke")]
    pub fn extend_pcr7(&self) -> Result<()> {
        use tss_esapi::handles::PcrHandle;
        use tss_esapi::interface_types::algorithm::HashingAlgorithm;
        use tss_esapi::structures::{Digest, DigestValues};
        use tss_esapi::TctiNameConf;

        let tcti: TctiNameConf = self
            .tcti
            .parse()
            .map_err(|e| anyhow::anyhow!("failed to parse TCTI {}: {}", self.tcti, e))?;
        let mut ctx = tss_esapi::Context::new(tcti)
            .map_err(|e| anyhow::anyhow!("failed to open TPM context: {}", e))?;

        let mut vals = DigestValues::new();
        vals.set(
            HashingAlgorithm::Sha256,
            Digest::try_from(vec![0xB1; 32])
                .map_err(|e| anyhow::anyhow!("bad sha256 digest: {}", e))?,
        );
        vals.set(
            HashingAlgorithm::Sha1,
            Digest::try_from(vec![0xB1; 20])
                .map_err(|e| anyhow::anyhow!("bad sha1 digest: {}", e))?,
        );
        ctx.pcr_extend(PcrHandle::Pcr7, vals)
            .map_err(|e| anyhow::anyhow!("PCR 7 extend failed: {}", e))?;
        Ok(())
    }
}

// ── Macro for skipping when prerequisites are absent ──────────────────────

/// Early-return `Ok(())` when the TPM environment could not be set up.
/// Usage:
/// ```rust
/// let env = tpm_skip_if_unavailable!(TpmTestEnv::setup().await?);
/// ```
#[macro_export]
macro_rules! tpm_skip_if_unavailable {
    ($expr:expr) => {
        match $expr {
            Some(e) => e,
            None => {
                eprintln!("[tpm-smoke] prerequisites missing — test skipped");
                return Ok(());
            }
        }
    };
}
