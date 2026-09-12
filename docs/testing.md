# Testing cosmic-bwarden

Tests run in complexity order: fast isolated unit tests first (encryption,
serialization, MVU state transitions), then the E2E suite against a real agent
and a Vaultwarden container.

`just test` runs both, in that order.


## Running the tests

Run every suite through `just`. The recipes carry the container-socket setup,
the binary rebuild the E2E harness depends on, and the resource caps described
below; invoking `cargo test` directly skips all of it.

| Recipe | Covers |
|---|---|
| `just test` | The whole Rust suite: unit tests for every crate, then the complete E2E crate |
| `just test-all` | `just test` plus the offline JS suites (extension unit tests, release-pipeline tests) |
| `just test-unit` | Unit tests only (core, ui, agent, cli) |
| `just test-unit-tpm` | The TPM-gated agent unit tests, which a plain `cargo test` cannot see |
| `just test-e2e` | The complete E2E crate |
| `just test-agent` / `test-cli` / `test-ui` | Focused subsets for iterating on one area — **not** a partition of the suite |
| `just test-extension-unit` | Extension JS unit tests (vitest) |
| `just test-extension-e2e` | Extension Playwright E2E, mocked agent |
| `just test-extension-e2e-full` | Extension E2E in Firefox with a real agent and Vaultwarden; needs a compositor |
| `just test-extension-e2e-chrome` | Same in Chrome (headless-compatible) |
| `just test-ext-release` | Release-pipeline logic, offline |
| `just test-extension-e2e-debug` | Playwright's interactive UI runner, for debugging a failing spec |
| `just test-extension-setup` | Installs the extension's npm dependencies (a dependency of the recipes above; rarely run directly) |

The subset recipes exist because they are quick. They do not add up to the full
suite: an earlier filter list left 41 of 90 E2E tests unrun, which is why
`just test-e2e` takes no filters.

## Resource limits

A full run takes over twenty minutes, and a release build saturates the same
cores for as long as it lasts. Without limits both monopolise the machine, so
the build and test recipes constrain them. There are two layers, because one
mechanism cannot reach both halves.

### Layer 1 — the systemd scope, for everything `just` starts

Each build and test recipe runs its command through
`packaging/run-limited.sh`, which wraps it in a transient systemd scope. This
covers cargo, rustc, the test binaries, and the agent processes the suite
spawns — the bulk of the CPU a run uses.

One `just` variable pair controls it, and builds and test runs share it:

| Variable | Default | Meaning |
|---|---|---|
| `test_cpus` | `6` | Cores a run may use. `0` disables the CPU cap. |
| `test_memory` | `8G` | Memory for a run, applied as `MemoryHigh` (throttle, not a hard kill). `0` disables it. |

The reason is concurrency: several agents, background tasks and containers can
be working at once, and unbounded they all run flat out until the desktop stops
responding. The scope both caps what one run may take and lowers its CPU and
I/O share and its `nice` level, so the foreground session always wins a
contention and only a genuinely idle machine is used at full speed.

```bash
just test                      # capped at 6 cores / 8G
just test_cpus=12 test         # let it use the whole machine
just test_cpus=0 test_memory=0 test   # no caps, no scope at all
```

The scheduling shares and the hard ceilings that stay off (`MemoryMax`,
`MemorySwapMax`, `TasksMax` — they kill rather than throttle) are fixed
constants at the top of `packaging/run-limited.sh`, deliberately not
environment variables: a knob per invocation is how the limits stop being
applied. Edit a value there if a run needs a different share.

The script falls back to running the command unwrapped, with a note on stderr,
when systemd is unavailable (a container, a CI runner with no user manager, a
non-systemd host), and when `systemd-run` rejects the limits. The caps are a
courtesy to the developer's machine, never a correctness requirement.

### Layer 2 — per-container caps, because containers escape the scope

Rootless podman does not place a container inside the cgroup of whatever
started it. Each container gets its own scope under
`user@<uid>.service/user.slice/libpod-<id>.scope`, a *sibling* of the test
scope, so it inherits none of the limits above. Verified on podman 6.1.1: a
container started from inside a `CPUQuota=200%` scope still reports
`cpu.max: max`.

Test containers are therefore capped separately, at **2 CPUs and 1024 MB**
each. The suite is serialized (`--test-threads=1`), so at most two are live at
once — Vaultwarden plus the openssh-server image during the SSH tests.

