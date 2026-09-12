# Cosmarden Client: Context & Architecture

A secure, native COSMIC Bitwarden client featuring a background agent, tray applet, and flexible CLI.

## Core Architecture

The project follows a modular Rust-based architecture split into specialized crates, each further decomposed for maintainability:

- **`cosmarden-core`**: The foundational library.
    - `api/`: API clients (`client.rs`) and data transfer models (`models.rs`).
    - `db/`: Persistence logic (`persistence.rs`) and vault models (`models.rs`).
    - `crypto/`: Cryptographic primitives and cipherstrings.
    - `protocol/entry_save.rs`: Pure `Entry -> Action` mapping shared by clients — decides *create vs. update* from whether the entry still carries a client-side `new-<unix_secs>` placeholder id. Lives in core so the E2E suite can drive the exact mapping the UI uses against a real server.
- **`cosmarden-agent`**: A secure background service.
    - `handler.rs`: Central IPC request dispatcher.
    - `server.rs`: High-level server-side synchronization logic.
    - `logind.rs`: Integration with systemd-logind for auto-locking.
    - `ssh_agent.rs`: SSH agent protocol implementation.
- **`cosmarden-cli`**: A feature-rich command-line interface.
- **`cosmarden-ui`**: The main graphical interface.
    - `app/`: MVU decomposition into `state.rs`, `update/` (chained `lifecycle`/`auth`/`vault`/`vault_edit`/`applet`/`pwgen` handlers), and `tasks.rs`. `update/vault_edit.rs` owns the detail pane's edit buffer; `update/{vault,auth,generator}_actions.rs` hold the pure `state -> Action` builders so tests can assert what the UI dispatches. `auth_actions` is shared by the main window and the applet, which previously built the same session actions twice. `update/activation.rs` is the pure classifier for applet activation-token exec strings (`open-vault` spawn vs. `activate:<name>` in-process quick actions).
    - `view/`: Modular view components (Auth, Vault, Settings, `applet/`). The generator pane's Settings/History split is a `tab_bar` (`state::generator_tabs`).
    - **New cipher types (Bitwarden v2026.7.0)**: cipher types 6 (BankAccount), 7 (DriversLicense), 8 (Passport) are fully synced, decrypted, displayed (detail pane + CLI `get`), and CRUD-able via the wire protocol (`AddBankAccount`/`AddDriversLicense`/`AddPassport`). The UI is read-only for them (like Card/Identity): no creation form, not in the applet search, "All" filter only. Vaultwarden only *emits* these types today — its write path rejects them until PR #7478 (`pm-32009-new-item-types`); the E2E suite probes server support and skips with a notice until then (see `tests/vault/new_item_types.rs`).
- **`cosmarden-tests`**: End-to-end integration tests using Docker.
    - `vault/ssh_agent.rs` / `vault/ssh_agent_lifecycle.rs`: Real-protocol SSH agent coverage — a real `ssh`/`ssh-add` client signs/authenticates against a containerized `sshd` via the agent's `ssh-agent-socket` (Ed25519 + RSA), including lock/unlock and logout/login state-transition checks. Helpers in `ssh_test_utils.rs`.

## Versioning & Protocol Compatibility

- **Application version**: Generated at build time in `cosmarden-core/build.rs` with format `YYYY.MM-N-<short git id>` where N is the number of seconds elapsed in the current month.
- **Unified builds**: A 30-second cache window via `target/build_version.txt` ensures all crates in a single build share the same version.
- **IPC protocol**: `Response::Version { version, protocol_version }` carries both the agent's build version and the protocol version. Currently both fields contain the same build version since all binaries are built together.
- **CLI check**: `cosmarden version` queries the agent, prints local/agent/protocol versions, and runs `check_protocol_compatibility()` — a pure function that compares the local build version against the agent's `protocol_version`.
- **UI display**: Version is shown muted in the applet context menu (next to "Open Vault") and in the Settings panel.

## Account State & Snapshot Ordering

There is no single "account state" enum — each surface derives it from flags the agent
reports in `Response::Config` (`needs_login`/`has_account`/`is_locked`/`sync_failed`) plus
the TPM status. Because the desktop UI applies those snapshots over an async pipe,
ordering matters:

