# Build and Run Instructions

## Prerequisites

Before building, ensure you have the following installed on your system:
- **Rust Toolchain**: `rustc` and `cargo` (1.75.0 or later recommended).
- **`just`** — the task runner used for all build/install/test orchestration (see `justfile`).
- **System Dependencies** (for `libcosmic` and cryptography):
  - `libxkbcommon-dev`
  - `libwayland-dev`
  - `libegl1-mesa-dev`
  - `pkg-config`
  - `cmake`
  - `openssl` (or `pkg-config` for system-wide detection)
- **For the Rust E2E test suite**: a container socket — either Docker, or podman with
  `systemctl --user start podman.socket` (auto-detected by the test harness).
- **For the browser extension**: `npm` (unit tests via vitest, E2E via Playwright).

> The reference repositories checked in as git submodules (`bitwarden-official-clients`,
> `libcosmic`, `cosmic_examples`, `rbw_reference`) are **not** build inputs — a plain
> `git clone` without `--recurse-submodules` builds fine; `libcosmic` is fetched by Cargo
> from GitHub.

## Building for Native Architecture

To build the entire workspace with optimizations for your specific CPU architecture:

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

This will produce binaries in `target/release/`:
- `cosmic-bwarden-agent`: The background daemon.
- `cosmic-applet-bwarden`: The main GUI application and panel tray applet (built from the `cosmic-bwarden-ui` crate).
- `cosmic-bwarden-cli`: The command-line interface.

## Running the Components

### 1. Start the Agent
The agent must be running for the UI or CLI to function.

```bash
./target/release/cosmic-bwarden-agent
```
*Note: In a production setup, this would typically be managed by a systemd user unit.*

### 2. Launch the UI / Applet
The same binary handles both the main window and the applet tray.

```bash
./target/release/cosmic-applet-bwarden
```

### 3. CLI Usage
The `cosmic-bwarden-cli` provides a powerful interface for scripting and advanced management.

```bash
# Register a new account
./target/release/cosmic-bwarden-cli register user@example.com --password mypassword

# Add a secure note
./target/release/cosmic-bwarden-cli add-note "My Private Key" --note "Content of the note..."

# Add an SSH key
./target/release/cosmic-bwarden-cli add-ssh-key "My SSH Key" --private-key-path ~/.ssh/id_rsa

# List and Search
./target/release/cosmic-bwarden-cli list
./target/release/cosmic-bwarden-cli list | grep "My Secret"

# Get password (interactive reprompt if needed)
./target/release/cosmic-bwarden-cli get "My Secret"

# Get a specific field (e.g. Note or SSH Private Key), revealing secrets
./target/release/cosmic-bwarden-cli get "My Private Key" --fields notes --show-secrets
```

### 4. Launch the Applet
If you are running the COSMIC desktop, you can launch the applet to see it in your panel.

```bash
./target/release/cosmic-applet-bwarden
```

## Environment Variables

- `COSMIC_BWARDEN_PROFILE`: Set this to use a different configuration profile (default is `cosmic-bwarden`).
- `RUST_LOG`: Set to `info` or `debug` for verbose logging (e.g., `RUST_LOG=cosmic-bwarden_agent=debug`).
  The HTTP stack (`reqwest`/`hyper`/`rustls`/`h2`) is capped at `info` even under
  `RUST_LOG=trace`: at trace level those crates print full request headers,
  including `Authorization: Bearer …` session tokens, and agent logs are
  persisted to disk by journald. The cap is applied in code (agent and UI
  logger setup) and cannot be raised via the environment.

## SSH Agent Setup
To use the built-in SSH agent, export the following environment variable in your shell profile:

```bash
export SSH_AUTH_SOCK=$(cosmic-bwarden-agent --print-ssh-socket-path) 
# Or manually find it in the runtime directory managed by the agent.
```