These are fixed constants, not variables: reaching them from `just` would mean
adding an environment variable, and the values are generous enough that no run
has needed to change them. They live in three places, which must stay in step:

- `crates/cosmic-bwarden-tests/src/container_limits.rs` (the Rust suite)
- `tools/run_vaultwarden.sh` (`CONTAINER_CPUS` / `CONTAINER_MEM_MB`)
- `tests/browser-extension/run-chrome-e2e.sh` (same names)

The Rust suite applies its caps after `start()` through bollard's
`update_container`, because testcontainers 0.23 has no create-time resource
API. The shell scripts pass `--cpus`/`--memory` to `docker run`, which both
runtimes honour at create time.

**Do not switch this to `NanoCpus`.** Podman's Docker-compat API accepts
`NanoCpus` on the update endpoint, returns HTTP 200, and silently ignores it —
`cpu.max` stays `max` and `HostConfig.NanoCpus` reads back `0`. Only
`CpuQuota` + `CpuPeriod` take effect.
`container_limits::tests::cpu_and_memory_caps_reach_the_cgroup` reads the limit
back through the API and fails if it did not land, so the trap cannot return
silently.

Capping a container is best-effort: a runtime without the update endpoint logs
`[container-limits] could not cap …` and the run continues, since no test
asserts on the limit. The Rust harness captures stderr for passing tests, so
that line only appears under `--nocapture` or when a test fails. To check:

```bash
cargo test -p cosmic-bwarden-tests -- --test-threads=1 --nocapture 2>&1 | grep container-limits
```

## Prerequisites

1.  **Container runtime** — **podman is the primary, recommended runtime**
    (used via `testcontainers-rs`' Docker-compatible API; no `docker` group or
    daemon needed):
    ```bash
    systemctl --user start podman.socket   # socket activation; the harness auto-detects $XDG_RUNTIME_DIR/podman/podman.sock
    ```
    Docker works as a drop-in alternative when `DOCKER_HOST` or
    `/var/run/docker.sock` is present. `just test-agent` (and friends)
    auto-detects or starts the socket.
2.  **Binaries**: The agent and CLI binaries must be pre-built as the E2E tests invoke them from `target/debug`.
    ```bash
    cargo build
    ```
    (`just test-agent`/`test-cli`/`test-ui` rebuild them automatically.)

## Which action does the client send? (the seam)

The layered strategy above has a blind spot worth stating explicitly, because a
real bug lived in it: **the E2E suite hand-builds the `Action` it sends.**
`vault/crud.rs` constructs `Action::AddEntry { … }` in Rust and pushes it over
IPC, which proves the *agent* handles that action — it can never catch the
*client* choosing the wrong one. Meanwhile the UI tests drive real `Message`s
but discard the returned `Task` (`let _ = app.update(…)`) and then hand-feed a
success (`SaveEditResult(Ok(()))`), so they assert on an outcome the test
invented. Both sides were green while the vault window sent every new entry as
`UpdateEntry`, producing `PUT /ciphers/new-<unix_secs>` and an HTTP 400.

Two rules keep that seam covered:

1. **Build actions in pure functions, never inline in the `Task::perform`
   async block.** An action constructed inside the closure is unreachable from
   any test that doesn't run an executor and an agent. These live in
   `protocol::entry_save` (core, shared with the E2E suite) and, in the UI,
   `app/update/{vault,auth,generator}_actions.rs`; all are unit-tested
   directly, with no runtime, socket, or server.
2. **Assert the emitted action, not a simulated response.** A UI test that
   feeds itself `Ok(())` passes even when the dispatched action is one the
   server rejects. Drive the real messages, then assert what the mapping
   produces from the resulting state — see
   `app/tests/flows.rs::test_e2e_user_flow_login_and_add_note`.

`vault/ui_save_flow.rs` closes the loop end to end: it builds the draft exactly
as `Message::AddEntryRequested` does (placeholder `new-<unix_secs>` id), routes
it through the same `entry_save::save_action` the UI calls, and sends the
result to a real agent and Vaultwarden. The action under test is chosen by
production code rather than by the test author.

## Known Gaps & Coverage

- **Missing**: `test_ssh_key_crud_lifecycle` is currently documented but NOT implemented.
- **Agent/CLI unit tests**: both crates now carry unit tests (agent ~67, CLI ~9)
  and `just test-unit` runs them. Until 2026-09-06 the `just` recipes ran only
  the core and UI crates, so those tests existed but were never executed by the
  project's own entry point.