- **Lock epoch**: the agent stamps every `Response::Config` with `(session_id, lock_epoch)`.
  `session_id` is random per agent process; `lock_epoch` increments on every lock-state
  transition (lock, unlock, login, logout — `State::bump_epoch()`). Clients drop any
  snapshot older than the newest one they have applied (`CosmardenApp::last_config`),
  so a response computed before a transition can never bounce the UI back to a stale view
  (e.g. back to the lock screen right after a successful PIN unlock). A changed
  `session_id` (agent restart) is never treated as staleness.
- **Out-of-sync survives locks**: `sync_failed`/`last_sync_error` are cleared by a
  successful sync, by a successful login (its initial server sync replaces the local
  vault, so a stale flag from a previous session must not survive), and by logout
  (account teardown) — `State::lock()` deliberately does not reset them. Both unlock
  paths re-authenticate and then run a background sync; a failed silent re-auth (or a
  PIN unlock with neither a session token nor a sealed server-credential hash) sets
  `sync_failed` so the UI honestly reports "not synced" instead of whitewashing the
  state on a lock cycle.
- **One unlock-request path**: `Action::RequestUnlock` and the internal SSH-agent path
  both go through `State::request_unlock()`, which broadcasts `PinRequested` when a TPM
  PIN is configured, `UnlockRequested` otherwise, at most once per lock period. The UI
  derives its unlock form from those events (`UnlockMode`), never from the TPM status
  while unlocked, and sends a desktop `org.freedesktop.Notifications` `Notify` when an
  account is ready (no applet-popup auto-open).
- **Unseal failure classification**: `tpm::classify_unseal_failure` distinguishes
  `TPM_RC_AUTH_FAIL` (wrong PIN, DA attempt consumed), `TPM_RC_LOCKOUT`, and
  `TPM_RC_POLICY_FAIL` (PCR state changed — BIOS/firmware update). Only the last maps to
  the stable `ERR_TPM_STATE_CHANGED` message: it is recovery guidance, never an
  "Incorrect PIN" mislabel. The PCR{0,7} seal binding itself is unchanged — a BIOS
  change still requires a master-password unlock and PIN re-seal.

## Technical Stack

- **Language**: Rust
- **UI Framework**: `libcosmic` 1.0.0 (MVU Architecture)
  - **Dependency declaration (do not change casually)**: `libcosmic`/`cosmic-config` in `Cargo.toml` must stay a **bare git URL** (`git = "https://github.com/pop-os/libcosmic"`), not `rev =`/`branch =`. The `applet` feature pulls `cosmic-panel-config`, whose transitive `cosmic-config` dep uses the bare URL; Cargo treats bare / `?rev=` / `?branch=` as **distinct sources**, so adding any qualifier on our side re-compiles the libcosmic tree (binary bloat). The exact commit is pinned by `Cargo.lock`.
  - **Which commit (bump with care)**: the lock holds **53314201b** (2026-09-11), the master tip's parent, not the tip. The tip's `chore: update iced` commit moves the bundled iced submodule's `cosmic-client-toolkit` pin to rev `c0cff4d` while libcosmic's own `Cargo.toml` still pins `32283d7`, so that commit pulls **two** cosmic-protocols sources into the graph — the duplicate the bare-URL rule above exists to prevent. No `[patch]` can fix it: Cargo rejects a same-URL patch ("patches must point to different sources") and silently ignores a rev-qualified key. A plain `cargo update -p libcosmic` therefore reintroduces the duplicate; bump deliberately and re-check the lock for a single `cosmic-protocols` revision.
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
- **Secrets stay wrapped end to end**: the IPC responses that carry plaintext (`Password`, `Totp`, `GeneratedPassword`, `GeneratorHistoryEntry`) hold `db::Secret`, not `String`, so they zeroize on drop; the UI propagates that through `OnDemandPayload` and its secret-carrying `Message` variants. `#[serde(transparent)]` keeps the wire bytes identical to a bare string, so this needed no protocol bump. Unwrapping happens only where plaintext is inherent — the clipboard and the `secure_input` widget — and on CLI stdout, where `Secret`'s redacting `Display` would otherwise print `********` instead of the value.
- **Safe Persistence**: `access_token` and `refresh_token` are marked `#[serde(skip)]` in `Db` to prevent plaintext leakage in JSON cache. Two authorized long-term stores exist: the Secret Service keyring via `oo7` (opt-in `keyring` cargo feature, gated on `persist_session`) and the **session envelope** below (always on).
- **Session Envelope**: the refresh token is persisted at `<data_dir>/session_<account_hash>.enc` under XChaCha20-Poly1305, keyed by HKDF-SHA256 over the vault encryption key with a dedicated label (`core/session_envelope.rs`). The vault keys only exist after an unlock — for PIN unlock, only after the TPM releases them under PolicyPCR(0,7) ∧ PolicyAuthValue — so the file inherits the TPM's PCR and PIN binding without a second sealed object. A refresh JWT (~660 bytes on Vaultwarden) is far past `TPM2_MAX_SYM_DATA` (256), which is why it cannot be sealed directly. The AEAD's AAD binds each envelope to one `(server, email)`, so it cannot be replayed across accounts or profiles.
- **Granular Reprompts**: Full enforcement of Master Password reprompting for sensitive items.
- **Reactive State**: The UI uses long-lived `Action::Subscribe` streams for agent-pushed events (`Locked`, `Unlocked`, `VaultChanged`) instead of polling.
- **Safe Domain Matching**: Entry-to-page matching (popup suggestions, badge, save prompt) uses exact / label-boundary-subdomain / PSL eTLD+1 rules in `cosmarden_core::domain` — never label-stripping of the page host, so `victim.co.uk` can never surface other `.co.uk` entries. Rationale and feature gate in `docs/public_suffix_list.md`.

