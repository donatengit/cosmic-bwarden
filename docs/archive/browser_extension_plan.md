# Plan: Firefox Extension for Cosmic BWarden (Integrated Agent)

Implement a "thin-client" Firefox extension that communicates with the `cosmic-bwarden-agent` via a new `browser-host` subcommand. This avoids a separate binary and simplifies the architecture.

## 1. Agent Subcommand: `browser-host` (Feature Flagged)

Update `cosmic-bwarden-agent` to support a subcommand for the Native Messaging protocol, guarded by a default-enabled feature flag.

### Implementation:
- Add a `browser-host` feature to `crates/cosmic-bwarden-agent/Cargo.toml`, enabled by default.
- Modify `crates/cosmic-bwarden-agent/src/main.rs` to check for `std::env::args()` within a `#[cfg(feature = "browser-host")]` block.
- If the first argument is `browser-host`, enter a dedicated loop:
    - Read 4-byte length prefix (native endian) from `stdin`.
    - Read JSON payload.
    - Connect to the local Unix socket (using `cosmic-bwarden-core/agent_client.rs`).
    - Send the `Action` (serialized via Postcard) to the agent.
    - Receive the `Response` (deserialized via Postcard).
    - Send 4-byte length prefix + JSON-serialized response to `stdout`.
- If the feature is disabled and the subcommand is called, exit with a helpful error message.
- Protocol Translation:
    - **Browser -> Subcommand**: `{"action": "Sync"}` (JSON)
    - **Subcommand -> Agent**: `Action::Sync` (Postcard over Unix Socket)
    - **Agent -> Subcommand**: `Response::Ack` (Postcard over Unix Socket)
    - **Subcommand -> Browser**: `{"response": "Ack"}` (JSON)

### Files to modify/create:
- `crates/cosmic-bwarden-agent/src/main.rs`: Handle subcommand logic.
- `crates/cosmic-bwarden-agent/src/browser_host.rs`: New module for Native Messaging loop.
- `tools/register_browser_host.py`: Script to register the agent with the `browser-host` arg as the host.

## 2. Firefox Extension (Thin Client)

Create a minimalist extension in `browser-extension/`.

### Background Script (`background.js`):
- Connect to the host using `browser.runtime.connectNative("com.enikeev.cosmic_bwarden")`.
- The browser will execute: `cosmic-bwarden-agent browser-host`.
- Maintain no long-term state or vault data in `browser.storage`.

### Popup UI (`popup/`):
- **Locked State**: Show "Vault Locked" if the agent returns `is_locked: true` in its config.
- **Search View**: Input field for searching the vault. Results are fetched on-demand from the agent.
- **Results**: List entries with "Copy Password" and "Auto-fill" buttons.

### Content Script (`content.js`):
- Simple login form detection.
- Inject credentials into detected `input[type="password"]` and related username fields.

### Files to create:
- `browser-extension/manifest.json`
- `browser-extension/background.js`
- `browser-extension/popup/popup.html`
- `browser-extension/popup/popup.js`
- `browser-extension/popup/popup.css`
- `browser-extension/content.js`

## 3. Build & Integration

- Add `just` tasks:
    - `just register-browser-host`: Registers the agent binary with the `browser-host` argument.
    - `just pack-extension`: Zips the extension for distribution.

## Verification Plan

### Automated Tests:
- Update `cosmic-bwarden-tests` to run the agent in `browser-host` mode and verify it can correctly proxy JSON messages to a running main agent process.

### Manual Verification:
1. Build the agent and start it normally.
2. Run the registration script.
3. Load the extension in Firefox.
4. Verify the extension can fetch and display vault items via the agent.
