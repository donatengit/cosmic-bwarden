# Cosmic BWarden Client: Context & Architecture

A secure, native COSMIC Bitwarden client featuring a background agent, tray applet, and flexible CLI.

## Core Architecture

The project follows a modular Rust-based architecture split into specialized crates, each further decomposed for maintainability:

- **`cosmic-bwarden-core`**: The foundational library.
    - `api/`: API clients (`client.rs`) and data transfer models (`models.rs`).
    - `db/`: Persistence logic (`persistence.rs`) and vault models (`models.rs`).
    - `crypto/`: Cryptographic primitives and cipherstrings.
- **`cosmic-bwarden-agent`**: A secure background service.
    - `handler.rs`: Central IPC request dispatcher.
    - `server.rs`: High-level server-side synchronization logic.
    - `logind.rs`: Integration with systemd-logind for auto-locking.
    - `ssh_agent.rs`: SSH agent protocol implementation.
- **`cosmic-bwarden-cli`**: A feature-rich command-line interface.
- **`cosmic-bwarden-ui`**: The main graphical interface.
    - `app/`: MVU decomposition into `state.rs`, `update/` (chained `lifecycle`/`auth`/`vault`/`applet` handlers), and `tasks.rs`.
    - `view/`: Modular view components (Auth, Vault, Settings, `applet/`).
- **`cosmic-bwarden-tests`**: End-to-end integration tests using Docker.
    - `vault/ssh_agent.rs` / `vault/ssh_agent_lifecycle.rs`: Real-protocol SSH agent coverage — a real `ssh`/`ssh-add` client signs/authenticates against a containerized `sshd` via the agent's `ssh-agent-socket` (Ed25519 + RSA), including lock/unlock and logout/login state-transition checks. Helpers in `ssh_test_utils.rs`.

## Versioning & Protocol Compatibility

- **Application version**: Generated at build time in `cosmic-bwarden-core/build.rs` with format `YYYY.MM-N-<short git id>` where N is the number of seconds elapsed in the current month.
- **Unified builds**: A 30-second cache window via `target/build_version.txt` ensures all crates in a single build share the same version.
- **IPC protocol**: `Response::Version { version, protocol_version }` carries both the agent's build version and the protocol version. Currently both fields contain the same build version since all binaries are built together.
- **CLI check**: `cosmic-bwarden-cli version` subcommand queries the agent, prints local/agent/protocol versions, and runs `check_protocol_compatibility()` — a pure function that compares the local build version against the agent's `protocol_version`.
- **UI display**: Version is shown muted in the applet context menu (next to "Open Vault") and in the Settings panel.

## Technical Stack

- **Language**: Rust
- **UI Framework**: `libcosmic` 1.0.0 (MVU Architecture)
- **Networking**: `reqwest`, `tokio`
- **Security**: Memory-locked regions for secrets, AES-256-CBC, PBKDF2/Argon2id.
- **Testing**: `testcontainers-rs` (Vaultwarden).

## "Game Changing" Improvements

### 🚀 Performance & UI
- **Decryption Caching**: The agent caches decrypted names/usernames, enabling instant search even in huge vaults.
- **Multi-Window Flow**: Distinct compact `Auth` window vs full `Main` window workspace.
- **Intelligent Focus**: Prevents window fragmentation by focusing existing windows using `window::gain_focus`.
- **Compact UX**: Uses `autosize` and `Length::Shrink` for a professional, focused feel on sensitive dialogs.

### 🛡️ Security
- **Thin Client Invariant**: UI and CLI MUST NOT store plaintext secrets in long-lived memory. Use `Secret` wrappers and transient `.expose()`.
- **Safe Persistence**: `access_token` and `refresh_token` are marked `#[serde(skip)]` in `Db` to prevent plaintext leakage in JSON cache. Keyring persistence via `oo7` is the authorized long-term storage method.
- **Granular Reprompts**: Full enforcement of Master Password reprompting for sensitive items.
- **Reactive State**: The UI uses long-lived `Action::Subscribe` streams for agent-pushed events (`Locked`, `Unlocked`, `VaultChanged`) instead of polling.

### 🔄 Data Integrity
- **Real-Time CRUD Sync**: Every Add, Update, and Delete operation is immediately synchronized with the server.
- **Manual Sync**: Dedicated Sync button for on-demand refreshes.

## Key Workflows

### Authentication
1. **Registration/Login**: Communicates with Bitwarden/Vaultwarden APIs.
2. **Unlock**: The agent holds the master key in memory-locked storage.
3. **Multi-Window Interaction**: UI automatically switches to the workspace window only after successful unlock.

### CLI Interactions
- **Modern Syntax**: Uses `KEY=VALUE` pairs for adding and editing entries.
- **Flexible Keywords**: Entry types can be placed anywhere in the command.

### COSMIC Panel Applet
The applet popup (`view/applet/`) is self-sufficient for everyday use without opening the full vault window:
- **Inline Unlock**: A master-password `secure_input` (with eye-icon reveal toggle) showing "Locked: need password" while locked, with an unlock-icon submit button and Enter-to-submit (`view/applet/unlock.rs`); a successful `AppletUnlockResult` immediately switches the popup to the search view and refreshes results, with `Event::Unlocked` as a secondary sync path.
- **Quick Search**: A `search_input` plus a favourites star toggle (`view/applet/search.rs`). An empty query always shows favourites only, regardless of the toggle. Results are capped at 10 and rendered as two buttons per entry — a truncated primary label (username/note title/"Public key") and a secret label ("Password"/"Note"/"Private key") — both copying to the clipboard with a toast confirmation. Sensitive copies that require reprompt show an inline master-password row with a reveal toggle and Enter-to-submit.
- **Menu Actions** (when unlocked): "Open Vault Window", "Lock" (stay running), "Logout" (stay running), "Lock and Quit", "Logout and Quit", "Quit" (plain close, no state change). When locked: "Open Vault Window" and "Quit" only. "Quit" intentionally does not lock so the agent's unlocked state remains available to the SSH agent and CLI.
- Pure popup logic (row building, truncation, favourites/query rules) lives in `app/applet_search.rs`, unit-tested without any widget dependencies.

## See Also

- [`AGENTS.md`](AGENTS.md) — agent/AI guidelines, golden rules, validation commands, and the full document index
- [`docs/ssh-agent.md`](docs/ssh-agent.md) — SSH agent protocol and socket configuration
- [`docs/browser_integration.md`](docs/browser_integration.md) — browser extension IPC
- [`docs/configurable_paths.md`](docs/configurable_paths.md) — path overrides and multi-instance isolation
- [`docs/cosmic_integration.md`](docs/cosmic_integration.md) — COSMIC panel applet registration
- [`docs/testing.md`](docs/testing.md) — test suite structure and run order
- [`docs/build_and_run.md`](docs/build_and_run.md) — build instructions and run modes
- [`docs/implementation.md`](docs/implementation.md) — crypto and vault sync internals