### 🔄 Data Integrity
- **Real-Time CRUD Sync**: Every Add, Update, and Delete operation is immediately synchronized with the server.
- **Manual Sync**: Dedicated Sync button for on-demand refreshes.

## Key Workflows

### Browser Save Prompt
On login-form submit, the extension captures the credentials, holds them in an in-memory per-tab map in the background script, and — once the post-login page settles — asks the agent (`CheckLoginMatch`) whether to offer an in-page **Save** (new Login with the site's origin URI, via the extended `AddEntry`) or **Update** (`UpdateLoginPassword`) bar. Invariants: the password comparison happens inside the agent (stored secrets never transit to JS), messages to the page never carry the password, and pending credentials are never persisted or logged. Details in `docs/browser_integration.md`.

### Password Generator
Charset-based generation, "last used settings", and a device-global 7-day history all live in the agent (`handler/generator/`), not in any one client — this is what lets the desktop pane, applet quick-gen, CLI (`generate` subcommand), and browser extension (context menu + inline field icon) share one set of settings and one history. `Action::GeneratePassword { settings: Option<GeneratorSettings> }` is the single request every surface uses: `Some` persists new settings and generates with them (the desktop pane's Generate button); `None` reuses whatever is currently persisted (applet/CLI-bare/browser extension). Deliberately independent of vault-lock state — no unlock, and no account, is required to generate. The 7-day history is encrypted at rest by reusing the existing `cipherstring.rs` symmetric cipher with a locally-generated, device-global key (not derived from any master password) — see `docs/password_generator_plan.md` for the exact threat model this does and doesn't cover, along with the full design and the storage paths.

### Authentication
1. **Registration/Login**: Communicates with Bitwarden/Vaultwarden APIs. When a TPM is present, the login form (desktop and CLI) always offers PIN unlock — including after logout, when a leftover sealed blob still sits on disk. Declining PIN (`Enable PIN` off, or an empty CLI PIN prompt) deletes those leftovers (`DisableTpmPin`: the vault-key blob and `tpm_enabled`). The same reseal-or-clear decision is used on the master-password unlock form.
2. **Unlock**: The agent holds the master key in memory-locked storage.
3. **Re-auth after unlock** (`handler/auth/reauth.rs`, shared by the master-password and PIN surfaces): tokens never survive a restart (`#[serde(skip)]`) and `State::lock()` drops them, so an unlock must re-mint a session. Sources, in order:
   1. the stored refresh token (session envelope) — revocable server-side, self-expiring, cannot change the account, and exempt from 2FA because the refresh grant bypasses the two-factor path. This is the only source a **PIN unlock** has.
   2. a full password grant, using a hash the caller derived moments ago from a password the user just typed. **Never from storage** — no master-password hash is persisted anywhere (no `State` field, no TPM blob, no envelope), so only the master-password unlock path can supply one.

   An earlier design sealed the hash in a second TPM blob so a PIN unlock could re-auth silently. It was removed: it stored the strongest credential in the system to avoid a prompt that fires at most once per refresh-token lifetime, and it could not help a 2FA account anyway. When the envelope is unusable the user is asked for their master password, which is also the answer for 2FA, a revoked device, and a changed TPM state. Agents started after the change delete any leftover `tpm_sealed_hash_*.bin` (`remove_legacy_hash_blob`).

   Both surfaces share one definition deliberately: the PIN path is the one with no fallback, so a divergence there stays invisible until a reboot. If every source fails the vault stays unlocked and usable offline, and `sync_failed` is set with the *specific* reason (2FA required, server unreachable, …) prefixed by `protocol::ERR_NO_SESSION`. Clients match that marker, never the prose, and offer an inline **Restore session** master-password prompt in the sidebar — `Action::Unlock` re-authenticates and syncs in place, where the old affordance forced a full logout and vault re-download.
4. **Recovery without an unlock**: `server::auth::with_refresh` runs the same envelope restore when it finds no token in memory, so a request that arrives after a failed unlock-time restore (server unreachable at that moment) re-mints a session by itself. Without it the only ways out were another lock/unlock cycle or a master-password prompt, neither warranted by a transient outage. Serialized on `refresh_lock` and re-checked after acquiring it, so concurrent requests don't each spend a restore.
5. **Missing sealed blob**: PIN unlock checks the blob exists before unsealing and returns `protocol::ERR_TPM_BLOB_MISSING`, distinct from `ERR_TPM_UNSEAL_FAILED`. The blob path derives from the server URL, so changing it (or a TPM reset) orphans the blob — reporting that as a failed unseal reads as "wrong PIN" and makes the user retry, burning TPM dictionary-attack attempts against a file that isn't there.
6. **Token refresh**: `server::auth::with_refresh` rolls the envelope forward on every successful refresh, so its validity window keeps moving (Vaultwarden's default is 30 days for a non-mobile device, and it does *not* invalidate the previous refresh JWT on use).
7. **Multi-Window Interaction**: UI automatically switches to the workspace window only after successful unlock.

### CLI Interactions
- **Modern Syntax**: Uses `KEY=VALUE` pairs for adding and editing entries.
- **Flexible Keywords**: Entry types can be placed anywhere in the command.

### COSMIC Panel Applet
The applet popup (`view/applet/`) is self-sufficient for everyday use without opening the full vault window:
- **Inline Unlock**: A master-password `secure_input` (with eye-icon reveal toggle) showing "Locked: need password" while locked, with an unlock-icon submit button and Enter-to-submit (`view/applet/unlock.rs`); a successful `AppletUnlockResult` immediately switches the popup to the search view and refreshes results, with `Event::Unlocked` as a secondary sync path.
- **Quick Search**: A `search_input` plus a favourites star toggle (`view/applet/search.rs`). An empty query always shows favourites only, regardless of the toggle. Results are capped at 10. Each row shows the truncated label; open-in-vault / open-URI / copy-secret icons appear only while that row is hovered (native app-list overlay pattern). Sensitive copies that require reprompt show an inline master-password row with a reveal toggle and Enter-to-submit.
- **Menu Actions** (when unlocked): "Open Vault Window", "Lock" (stay running), "Logout" (stay running), then a single native `menu_button` **Quit** footer (plain close, no lock). Locked and protocol-mismatch popups use the same Quit row. Header Lock/Logout cover stay-running session actions; there is no Lock-and-Quit / Logout-and-Quit footer. Quit does not lock so the agent's unlocked state remains available to the SSH agent and CLI.
- Pure popup logic (row building, truncation, favourites/query rules, quit listing, hover-icon visibility) lives in `app/applet_search.rs` and `app/applet_menu.rs`, unit-tested without any widget dependencies.

## See Also

- [`README.md`](README.md) — project overview, and the index of every document in `docs/`
- [`AGENTS.md`](AGENTS.md) — agent/AI guidelines: golden rules, security invariants, and validation commands
- [`docs/ssh-agent.md`](docs/ssh-agent.md) — SSH agent protocol and socket configuration
- [`docs/ssh_agent_locked_message.md`](docs/ssh_agent_locked_message.md) — locked-vault identity listing and wait-on-sign (decision record)
- [`docs/browser_integration.md`](docs/browser_integration.md) — browser extension IPC
- [`docs/configurable_paths.md`](docs/configurable_paths.md) — path overrides and multi-instance isolation
- [`docs/cosmic_integration.md`](docs/cosmic_integration.md) — COSMIC panel applet registration
- [`docs/testing.md`](docs/testing.md) — test suite structure and run order
- [`docs/build_and_run.md`](docs/build_and_run.md) — build instructions and run modes
- [`docs/implementation.md`](docs/implementation.md) — crypto and vault sync internals
