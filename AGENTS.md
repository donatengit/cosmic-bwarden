# cosmic-bwarden: Agent Guidelines

## Key Documents

Start here, then go deeper as needed:

| Document | Purpose |
|---|---|
| [`CONTEXT.md`](CONTEXT.md) | Architecture, security invariants, key workflows, "game-changing" features — the canonical project map |
| [`docs/summary.md`](docs/summary.md) | User-facing project overview and feature list |
| [`docs/build_and_run.md`](docs/build_and_run.md) | Prerequisites, build commands, run modes |
| [`docs/testing.md`](docs/testing.md) | Complexity-ordered testing strategy and what each test suite covers |
| [`docs/ssh-agent.md`](docs/ssh-agent.md) | SSH agent protocol implementation and socket paths |
| [`docs/browser_integration.md`](docs/browser_integration.md) | Native browser extension architecture and IPC protocol |
| [`docs/public_suffix_list.md`](docs/public_suffix_list.md) | Domain-matching rules (exact / boundary-subdomain / PSL eTLD+1) and the `public_suffix_list` feature |
| [`docs/configurable_paths.md`](docs/configurable_paths.md) | Socket, config, and SSH path overrides; multi-instance isolation |
| [`docs/cosmic_integration.md`](docs/cosmic_integration.md) | COSMIC panel applet registration and metadata |
| [`docs/implementation.md`](docs/implementation.md) | Crypto, vault sync, and data model internals |

## Golden Rules
- **Never ask for confirmation.** Apply fixes, run validation, iterate until passing. Report only on final outcome or exhausted options.
- **Never circle back to a failed approach.** If a fix didn't work, note why and move forward.
- **One responsibility per file.** If a file exceeds ~250 lines, it needs splitting.
- **cargo check before cargo test.** Don't run expensive tests against code that won't compile.
- **Never commit generated or temporary artifacts.** Build output, `node_modules`, test-runner results (`test-results/`, `playwright-report/`), coverage, and logs belong in `.gitignore`, never in a commit. If `git status` shows a generated path, add it to `.gitignore` rather than staging it (note: a slash-suffixed pattern matches directories only — drop the slash to also catch symlinks).

## Versioning

- **Build version**: `YYYY.MM-N-<git_id>` generated in `core/build.rs`. Reused across crates via a 30-second `target/build_version.txt` cache.
- **Protocol version**: Independent of the build version — `cosmic_bwarden_core::PROTOCOL_VERSION`, a small integer string bumped ONLY on breaking wire-protocol changes (adding a field to a postcard-encoded `Action`/`Response` variant counts). `Response::Version` always includes both `version` and `protocol_version` fields.
- **Compatibility check**: Pure function `check_protocol_compatibility()` in the CLI crate compares local version against agent's `protocol_version`. Unit-tested for both match and mismatch scenarios.
- **Adding a version subcommand**: Always add `Commands::Version` to the CLI's enum, route it to the auth handler, and include the `check_protocol_compatibility()` call. Update `preprocess_args` if the new command name conflicts with type keywords.
- **Breaking protocol changes**: Bump the `protocol_version` in `Response::Version` by updating `check_protocol_compatibility` expectations if the protocol surface changes incompatibly.

## Workspace Structure

| Crate | Responsibility |
|---|---|
| `cosmic-bwarden-core` | Daemon, IPC server, crypto, vault state |
| `cosmic-bwarden-cli` | Argument parsing, IPC client, output formatting |
| `cosmic-bwarden-ui` | libcosmic applet, MVU model/view/update |
| `cosmic-bwarden-tests` | E2E tests against Vaultwarden in Docker |

## Security Invariants (core)
These must never regress. Treat violations as build-blocking bugs.

