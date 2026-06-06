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
sg docker -c "cargo test -p cosmic-bwarden-tests --test agent --test security --test vault_ops --test pinned_ops -- --test-threads=1"
```

### 3. CLI E2E
```bash
sg docker -c "cargo test -p cosmic-bwarden-tests --test cli_lifecycle --test cli_secret_mask_test --test custom_fields_cli -- --test-threads=1"
```

### 4. UI E2E
```bash
sg docker -c "cargo test -p cosmic-bwarden-tests --test window_flow --test custom_fields_ui -- --test-threads=1"
```

---

## Prerequisites

1.  **Docker**: Ensure Docker is installed and the daemon is running (used by `testcontainers-rs`).
2.  **Permissions**: Your user must have permission to access `/var/run/docker.sock`. 
3.  **Binaries**: The agent and CLI binaries must be pre-built as the E2E tests invoke them from `target/debug`.
    ```bash
    cargo build
    ```

## Known Gaps & Coverage

- **Missing**: `test_ssh_key_crud_lifecycle` is currently documented but NOT implemented.
- **Agent Unit Tests**: The `cosmic-bwarden-agent` crate lacks unit tests and relies on integration tests.
- **CLI Unit Tests**: The `cosmic-bwarden-cli` crate lacks unit tests.

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

# Login without an interactive prompt
./target/debug/cosmic-bwarden-cli login user@example.com --server http://localhost:8080 --password mypassword

# Add an entry with a secret
./target/debug/cosmic-bwarden-cli add "My Secret" --username "admin" --password "supersecret"

# Sync and verify
./target/debug/cosmic-bwarden-cli sync
./target/debug/cosmic-bwarden-cli get "My Secret"
```

## Debugging

If tests fail, the agent logs are captured in temporary files. The E2E suite is configured to print these logs on failure when using `--nocapture`. Look for "Agent Log:" in the test output for detailed internal state transitions and error messages.