- **Remaining inline actions**: the parameterless status/config queries
  (`GetConfig`, `CheckTpm`, `GetTpmDaStatus`, `CheckTpmDiagnostics`, `Version`,
  `SetPendingEntry`) are still built inline. They carry no branch and no field
  logic, so there is no decision for a test to observe — extracting them would
  add indirection without adding coverage.

## Vaultwarden Configuration

For the full test suite to pass, the Vaultwarden container must be configured with certain experimental features enabled.

- **SSH Keys**: Set `EXPERIMENTAL_CLIENT_FEATURE_FLAGS=ssh-key-vault-item` to enable support for Bitwarden type 5 (SSH Key) items.
- **Client Version**: The client version reported by `cosmic-bwarden` is set to `2025.1.0` to ensure compatibility with modern Bitwarden features during synchronization.

## Automated Manual Testing

The CLI supports non-interactive flags for use in scripts or CI environments:

```bash
# Start the agent in the background.
# Note the `test-` prefix: cleanup_profile() (tests/browser-extension/cleanup.sh)
# refuses any profile not named test-*, so a `manual-test` profile cannot be
# torn down with the shared helper — and its dirs would linger in ~/.cache,
# ~/.config, ~/.local/share and $XDG_RUNTIME_DIR.
export COSMIC_BWARDEN_PROFILE=test-manual
./target/debug/cosmic-bwarden-agent &

# Login (prompts for the master password)
./target/debug/cosmic-bwarden-cli login user@example.com --server http://localhost:8080

# Add an entry with a secret
./target/debug/cosmic-bwarden-cli add "My Secret" --username "admin" --password "supersecret"

# Sync and verify
./target/debug/cosmic-bwarden-cli sync
./target/debug/cosmic-bwarden-cli get "My Secret"

# Tear the profile's state back down when finished.
source tests/browser-extension/cleanup.sh
cleanup_profile test-manual
```

## Cleanup after tests

`just clean-test-residue` lists any leftover `cosmic-bwarden-test-*` profile
directory across `$XDG_CONFIG_HOME`, `$XDG_CACHE_HOME`, `$XDG_DATA_HOME`,
`$XDG_RUNTIME_DIR` and `$TMPDIR`; `just clean-test-residue-apply` removes them.
It refuses anything not named `cosmic-bwarden-test-*`, any symlink, and any
non-directory, so the live `cosmic-bwarden` profile cannot match. Use it after
a run is SIGKILLed, which skips every teardown path — not as part of the normal
loop.

Beyond profile directories, three other things a test run can leave behind, all
now handled:

- **Anonymous podman volumes.** Vaultwarden mounts `/data` as an anonymous
  volume. `RemoveContainerOptions` must set `v: true` or the volume outlives
  the container and sits dangling forever. testcontainers' own teardown does
  this; `cleanup_stale_containers` did not, so a volume leaked each time a
  killed run's container was swept.
- **Zombie processes.** Every `kill()` needs a matching `wait()`, or the child
  stays defunct until the test binary exits. Three sites also restarted an
  agent on the same socket without reaping the previous one first.
- **Files in `/tmp` and the repo root.** Script logs are removed in each
  `cleanup()` trap, and the E2E harness writes `agent_test.log` under `target/`
  rather than the repo root.


Every suite removes its own state; there is no sweep command to remember. The
rule, and the reason a spawn must never *inherit* its environment, is in
`AGENTS.md` under "Tests must never touch real user state"; the mechanics and
the end-to-end verification recipe are in `docs/test_cleanup_plan.md`.

Two things are worth knowing before writing a new test:

- **Rust E2E** — `TestEnv` redirects all four XDG vars into a
  `tempfile::tempdir()`, so its dirs never reach the home. `TestEnv::Drop`
  additionally *reports* (never deletes) any `cosmic-bwarden*` dir that
  appeared in the real roots during the test, via `state_guard.rs`. If you see
  `[state-guard] ERROR:` in the output, a spawn is missing its explicit
  environment.
- **Browser-extension scripts** — `tests/browser-extension/cleanup.sh` provides
  `cleanup_profile()` (guards against empty, non-`test-*`, and traversal names,
  and refuses to touch the live `cosmic-bwarden` dirs) plus
  `backup_file()`/`restore_file()` for the native-messaging manifests the
  suites overwrite.

## Debugging

If tests fail, the agent logs are captured in temporary files. The E2E suite is configured to print these logs on failure when using `--nocapture`. Look for "Agent Log:" in the test output for detailed internal state transitions and error messages.
