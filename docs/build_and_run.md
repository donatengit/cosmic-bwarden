# Build and Run Instructions

## Prerequisites

Before building, ensure you have the following installed on your system:
- **Rust Toolchain**: `rustc` and `cargo` (1.75.0 or later recommended).
- **System Dependencies** (for `libcosmic` and cryptography):
  - `libxkbcommon-dev`
  - `libwayland-dev`
  - `libegl1-mesa-dev`
  - `pkg-config`
  - `cmake`
  - `openssl` (or `pkg-config` for system-wide detection)

## Building for Native Architecture

To build the entire workspace with optimizations for your specific CPU architecture:

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

This will produce binaries in `target/release/`:
- `cosmic-bwarden-agent`: The background daemon.
- `cosmic-bwarden-ui`: The main GUI application and panel tray applet.
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
./target/release/cosmic-bwarden-ui
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
./target/release/cosmic-bwarden-ui
```

## Environment Variables

- `COSMIC_BWARDEN_PROFILE`: Set this to use a different configuration profile (default is `cosmic-bwarden`).
- `RUST_LOG`: Set to `info` or `debug` for verbose logging (e.g., `RUST_LOG=cosmic-bwarden_agent=debug`).

## SSH Agent Setup
To use the built-in SSH agent, export the following environment variable in your shell profile:

```bash
export SSH_AUTH_SOCK=$(cosmic-bwarden-agent --print-ssh-socket-path) 
# Or manually find it in the runtime directory managed by the agent.
```
