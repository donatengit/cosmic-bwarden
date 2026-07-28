# Cosmic BWarden Client: Context & Architecture

A secure, native COSMIC Bitwarden client featuring a background agent, tray applet, and flexible CLI.

## Core Architecture

The project follows a modular Rust-based architecture split into specialized crates, each further decomposed for maintainability:

- **`cosmic-bwarden-core`**: The foundational library.
    - `api/`: API clients (`client.rs`) and data transfer models (`models.rs`).
    - `db/`: Persistence logic (`persistence.rs`) and vault models (`models.rs`).
    - `crypto/`: Cryptographic primitives and cipherstrings.
    - `protocol/entry_save.rs`: Pure `Entry -> Action` mapping shared by clients — decides *create vs. update* from whether the entry still carries a client-side `new-<unix_secs>` placeholder id. Lives in core so the E2E suite can drive the exact mapping the UI uses against a real server.
- **`cosmic-bwarden-agent`**: A secure background service.
    - `handler.rs`: Central IPC request dispatcher.
    - `server.rs`: High-level server-side synchronization logic.
    - `logind.rs`: Integration with systemd-logind for auto-locking.
    - `ssh_agent.rs`: SSH agent protocol implementation.
- **`cosmic-bwarden-cli`**: A feature-rich command-line interface.
- **`cosmic-bwarden-ui`**: The main graphical interface.
    - `app/`: MVU decomposition into `state.rs`, `update/` (chained `lifecycle`/`auth`/`vault`/`vault_edit`/`applet`/`pwgen` handlers), and `tasks.rs`. `update/vault_edit.rs` owns the detail pane's edit buffer; `update/{vault,auth,generator}_actions.rs` hold the pure `state -> Action` builders so tests can assert what the UI dispatches. `auth_actions` is shared by the main window and the applet, which previously built the same session actions twice.
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
- **Decryption Caching**: The agent caches decrypted names/usernames — plus each login's URI hosts for tab-domain matching (`CachedSidebarEntry.hosts`) — enabling instant search even in huge vaults. Plaintext hosts in unlocked-agent memory are deliberate: same sensitivity class as the cached names (see `docs/public_suffix_list.md`).
- **Multi-Window Flow**: Distinct compact `Auth` window vs full `Main` window workspace.
- **Intelligent Focus**: Prevents window fragmentation by focusing existing windows using `window::gain_focus`.
- **Compact UX**: Uses `autosize` and `Length::Shrink` for a professional, focused feel on sensitive dialogs.

### 🛡️ Security
- **Thin Client Invariant**: UI and CLI MUST NOT store plaintext secrets in long-lived memory. Use `Secret` wrappers and transient `.expose()`.
- **Safe Persistence**: `access_token` and `refresh_token` are marked `#[serde(skip)]` in `Db` to prevent plaintext leakage in JSON cache. Keyring persistence via `oo7` is the authorized long-term storage method.
- **Granular Reprompts**: Full enforcement of Master Password reprompting for sensitive items.
- **Reactive State**: The UI uses long-lived `Action::Subscribe` streams for agent-pushed events (`Locked`, `Unlocked`, `VaultChanged`) instead of polling.
- **Safe Domain Matching**: Entry-to-page matching (popup suggestions, badge, save prompt) uses exact / label-boundary-subdomain / PSL eTLD+1 rules in `cosmic_bwarden_core::domain` — never label-stripping of the page host, so `victim.co.uk` can never surface other `.co.uk` entries. Rationale and feature gate in `docs/public_suffix_list.md`.

### 🔄 Data Integrity
- **Real-Time CRUD Sync**: Every Add, Update, and Delete operation is immediately synchronized with the server.
- **Manual Sync**: Dedicated Sync button for on-demand refreshes.

## Key Workflows

### Browser Save Prompt
On login-form submit, the extension captures the credentials, holds them in an in-memory per-tab map in the background script, and — once the post-login page settles — asks the agent (`CheckLoginMatch`) whether to offer an in-page **Save** (new Login with the site's origin URI, via the extended `AddEntry`) or **Update** (`UpdateLoginPassword`) bar. Invariants: the password comparison happens inside the agent (stored secrets never transit to JS), messages to the page never carry the password, and pending credentials are never persisted or logged. Details in `docs/browser_integration.md`.

### Password Generator
Charset-based generation, "last used settings", and a device-global 7-day history all live in the agent (`handler/generator/`), not in any one client — this is what lets the desktop pane, applet quick-gen, CLI (`generate` subcommand), and browser extension (context menu + inline field icon) share one set of settings and one history. `Action::GeneratePassword { settings: Option<GeneratorSettings> }` is the single request every surface uses: `Some` persists new settings and generates with them (the desktop pane's Generate button); `None` reuses whatever is currently persisted (applet/CLI-bare/browser extension). Deliberately independent of vault-lock state — no unlock, and no account, is required to generate. The 7-day history is encrypted at rest by reusing the existing `cipherstring.rs` symmetric cipher with a locally-generated, device-global key (not derived from any master password) — see `AGENTS.md`'s "Password Generator" section for the exact threat model this does and doesn't cover. Full design in `docs/password_generator_plan.md`.

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
