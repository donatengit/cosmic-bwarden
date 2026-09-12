# Configurable Paths and Isolation

Cosmarden allows you to customize the paths for its IPC socket, SSH agent socket, and configuration file. This is useful for running multiple instances, testing, or advanced system configurations.

## Command Line Arguments

All binaries (`cosmarden-agent`, `cosmarden-cli`, and `cosmarden-ui`) support the following flags:

- `--config <PATH>`: Use a specific configuration file.
- `--socket <PATH>`: Use a specific Unix socket for IPC.

The agent also supports:
- `--ssh-socket <PATH>`: Use a specific Unix socket for the SSH agent protocol.

## Environment Variables

You can also set these paths via environment variables:

- `COSMARDEN_CONFIG`: Path to the configuration file.
- `COSMARDEN_SOCKET`: Path to the main IPC socket.
- `COSMARDEN_SSH_SOCKET`: Path to the SSH agent socket.

## Configuration File Settings

The `config.json` file supports these keys:

```json
{
  "socket_path": "/path/to/socket",
  "ssh_agent_socket_path": "/path/to/ssh-socket"
}
```

## Priority Order

The application resolves paths in the following order (highest to lowest priority):

1. **Command Line Argument** (`--socket`)
2. **Environment Variable** (`COSMARDEN_SOCKET`)
3. **Configuration File** (`socket_path` in `config.json`)
4. **Default System Path** (usually in `XDG_RUNTIME_DIR`)

## Testing Isolation

These features are used by the E2E test suite to ensure that every test run is completely isolated from the user's daily client. Each test starts an agent on a unique socket in a temporary directory, preventing data corruption or interference.

`COSMARDEN_PROFILE` namespaces the whole state tree: with it set to `<x>`,
the config, cache, data, and runtime dirs all become `cosmarden-<x>` under
their respective XDG roots (`dirs::profile()`). Unset — or empty — it resolves
to the **live** `cosmarden` profile.

## Cleanup after tests

A spawn that inherits its environment is the failure mode to design against.
`COSMARDEN_PROFILE` is process-global, and the E2E crate sets it in ~59
places without restoring it, so a subprocess launched with no explicit
environment silently adopts another test's profile — or the live one.

Rules (the normative copy is in `AGENTS.md`; the full rationale and the
verification recipe are in `docs/test_cleanup_plan.md`):

- Pass an **explicit** `COSMARDEN_PROFILE`, `HOME`, and **all four**
  `XDG_*` vars on every agent/CLI spawn. A partial set is not partial safety:
  `directories` falls back to the passwd entry when `$HOME` is unset, so a
  missing `XDG_DATA_HOME` still writes into the real `~/.local/share` even
  under `env_clear()`.
- Name test profiles `test-*`.
- Remove the four `cosmarden-<profile>` dirs — config, cache, data, **and
  runtime** — in the same teardown that kills the agent, after `wait`ing for
  it. Redirecting all four XDG vars into a `tempfile::tempdir()` satisfies
  this without an explicit `rm`, and is preferred for Rust spawns.
- Restore any real user file the test overwrote — browser native-messaging
  manifests especially, since those outlive the run and repoint a real browser.
- **Never derive a deletion path from `dirs::`.** Those functions read
  process-global env; an unset profile resolves to the live `cosmarden`
  profile, so such a "cleanup" would erase the developer's real vault cache.

Teardown helpers:

| Suite | Helper |
|---|---|
| Rust E2E | XDG redirect into `TestEnv::_temp_dir`; `state_guard::RealHomeSnapshot` *detects* (never deletes) leaks in `TestEnv::Drop` |
| Rust E2E guard | `paths.rs::agent_spawn_writes_nothing_to_the_real_home` — deterministic, spawns its own agent |
| Shell / Playwright | `cleanup_profile()` and `backup_file()`/`restore_file()` in `tests/browser-extension/cleanup.sh` |
