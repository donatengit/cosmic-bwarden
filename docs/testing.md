# Testing cosmic-bwarden

This project follows a **Complexity-Ordered Testing Strategy** to optimize verification and context usage. Tests should be run in the following order:

1.  **Unit Tests (Core & UI Logic)**: Fast, isolated tests for encryption, serialization, and MVU state transitions.
2.  **Agent & Protocol E2E**: Verifies the background agent, its state management, and the IPC protocol.
3.  **CLI E2E**: Validates the command-line interface (assumes Agent is correct).
4.  **UI E2E**: High-level flow verification (assumes Agent and CLI are correct).

## Running Tests

The recommended way to run the full suite in order is via `just`:

```bash
just test
```

If you don't have `just` installed, you can run them manually in order:

### 1. Unit Tests
```bash
cargo test -p cosmic-bwarden-core
cargo test -p cosmic-bwarden-ui --lib
```

### 2. Agent & Protocol E2E
```bash
sg docker -c "cargo test -p cosmic-bwarden-tests agent security vault_ops pinned_ops applet_flow -- --test-threads=1"
```

### 3. CLI E2E
```bash
sg docker -c "cargo test -p cosmic-bwarden-tests cli_lifecycle cli_secret_mask_test custom_fields_cli -- --test-threads=1"
```

### 4. UI E2E
```bash
sg docker -c "cargo test -p cosmic-bwarden-tests window_flow custom_fields_ui ui_save_flow -- --test-threads=1"
```

---

## Prerequisites

1.  **Docker**: Ensure Docker is installed and the daemon is running (used by `testcontainers-rs`).
2.  **Permissions**: Your user must have permission to access `/var/run/docker.sock`. 
3.  **Binaries**: The agent and CLI binaries must be pre-built as the E2E tests invoke them from `target/debug`.
    ```bash
    cargo build
    ```

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
- **Agent Unit Tests**: The `cosmic-bwarden-agent` crate lacks unit tests and relies on integration tests.
- **CLI Unit Tests**: The `cosmic-bwarden-cli` crate lacks unit tests.
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
# Start the agent in the background
export COSMIC_BWARDEN_PROFILE=manual-test
./target/debug/cosmic-bwarden-agent &

# Login (prompts for the master password)
./target/debug/cosmic-bwarden-cli login user@example.com --server http://localhost:8080

# Add an entry with a secret
./target/debug/cosmic-bwarden-cli add "My Secret" --username "admin" --password "supersecret"

# Sync and verify
./target/debug/cosmic-bwarden-cli sync
./target/debug/cosmic-bwarden-cli get "My Secret"
```

## Debugging

If tests fail, the agent logs are captured in temporary files. The E2E suite is configured to print these logs on failure when using `--nocapture`. Look for "Agent Log:" in the test output for detailed internal state transitions and error messages.