- **Core dumps**: `libc::prctl(PR_SET_DUMPABLE, 0)` on daemon startup.
- **IPC auth**: Verify every connection via `SO_PEERCRED`. Reject mismatched UIDs.
- **Socket perms**: Create Unix sockets with mode `0600`.
- **Persistent IPC connections**: The agent keeps client connections alive across multiple requests (one `tokio::spawn` per connected socket, inner `loop` for subsequent requests). Subscribe connections are long-lived; all others reuse the same socket until the client disconnects.
- **Sensitive memory**: Use memory-locked storage for all key material and plaintext secrets.
- **No silent failures**: Any operation that can fail and affect data availability, integrity, or security must log at `warn` or `error` level. **Anything that can corrupt or lose data logs `error!` — no exceptions.** Specifically:
  - **Server API failures**: Every non-2xx API response goes through `Client::request_failed` (core, `api/client/mod.rs`), which logs `error!` with method, URL, status, and response body. Never construct `Error::RequestFailed` directly.
  - **Failed vault mutations & sync**: A server rejection of add/update/delete/favorite, or a sync failure, must log `error!` at the handler — the optimistic local change is silently undone by the next sync, which is data loss from the user's perspective.
  - **Decryption**: Every `vault::decrypt` failure must log `warn!` with the entry ID and field name. Never use `.ok()` silently — a cipher string leaking into plaintext position caused a double-encryption incident.
  - **Vault DB persistence** (`db.save`): Log `error!` if save fails. Silent failure means in-memory and on-disk state diverge; data is lost on agent restart.
  - **Keyring operations** (`store_tokens`, `delete_tokens`): Log `error!` if they fail. Silent failure means tokens are lost across restarts or stale tokens survive logout.
  - **Write failures on IPC/browser-host sockets**: Log `error!` if a response cannot be delivered to a client.
  - **Fallbacks from load errors**: If a fallback value is used after a load error (e.g. creating a fresh DB when disk load fails), log `error!` with the underlying error — the fallback can shadow data loss.
  - **Log visibility**: the agent defaults to `info`-level logging when `RUST_LOG` is unset (env_logger's built-in default of `error`-only hid warnings from journalctl). Don't rely on `RUST_LOG` being set in the systemd unit.
  - Use `let _ = expr` only for genuinely fire-and-forget side effects (e.g. removing a stale socket file before rebind) where the next operation will surface any real problem. Add a comment explaining why the error is intentionally discarded.

## Workflow

### Fixing failing tests
1. Run `cargo test -p cosmic-bwarden-tests -- --test-threads=1`, capture full output.
2. Identify root cause from panic/error line — do not guess from test name alone.
3. `cargo check` after each edit before re-running tests.
4. If a fix attempt fails, document why before trying the next approach.
5. Every bug fix requires a corresponding test case.

### Tests must never touch real user state (review-blocking)
`dirs::config_file()`, `db_file()`, `device_id_file()` and friends fall back to
the *live* user paths (`~/.config/cosmic-bwarden/`, `~/.cache/…`) whenever the
`COSMIC_BWARDEN_*` overrides are unset. A unit test that reaches a save path
therefore overwrites the developer's own account. On 2026-08-10 a plain
`cargo test -p cosmic-bwarden-ui` did exactly that: `test_settings_flow` drove
`SettingsSaveClicked` with a `Default` config and replaced a live
`config.json` with all-`null` fields. The running agent kept serving the vault
from memory, so nothing looked broken until the next vault write failed with
`email not set in config` — and only the untouched cache file made recovery
possible.

Rules:
- Any test that can reach `save_legacy()`, `Db::save()`, or a keyring/TPM write
  must redirect the path first. In the UI crate use
  `app/tests/config_env.rs::ConfigFile`; elsewhere set the `COSMIC_BWARDEN_*`
  override explicitly.
- Env overrides are process-global — serialize such tests behind the helper's
  lock rather than hoping the scheduler is kind.
- When adding a config field, ask which process *owns* it. The UI owns only
  what its Settings pane edits; it must read-modify-write the file, never
  persist its whole in-memory struct.

### Adding features
1. Update `preprocess_args` and `--help` (`after_help` with `EXAMPLES:` block) for any CLI change.
2. Follow MVU strictly for UI changes — no logic in view functions.
3. Update `CONTEXT.md` for architectural changes.

### Dispatching agent actions from the UI (review-blocking)
Never decide *which* `Action` to send inside the `async` block handed to
`Task::perform`. An action built in that closure is unreachable from any test
that doesn't spin up an executor and a live agent, so a wrong variant stays
invisible until it hits the server — this shipped once as new entries being
sent via `UpdateEntry`, producing `PUT /ciphers/new-<unix_secs>` and an HTTP
400 that discarded the user's unsaved work.

- **Build the action in a pure function**, then move it into the closure:
  `protocol::entry_save` (core, shared with the E2E suite) and, in the UI,
  `app/update/{vault,auth,generator}_actions.rs`. Unit-test the mapping
  directly. Parameterless status/config queries are exempt — they have no
  decision to get wrong.
- **The applet and main window must share one builder.** Both surfaces send
  lock/logout/unlock/PIN actions; two hand-written copies is how they drift.
  `auth_actions` is the single definition for both.
- **A builder taking a secret by value owns wiping it.** If a decision arm
  sends nothing, hand the secret back (see `UnlockPinIntent::Nothing`) so the
  caller can `zeroize` it — dropping a plain `String` leaves it in freed memory.
- **Tests must assert the emitted action**, not a hand-fed response. Feeding
  `SaveEditResult(Ok(()))` asserts a success the test invented and passes
  against an action the server rejects.
- **Optimistic local mutation must be paired with the matching action** — the
  next sync silently reverts any mismatch, which reads as data loss.
- **Never `take()` an edit buffer before the agent confirms.** Clone it, and
  clear it only on success, so a failed save leaves the user's input on screen.

## Code Organization
- **Target file size: 150–250 lines.** This is the range where edits are reliable and context fits cleanly.
- **Hard limit: 500 lines.** If a file exceeds this, split it before adding more code. No exceptions.
- **One module = one responsibility.** If you find yourself writing "and also" when describing what a file does, it needs splitting.

### Modular Patterns (Mandatory)
When a crate's main logic grows, decompose using these established patterns:
- **`cosmic-bwarden-agent`**: Split into `handler.rs` (request routing), `server.rs` (API interaction), and `logind.rs` (DBus events).
- **`cosmic-bwarden-core`**: 
    - `api/`: Split into `models.rs` (DTOs) and `client.rs` (Network logic).
    - `db/`: Split into `models.rs` (Data structs) and `persistence.rs` (File I/O).
- **`cosmic-bwarden-ui`**: Split into `app/state.rs` (State), `app/update.rs` (MVU logic), and `app/tasks.rs` (Async tasks).

- **When splitting**: prefer extracting into a sibling module (`mod foo;` in the parent) rather than a new crate unless the boundary is a genuine abstraction layer.
- **Before adding to a file**: check its current line count. If it's above 200, consider whether the new code belongs in an existing or new sibling module instead.

## Internationalization (UI)

**Every user-facing string in `cosmic-bwarden-ui` must go through the `fl!` macro** — never a bare string literal in a widget (`text::body`, `button::*`, `secure_input`/`text_input` placeholders, `.title`/`.body`, dialog captions, dropdown entries). This is a review-blocking rule for the UI crate.

- **Where strings live**: `crates/cosmic-bwarden-ui/i18n/en/cosmic_bwarden_ui.ftl` (the fallback locale). Add a kebab-case key there, then reference it with `fl!("my-key")`.
- **Interpolation**: use Fluent placeables, e.g. `pin-min-chars = PIN (min { $count } characters)` called as `fl!("pin-min-chars", count = value)`. Do **not** build display strings with `format!`. Bind ambiguous numeric expressions (e.g. `a / b`) to a typed local first — `FluentValue` conversion can't infer the type inline. Add a `# comment` above the key documenting each `$arg`.
- **Logic keys vs. display labels**: strings used as match/lookup keys (e.g. `EditFieldChanged`/`revealed_fields` field names in `view/vault/detail.rs`) must stay stable literals. Localize only their *display* via a mapping helper (`field_label`) — never the key itself.
- **Not localized**: symbols/glyphs (`—`, `✅`, `…`), the version string, and runtime text already produced by the agent (diagnostics, agent error messages). Compact unit suffixes (`2h`, `90m`) are left numeric by design; only the words around them are keyed.
- **Bidi isolation** is disabled in the loader (`set_use_isolating(false)`), so interpolated values render/compare without U+2068/U+2069 marks.
- **PIN length**: the single source is `cosmic_bwarden_core::MIN_PIN_LEN`; the UI (`crate::MIN_PIN_LEN`) and agent (`tpm_pin::MIN_PIN_LEN`) aliases and the CLI prompt all derive from it. Captions/validation use the constant, never a hardcoded number — a hardcoded "min 4" caption survived one bump already.
- `i18n_embed_fl::fl!` verifies message IDs against the fallback `.ftl` **at compile time** — a typo'd key fails the build, so `cargo check -p cosmic-bwarden-ui` is the guard.

## Tool Discipline

- **Symbol lookup**: `grep_search` first, read full file only if needed.
- **File edits**: `replace` with enough surrounding context for uniqueness. One `replace` per file per turn maximum.
- **State tracking**: `update_topic` on strategic pivots. `MEMO.md` for local/machine-specific notes only.

## Optional Features

### TPM PIN Unlock (`--features tpm`)
Seals the 64-byte vault key (`enc_key_expanded ‖ mac_key_expanded`) in a TPM2 object protected by a user PIN and bound to PCR{0,7} (firmware + Secure Boot state).

- **Agent**: `cargo check -p cosmic-bwarden-agent --features tpm`
- **Module**: `crates/cosmic-bwarden-agent/src/tpm/` — seal/unseal/clear using `tss-esapi 8.0.0-alpha.2` (`mod.rs` API, `policy.rs`, `blob.rs`, `ops.rs`)
- **Handler**: `crates/cosmic-bwarden-agent/src/handler/auth/tpm_pin/` — per-concern handlers (`status`, `setup`, `unlock`, `disable`, `server_credentials`)
- **State**: `tpm_configured` in agent `State`; `tpm_available`/`show_pin_unlock` in UI `CosmicBWardenApp`
- **Blob storage**: `<data_dir>/tpm_sealed_<sha256hex16(server+email)>.bin` — per-account, persisted across reboots
- **Graceful degradation**: if TPM hardware is absent at runtime, `is_available()` returns false, UI hides PIN controls
- **Smoke tests**: `cargo test -p cosmic-bwarden-tests --features tpm-smoke -- tpm --test-threads=1` (requires `swtpm` in PATH; auto-skip when absent)

## Password Generator

Charset-based generation, "last used settings", and a 7-day local history — all agent-side (`crates/cosmic-bwarden-agent/src/handler/generator/`), deliberately its own dispatch group in `handler.rs` (not folded into `handler/vault/`), since generation must work with the vault **locked** and even with **no account configured at all**.

- **Protocol**: `Action::GeneratePassword { settings: Option<GeneratorSettings> }` → `Response::GeneratedPassword { password }`. `Some` persists the settings as the new device-wide "last used" and generates with them; `None` reuses whatever is currently persisted. Every call appends to history. `Action::GetGeneratorSettings` / `Action::GetPasswordHistory` are the read-only counterparts.
- **Algorithm**: `handler/generator/algorithm.rs` — forces one char from each selected charset, fills the rest from the union, shuffles. **Must use `rand::rngs::OsRng`** (via `rand::TryRngCore::unwrap_err()`, since `OsRng` is fallible in rand 0.9) — never `rand::rng()`/`ThreadRng`, and never the seeded `StdRng` used elsewhere in this codebase for deterministic fuzz tests.
- **Settings storage**: `crates/cosmic-bwarden-core/src/generator_settings.rs` — plain JSON at `dirs::generator_settings_file()` (`<data_dir>/generator_settings.json`), device-global (no server/email in the path), *not* folded into `CosmicBWardenConfig` (which is account-shaped and unavailable pre-login).
- **History storage**: `handler/generator/storage.rs` — postcard-encoded `Vec<{created_at, ciphertext}>` at `dirs::generator_history_file()` (`<data_dir>/generator_history.bin`), atomic tmp+rename+0600 (same pattern as `db::persistence::Db::save`). Pruned to 7 days on every read and write — no background sweep.
- **At-rest encryption**: reuses `cipherstring.rs`'s existing `CipherString::encrypt_symmetric`/`decrypt_symmetric` (AES-256-CBC + HMAC-SHA256) with a **locally-generated, device-global key** (`dirs::generator_key_file()`, `<data_dir>/generator_key.bin`, 0600) — not derived from any account's master password, since generation must work standalone. **Threat model**: protects against a different local user, a stray backup, or misconfigured permissions elsewhere reading the file directly. Does **not** protect against another process running as the same local user (the key sits unguarded next to the ciphertext by design) — the same protection level as the vault `Db` JSON cache's 0600-only model, not as strong as anything gated by the master password.
- **Surfaces sharing this**: desktop UI pane (`view/vault/generator.rs`), COSMIC applet quick-gen entry (`view/applet/menu.rs`, works while locked), `cosmic-bwarden-cli generate` (`commands/generator.rs`), browser extension context menu + inline field icon (`content-generate.js`, `docs/browser_integration.md`).
- **Full design**: `docs/password_generator_plan.md`.

## Browser Extension

Source lives in `browser-extension/`. Plain vanilla JS — no bundler, no framework.

| File | Responsibility |
|---|---|
| `background.js` | Native messaging queue, badge count, theme-aware icon switching |
| `background-save.js` | Save-prompt state machine: per-tab pending credentials, save/update decision via `CheckLoginMatch` |
| `content.js` | Form fill injected into pages |
| `content-heuristics.js` | Shared pure DOM helpers (username-field detection, submitted-credential capture); loaded first |
| `content-submit.js` | Login-form submit detection → `LOGIN_SUBMITTED` to background |
| `content-bar.js` | In-page "Save/Update password?" notification bar (open shadow root; never receives the password) |
| `popup/popup.js` | View management, list rendering, fill, domain helpers |
| `popup/popup-lock.js` | Locked-vault view: TPM PIN unlock, DA lockout feedback; loaded *before* `popup.js` — see `docs/browser_integration.md`'s "Script load order gotcha" |
| `popup/popup-detail.js` | Detail view, secret reveal/copy (on-demand via `GetPassword`) |
| `popup/popup-edit.js` | Edit/add form, field rendering |
| `popup/popup.css` | CSS custom properties; dark mode via `prefers-color-scheme` |

**Icons**: `browser-extension/icons` is a symlink to the repo-root `icons/` folder (black*.png for light theme, white*.png for dark). `zip -r` dereferences symlinks, so `pack-extension` embeds them correctly.

**Security invariant**: The detail view uses `GetEntryMeta` (no secrets). Secrets are fetched only on explicit reveal/copy (`GetPassword`/`GetTotp`) or fill (`GetEntry`). Never hold plaintext passwords in JS state from passive browsing.

**Save prompt**: credentials captured at user-initiated form submit are the one exception — they live transiently in the background per-tab pending map (cleared on action/90 s TTL/tab close, never persisted or logged). "Does this login exist / did the password change" is decided inside the agent (`CheckLoginMatch`); the extension never fetches a stored secret to compare, and `SHOW_SAVE_BAR` messages to the page never carry the password. Updates go through `UpdateLoginPassword { id, password }` — never echo a `GetEntryMeta` result through `UpdateEntry`, which wipes notes (redaction sets them `None` and the merge treats `None` notes as a legitimate clear). See `docs/browser_integration.md`.

## Justfile (task runner)

The project uses `just` for all build, install, and test orchestration. Key recipes:

| Recipe | What it does |
|---|---|
| `just build` | Release build of all Rust crates (auto-detects TPM) |
| `just install` | `build` + install binaries + register Firefox native host (system-wide, needs sudo) |
| `just user-install` | Same but installs to `~/.local` (no sudo) |
| `just pack-extension` | Zips `browser-extension/` → `target/cosmic-bwarden-extension.zip` via `packaging/pack-extension.sh` (excludes node_modules, test artifacts; asserts the artifact's shape). **Not part of `build`.** Never inline the zip command elsewhere — CI and the release workflow call the same script. |
| `just register-browser-host` | Registers native host pointing at debug build (dev workflow) |
| `just test` | Full Rust test suite in order: unit → agent → CLI → UI |
| `just test-extension-unit` | Extension JS unit tests (vitest) |
| `just test-extension-e2e` | Extension Playwright E2E (Firefox, mock agent) |
| `just test-extension-e2e-full` | Extension Playwright E2E (Firefox, real agent + Vaultwarden) |
| `just test-extension-e2e-chrome` | Same but Chrome |
| `just restart-panel` | Restart COSMIC panel after install |
| `just enable-agent` | Enable + start agent systemd user service |

When modifying the browser extension, validate with `just test-extension-unit` and `just test-extension-e2e` before reporting done.

## Validation Commands
```
cargo check -p <crate>
cargo check -p cosmic-bwarden-agent --features tpm
cargo test -p cosmic-bwarden-tests -- --test-threads=1
cargo test -p cosmic-bwarden-tests --features tpm-smoke -- tpm --test-threads=1
just test-extension-unit
just test-extension-e2e
just pack-extension
```
