# Browser Extension Integration

Cosmic BWarden includes a native browser extension (currently targeting Firefox) that provides secure, high-performance vault access directly within the browser.

## Architecture: The Native Messaging Bridge

The extension follows a **"Thin Client" Invariant**: it contains no cryptographic logic and stores no secrets. Instead, it acts as a UI proxy for the `cosmic-bwarden-agent`.

Communication is handled via the [WebExtensions Native Messaging API](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging).

### 1. The `browser-host` Subcommand

The `cosmic-bwarden-agent` includes a specialized mode triggered by the `browser-host` subcommand. This mode is responsible for:
- Reading JSON messages from `stdin` (sent by the browser).
- Translating JSON to the internal `Action` protocol (using Postcard).
- Forwarding the action to the main agent process via the local Unix socket.
- Translating the agent's `Response` back to JSON.
- Writing the JSON response to `stdout` (read by the browser).

### 2. Configuration & Compilation

The browser integration is guarded by a Cargo feature flag in `cosmic-bwarden-agent`:

- **Feature Name**: `browser-host`
- **Default**: Enabled
- **Crate**: `crates/cosmic-bwarden-agent`

To compile the agent without browser support:
```bash
cargo build -p cosmic-bwarden-agent --no-default-features
```

The `browser-host` feature requires the `io-std` feature of `tokio` for asynchronous access to standard input and output streams.

## Security Model

- **No Local Storage**: The extension does not use `browser.storage` for vault data. All searches and fetches are performed on-demand via the agent.
- **Process Isolation**: The `browser-host` subcommand runs as a separate process from the main agent daemon, providing an additional layer of isolation.
- **Native Endianness**: The Native Messaging protocol requires a 4-byte length prefix in the CPU's native endianness, which the bridge correctly handles to avoid cross-platform issues.

## Registration & Installation

Before the extension can communicate with the agent, the host must be registered with the browser.

### Automatic Registration
Use the provided `just` task:
```bash
just register-browser-host
```
This runs `tools/register_browser_host.py`, which:
1. Detects the agent's location in `target/debug`.
2. Creates a wrapper script to handle the `browser-host` argument.
3. Generates the host manifest JSON in `~/.mozilla/native-messaging-hosts/com.8bit.cosmic_bwarden.json`.

### Manual Installation
1. Open Firefox and navigate to `about:debugging#/runtime/this-firefox`.
2. Click **Load Temporary Add-on...**.
3. Select `browser-extension/manifest.json`.

## Protocol Translation

| Source | Format | Protocol |
| :--- | :--- | :--- |
| Extension Popup | JSON | `{"GetConfig": null}` |
| `browser-host` (Bridge) | Postcard | `Action::GetConfig` |
| Main Agent | Postcard | `Response::Config { ... }` |
| `browser-host` (Bridge) | JSON | `{"Config": { ... }}` |

## Verification

The integration is verified by an E2E test suite in `crates/cosmic-bwarden-tests/src/browser_host.rs`. This test:
1. Spawns the main agent.
2. Spawns the agent in `browser-host` mode.
3. Simulates the browser by sending length-prefixed JSON to the bridge's `stdin`.
4. Verifies the JSON responses received on `stdout`.

To run the integration tests:
```bash
cargo test -p cosmic-bwarden-tests --lib browser_host
```
