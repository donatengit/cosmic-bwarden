//! CPU and memory caps for the E2E suite's containers.
//!
//! Without these, Vaultwarden and the openssh-server image run uncapped and
//! compete with the developer's desktop for all cores — the suite takes ~23
//! minutes, so that is 23 minutes of an unresponsive machine.
//!
//! testcontainers 0.23 has no create-time resource API (`ImageExt` exposes
//! `with_ulimit`, `with_cgroupns_mode` and `with_shm_size`, but nothing for
//! CPU or memory), so the caps are applied immediately after `start()` through
//! bollard's `update_container` — bollard is already a dependency, used by
//! `common::cleanup_stale_containers`.
//!
//! **Use `cpu_quota`/`cpu_period`, not `nano_cpus`.** Podman's Docker-compat
//! API accepts `NanoCpus` on the update endpoint and returns 200 while
//! silently ignoring it: the container's `cpu.max` stays `max`, and
//! `HostConfig.NanoCpus` reads back as 0. `CpuQuota` + `CpuPeriod` map
//! straight onto cgroup v2 `cpu.max` and do take effect. Verified on podman
//! 6.1.1.
//!
//! **Why this is not driven by the `just` variables.** `just` puts the test
//! command inside a systemd scope (`packaging/run-limited.sh`), which caps
//! cargo, rustc, the test binary and the agents it spawns. Containers escape
//! that scope: podman places each one in `user@<uid>.service/user.slice/
//! libpod-<id>.scope`, a sibling of the test scope rather than a child, so it
//! inherits nothing from it. The values below are therefore fixed constants
//! rather than a knob — passing a `just` variable in would mean adding an
//! environment variable, and these caps are generous enough that no run has
//! needed to change them.

/// cgroup v2 scheduling period. Quota is expressed against this: one full core
/// is `quota == period`.
const CPU_PERIOD_US: i64 = 100_000;

/// Cores each test container may use. Deliberately a small fraction of a
/// developer machine — Vaultwarden with volatile storage is idle most of the
/// test run, and the suite is serialized (`--test-threads=1`), so at most two
/// containers are live at once. Kept in step with `CONTAINER_CPUS` in
/// `tools/run_vaultwarden.sh` and `tests/browser-extension/run-chrome-e2e.sh`.
const CONTAINER_CPUS: f64 = 2.0;

/// Memory cap per container. Vaultwarden's resident set is well under 100 MB,
/// so this is ~10x headroom: it protects the host without risking an OOM kill
/// that would surface as a confusing test failure.
const CONTAINER_MEMORY_MB: i64 = 1024;

/// The caps applied to every test container.
pub fn limits() -> (f64, i64) {
    (CONTAINER_CPUS, CONTAINER_MEMORY_MB)
}

/// Cap one running container's CPU and memory.
///
/// Best-effort by design: a runtime that does not implement the update
/// endpoint must not fail the suite, since the caps are a courtesy to the
/// developer's desktop and not something any test asserts. Failures are
/// reported to stderr rather than swallowed — a silently uncapped container is
/// exactly the situation this module exists to avoid, and the developer should
/// know why their machine is busy.
pub async fn apply(container_id: &str, label: &str) {
    let (cpus, mem_mb) = limits();

    let Ok(docker) = bollard::Docker::connect_with_defaults() else {
        eprintln!("[container-limits] no container runtime client; {label} runs uncapped");
        return;
    };

    #[allow(clippy::cast_possible_truncation)]
    let cpu_quota = (cpus * CPU_PERIOD_US as f64).round() as i64;

    let options = bollard::container::UpdateContainerOptions::<String> {
        cpu_quota: Some(cpu_quota),
        cpu_period: Some(CPU_PERIOD_US),
        memory: Some(mem_mb * 1024 * 1024),
        ..Default::default()
    };

    match docker.update_container(container_id, options).await {
        Ok(_) => eprintln!("[container-limits] {label}: {cpus} cpu, {mem_mb} MB"),
        Err(e) => eprintln!(
            "[container-limits] could not cap {label} ({e}); it runs uncapped. \
             Adjust the constants in this file if that is a problem."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers::{core::WaitFor, runners::AsyncRunner, GenericImage, ImageExt};

    /// The cap must actually reach the container's cgroup — not merely return
    /// `Ok`. This is not a theoretical concern: podman's Docker-compat API
    /// accepts `NanoCpus` on this endpoint, answers 200, and silently ignores
    /// it (`cpu.max` stays `max`). Only `CpuQuota`/`CpuPeriod` take effect, and
    /// nothing but a read-back proves which of the two the code is sending.
    ///
    /// Reads the limit back through the same API rather than racing a
    /// `podman inspect` from a shell: the container lives only a few seconds.
    #[tokio::test]
    async fn cpu_and_memory_caps_reach_the_cgroup() -> anyhow::Result<()> {
        if std::env::var_os("DOCKER_HOST").is_none() {
            let sock = format!(
                "/run/user/{}/podman/podman.sock",
                std::fs::metadata("/proc/self")
                    .map(|m| {
                        use std::os::unix::fs::MetadataExt as _;
                        m.uid()
                    })
                    .unwrap_or(0)
            );
            if std::path::Path::new(&sock).exists() {
                std::env::set_var("DOCKER_HOST", format!("unix://{sock}"));
            }
        }

        // Reuses the image the suite already requires rather than pulling a
        // third one just for this check: any running container proves the cap
        // reached the cgroup, and adding an image would mean another pull on a
        // fresh machine or CI runner. `WaitFor::Nothing` because readiness does
        // not matter here — the container only has to exist.
        let container = GenericImage::new("vaultwarden/server", "latest")
            .with_wait_for(WaitFor::Nothing)
            .with_env_var("I_REALLY_WANT_VOLATILE_STORAGE", "true")
            .with_label("com.enikeev.cosmic-bwarden.e2e", "true")
            .start()
            .await?;

        apply(container.id(), "limits-selftest").await;

        let docker = bollard::Docker::connect_with_defaults()?;
        let info = docker.inspect_container(container.id(), None).await?;
        let host = info.host_config.expect("host config");

        let (cpus, mem_mb) = limits();
        #[allow(clippy::cast_possible_truncation)]
        let want_quota = (cpus * CPU_PERIOD_US as f64).round() as i64;

        assert_eq!(
            host.cpu_quota,
            Some(want_quota),
            "CpuQuota did not reach the container — if this is None/0 the \
             runtime accepted the request and ignored it (the NanoCpus trap)"
        );
        assert_eq!(
            host.cpu_period,
            Some(CPU_PERIOD_US),
            "CpuPeriod not applied"
        );
        assert_eq!(
            host.memory,
            Some(mem_mb * 1024 * 1024),
            "Memory limit not applied"
        );
        Ok(())
    }
}
