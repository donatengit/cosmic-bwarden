# cosmic-bwarden: Agent Guidelines

Instructions and hard rules only. No explanations, no design notes, no
inventories — those live in the guides below.

## Guiding documents

- [`README.md`](README.md) — project overview and the index of every document
  in `docs/`. It is the only place that links them. This file names a document
  in plain backticks when a rule needs it; those paths are listed there. README
  links back here.
- [`CONTEXT.md`](CONTEXT.md) — architecture: crates, module layout, data flow,
  protocol compatibility.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — how to contribute, and the full list of
  gates a change has to pass.
- [`SECURITY.md`](SECURITY.md) — how to report a vulnerability.

A rule belongs in this file when breaking it can lose data, leak a secret,
break someone else's build, or force a re-discovery of a bug that already
shipped. Everything explanatory belongs in the guides.

## Golden Rules

- **Never ask for confirmation.** Apply fixes, run validation, iterate until passing. Report only on final outcome or exhausted options.
- **Never circle back to a failed approach.** If a fix didn't work, note why and move forward.
- **One responsibility per file.** If a file exceeds ~250 lines, it needs splitting.
- **cargo check before cargo test.** Don't run expensive tests against code that won't compile.
- **Run every test through `just`, never `cargo test` or a bare script.** The
  recipes carry the container-socket setup, the debug-binary rebuild the E2E
  harness depends on, and the resource caps that keep the machine usable during
  a 20+ minute run. `just test` is the whole Rust suite; `just test-all` adds
  the offline JS suites; `just --list` shows the rest. Testing strategy and
  resource limits: `docs/testing.md`.
  - **If the test suite needs something new, add it to the justfile in the same
    change.** A new test module, feature flag, service, or fixture that only
    runs from a hand-typed command line is not wired up. The standard is that a
    human can run the full suite at any moment with one `just` command and no
    prior knowledge. A recipe that covers only part of a suite must say so in
    its comment: `just test` once used name filters that silently left 41 of 90
    E2E tests unrun.
- **Write all text in plain technical English.** This covers everything you
  produce: chat replies, commit messages, code comments, doc prose, log lines,
  error strings, and identifiers. State what a thing does, why it exists, and
  what breaks without it. No marketing register, no filler adjectives
  ("powerful", "seamless", "robust", "comprehensive"), no praise of the code or
  the reader, no emoji outside data payloads, no rhetorical questions. Prefer
  short declarative sentences and concrete nouns; name the file, the function,
  the flag, the exact error. When something is uncertain, say so and say what
  would settle it — do not hedge with vague qualifiers. Report failures and
  gaps directly, including your own.
- **Never commit generated or temporary artifacts.** Build output, `node_modules`, test-runner results (`test-results/`, `playwright-report/`), coverage, and logs belong in `.gitignore`, never in a commit. If `git status` shows a generated path, add it to `.gitignore` rather than staging it (note: a slash-suffixed pattern matches directories only — drop the slash to also catch symlinks).

## Naming (one spelling per layer)

Keep each layer in its own lane:

| Layer | Canonical form | Where |
|---|---|---|
| Display name (user-visible) | `COSMIC BWarden` | `.desktop` `Name=`, applet `.ron` `name:`, AppStream `<name>`, `app-title`/`welcome-title` in the FTL, systemd `Description=`, extension `manifest.json` `name`, prose in README/docs |
| Project / binary / package | `cosmic-bwarden` | repo, crate names, `cosmic-bwarden-{agent,cli,ui,core,tests}`, deb/AUR package |
| App ID | `com.enikeev.cosmic_bwarden` | `APP_ID`, `.desktop`/`.ron`/metainfo filenames, D-Bus path, `StartupWMClass`, native-messaging host name |
| Code identifiers | `CosmicBWarden*` | `CosmicBWardenApp`, `CosmicBWardenConfig`, `CosmicBWardenFlags` — Rust types, never rename to match the display name |

`com.system76.CosmicBWarden` appears only in the justfile's legacy-cleanup
`rm -f` lines; it is a historical app ID, not a name — leave it byte-for-byte.

The project URL is `cosmic_bwarden_core::HOMEPAGE` on the Rust side. The
non-Rust manifests (metainfo, systemd units, `manifest.json`, `package.json`,
PKGBUILD) carry their own copy and must be updated together if the repo moves.

## Versioning

- **Build version**: `YYYY.MM-N-<git_id>` generated in `core/build.rs`. Reused across crates via a 30-second `target/build_version.txt` cache.
- **Protocol version**: Independent of the build version — `cosmic_bwarden_core::PROTOCOL_VERSION`, a small integer string bumped ONLY on breaking wire-protocol changes (adding a field to a postcard-encoded `Action`/`Response` variant counts). `Response::Version` always includes both `version` and `protocol_version` fields.
- **Compatibility check**: Pure function `check_protocol_compatibility()` in the CLI crate compares local version against agent's `protocol_version`. Unit-tested for both match and mismatch scenarios.
- **Adding a version subcommand**: Always add `Commands::Version` to the CLI's enum, route it to the auth handler, and include the `check_protocol_compatibility()` call. Update `preprocess_args` if the new command name conflicts with type keywords.
- **Breaking protocol changes**: Bump the `protocol_version` in `Response::Version` by updating `check_protocol_compatibility` expectations if the protocol surface changes incompatibly.

## Security Invariants (core)

These must never regress. Treat violations as build-blocking bugs.

- **Core dumps**: `libc::prctl(PR_SET_DUMPABLE, 0)` on daemon startup.
- **IPC auth**: Verify every connection via `SO_PEERCRED`. Reject mismatched UIDs.
- **Socket perms**: Create Unix sockets with mode `0600`.
- **Persistent IPC connections**: The agent keeps client connections alive across multiple requests (one `tokio::spawn` per connected socket, inner `loop` for subsequent requests). Subscribe connections are long-lived; all others reuse the same socket until the client disconnects.
- **Sensitive memory**: Use memory-locked storage for all key material and plaintext secrets.
- **Plaintext secrets cross IPC as `db::Secret`, never `String`** (review-blocking). `Secret` is `ZeroizeOnDrop`, so the plaintext is scrubbed when the response is dropped instead of lingering in freed heap. It is `#[serde(transparent)]`, so this costs nothing on the wire — `protocol::tests::secret_fields_are_wire_identical_to_plain_strings` pins that, and no version bump is needed to adopt it. Applies to `Response::{Password, Totp, GeneratedPassword}` and `GeneratorHistoryEntry::password`; propagate it through client state too (the UI's `OnDemandPayload` and secret-carrying `Message` variants) rather than unwrapping at the IPC boundary, which only moves the leak inward.
  - **`Secret`'s `Display` prints `********`.** Converting a field from `String` to `Secret` silently changes any `println!("{x}")` from the value to asterisks — it compiles clean and the compiler says nothing. This shipped once: `cosmic-bwarden-cli generate` printed asterisks. Where a command's purpose *is* to emit the value (CLI stdout, clipboard), call `.expose()` explicitly and say why in a comment.
  - Legitimate conversion points are the clipboard and the `secure_input` widget — both plaintext by nature. Everything upstream of them stays wrapped.
  - `Secret` also wraps *ciphertext* in this codebase (`protected_key`, `protected_org_keys`). Those need no zeroization; "it's a `Secret`" does not by itself mean "sensitive in memory".
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

1. Run the failing suite through `just` (`just test-e2e` for the E2E crate, `just test` for everything), capture full output.
2. Identify root cause from panic/error line — do not guess from test name alone.
3. `cargo check` after each edit before re-running tests.
4. If a fix attempt fails, document why before trying the next approach.
5. Every bug fix requires a corresponding test case.

### Tests must never touch real user state (review-blocking)

`dirs::config_file()`, `db_file()`, `device_id_file()` and friends fall back to
the live user paths (`~/.config/cosmic-bwarden/`, `~/.cache/…`) whenever the
`COSMIC_BWARDEN_*` overrides are unset, so a test that reaches a save path
overwrites the developer's own account. Full rationale and the verification
recipe: `docs/test_cleanup_plan.md`.

- Any test that can reach `save_legacy()`, `Db::save()`, or a keyring/TPM write
  must redirect the path first. In the UI crate use
  `app/tests/config_env.rs::ConfigFile`; elsewhere set the `COSMIC_BWARDEN_*`
  override explicitly.
- Env overrides are process-global — serialize such tests behind the helper's
  lock rather than hoping the scheduler is kind.
- When adding a config field, ask which process *owns* it. The UI owns only
  what its Settings pane edits; it must read-modify-write the file, never
  persist its whole in-memory struct.
- **Tests must clean up their own state, and never inherit their environment.**
  Any test or script that spawns the agent or CLI must pass an **explicit**
  `COSMIC_BWARDEN_PROFILE`, `HOME`, and **all four** `XDG_*` vars — never
  inherit them. A partial set is not partial safety: `directories` falls back
  to the passwd entry when `$HOME` is unset, so a missing `XDG_DATA_HOME` still
  lands in the real `~/.local/share` even under `env_clear()`. Test profiles
  must be named `test-*`. Each test/script then removes the
  `cosmic-bwarden-<profile>` dirs it created — config, cache, data, **and
  runtime** — and restores any real user file it overwrote (browser
  native-messaging manifests especially) in its own teardown path (Rust `Drop`,
  script `cleanup()` trap; `wait` for the agent first, or its shutdown writes
  recreate what you deleted). Leaving residue, or leaving a developer's browser
  pointed at a test profile, is a review-blocking regression — not fixable by a
  manual sweep or a `clean-test-data` command.
  - **Never compute a deletion path from `dirs::`** (`cache_dir()`,
    `data_dir()`, …). They read process-global env, so an unset profile
    resolves to the live `cosmic-bwarden` profile and the "cleanup" erases the
    developer's real vault cache. Derive removal paths only from the test's own
    recorded temp roots, and assert the target is under them.
  - Shell teardown goes through `cleanup_profile()` in
    `tests/browser-extension/cleanup.sh`, which refuses an empty name, a
    non-`test-*` profile, and any name containing a separator or `..`.
  - **Removing a container must also remove its anonymous volumes** — pass
    `v: true` in `RemoveContainerOptions`, or the volume outlives the container
    in `~/.cache/podman/storage/volumes` forever.
  - **Every `kill()` needs a matching `wait()`.** An unreaped child stays a
    zombie for the rest of the test binary's life.
  - `just clean-test-residue` lists leftover `cosmic-bwarden-test-*` dirs and
    `clean-test-residue-apply` removes them. This is a recovery tool for a
    SIGKILLed run, not part of the loop: a clean run leaves nothing, and if one
    does not, fix the test.

### Builds and test runs must be resource-capped

A full run takes over twenty minutes and would otherwise saturate the machine.
Keep both layers; full explanation: `docs/testing.md`.

- **Layer 1, everything `just` starts**: each build and test recipe runs its
  command through `packaging/run-limited.sh`, which wraps it in a transient
  systemd scope. One budget covers both: the `test_cpus` (default 6) and
  `test_memory` (default 8G) justfile variables, and `0` for both disables
  everything, scope included. Memory uses `MemoryHigh` (throttle) rather than
  `MemoryMax`, so a spike slows the run instead of OOM-killing a browser
  mid-test. The scope also lowers the CPU and I/O share and the nice level, so
  a run yields to the foreground session instead of only being rate-limited.
  Any new build or test recipe must go through it too.
- **Layer 2, containers**: rootless podman puts each container in a scope that
  is a sibling of the test scope, so it inherits nothing from layer 1.
  Containers are capped separately at 2 CPUs / 1024 MB by fixed constants in
  `crates/cosmic-bwarden-tests/src/container_limits.rs`,
  `tools/run_vaultwarden.sh`, and `tests/browser-extension/run-chrome-e2e.sh` —
  keep the three in step.
- **Rust suites**: call `container_limits::apply(container.id(), "<label>")`
  immediately after `.start()`. testcontainers 0.23 has no create-time resource
  API, so the cap goes on afterwards via bollard's `update_container`.
- **Shell scripts**: pass `--cpus` / `--memory` to `docker run` (both runtimes
  honour these at create time).
- **Never use `NanoCpus`.** Podman's Docker-compat API accepts it on the update
  endpoint, answers 200, and ignores it — `cpu.max` stays `max`. Only
  `CpuQuota` + `CpuPeriod` work. `container_limits::tests::cpu_and_memory_caps_
  reach_the_cgroup` reads the value back and fails if the cap did not land;
  keep that test whenever touching this code.
- Capping is best-effort — a runtime that rejects the update logs and continues,
  because no test asserts on the limit.

### Adding features

1. Update `preprocess_args` and `--help` (`after_help` with `EXAMPLES:` block) for any CLI change.
2. Follow MVU strictly for UI changes — no logic in view functions.
3. Update `CONTEXT.md` for architectural changes.

### Dispatching agent actions from the UI (review-blocking)

Never decide *which* `Action` to send inside the `async` block handed to
`Task::perform`. An action built in that closure is unreachable from any test
that doesn't spin up an executor and a live agent, so a wrong variant stays
invisible until it hits the server. `docs/testing.md` records what that cost
the last time it happened.

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
- **Not localized**: symbols/glyphs (`—`, `…`), the version string, and runtime text already produced by the agent (diagnostics, agent error messages). Compact unit suffixes (`2h`, `90m`) are left numeric by design; only the words around them are keyed.
- **Bidi isolation** is disabled in the loader (`set_use_isolating(false)`), so interpolated values render/compare without U+2068/U+2069 marks.
- **PIN length**: the single source is `cosmic_bwarden_core::MIN_PIN_LEN`; the UI (`crate::MIN_PIN_LEN`) and agent (`tpm_pin::MIN_PIN_LEN`) aliases and the CLI prompt all derive from it. Captions/validation use the constant, never a hardcoded number — a hardcoded "min 4" caption survived one bump already.
- `i18n_embed_fl::fl!` verifies message IDs against the fallback `.ftl` **at compile time** — a typo'd key fails the build, so `cargo check -p cosmic-bwarden-ui` is the guard.

## Tool Discipline

- **Symbol lookup**: `grep_search` first, read full file only if needed.
- **File edits**: `replace` with enough surrounding context for uniqueness. One `replace` per file per turn maximum.
- **State tracking**: `update_topic` on strategic pivots. `MEMO.md` for local/machine-specific notes only.

## Optional Features

### TPM PIN Unlock (`--features tpm`)

Seals the vault symmetric keys in a TPM2 object protected by a user PIN and
bound to PCR{0,7} (firmware + Secure Boot state). Design, blob format, blob
paths, and the code layout: `docs/tpm.md`.

- **Check it with the feature on**: `cargo check -p cosmic-bwarden-agent --features tpm`. TPM code is invisible to a plain `cargo check` and to `cargo test`.
- **Seal the vault keys, never `identity.keys`.** `handle_unlock_with_pin` uses
  the unsealed bytes directly as `state.keys`, so sealing the KDF keys would
  decrypt nothing. This shipped once as a mismatch between the two setup paths.
- **Never persist the master-password hash (review-blocking).** It is derived
  per unlock, handed to `Client::login`, and dropped — no `State` field, no TPM
  blob, no envelope. Reprompt verification proves the password by decrypting
  `protected_key` (`handler/vault/query.rs`), which needs no stored hash and
  works after a PIN unlock too. `reauth::tests::no_master_password_hash_is_ever_persisted`
  pins this.
- **Graceful degradation**: if TPM hardware is absent at runtime,
  `is_available()` returns false and the UI hides the PIN controls.
- **Smoke tests**: `just test-tpm-smoke` (needs `swtpm` in PATH; auto-skips when
  absent). The recipe is not part of `just test`.
- **Never let a TPM test reach real hardware (review-blocking)** — the suite
  silently did once, and drove a developer's actual TPM into dictionary-attack
  lockout. Keep both properties:
  - `open_context` must **fail closed**: an explicitly configured TCTI that
    cannot be opened is an error, never a fall-through to `/dev/tpmrm0`. Note
    `TctiNameConf::from_environment_variable()` reads
    `TPM2TOOLS_TCTI`/`TCTI`/`TEST_TCTI`, **not** `TSS2_TCTI` — that one is read
    explicitly.
  - tpm2-tss's swtpm TCTI derives the control socket as `<data-socket>.ctrl`
    (the `%s.ctrl` format string in `libtss2-tcti-swtpm`). `start_swtpm` must
    name them `swtpm.sock` / `swtpm.sock.ctrl`; any other pairing makes
    `Context::new` fail.
  - The agent logs `TPM: using TCTI from TSS2_TCTI (…)` at debug — grep for it
    to confirm a test run is on the emulator.

### Password Generator

Generation must work with the vault **locked** and even with **no account
configured at all**, so it stays its own dispatch group in `handler.rs`, not
folded into `handler/vault/`. Full design, storage paths, and the at-rest
threat model: `docs/password_generator_plan.md`.

- **The algorithm must use `rand::rngs::OsRng`** (via
  `rand::TryRngCore::unwrap_err()`, since `OsRng` is fallible in rand 0.9) —
  never `rand::rng()`/`ThreadRng`, and never the seeded `StdRng` used elsewhere
  in this codebase for deterministic fuzz tests.
- **Settings are device-global** and must not be folded into
  `CosmicBWardenConfig`, which is account-shaped and unavailable pre-login.
- **History is encrypted at rest and pruned to 7 days on every read and write**
  — no background sweep. Persist it atomically (tmp+rename) with mode `0600`,
  the same pattern as `db::persistence::Db::save`.
- Every surface shares one settings set and one history: desktop pane, applet
  quick-gen (works while locked), `cosmic-bwarden-cli generate`, and the browser
  extension. Do not fork the storage per surface.

### Browser Extension

Plain vanilla JS — no bundler, no framework. File layout, message flow,
protocol translation, and the script-load-order gotcha:
`docs/browser_integration.md`.

- **Security invariant**: the detail view uses `GetEntryMeta` (no secrets).
  Secrets are fetched only on explicit reveal/copy (`GetPassword`/`GetTotp`) or
  fill (`GetEntry`). Never hold plaintext passwords in JS state from passive
  browsing.
- **Save prompt**: credentials captured at user-initiated form submit are the
  one exception — they live transiently in the background per-tab pending map
  (cleared on action/90 s TTL/tab close, never persisted or logged). "Does this
  login exist / did the password change" is decided inside the agent
  (`CheckLoginMatch`); the extension never fetches a stored secret to compare,
  and `SHOW_SAVE_BAR` messages to the page never carry the password.
- **Updates go through `UpdateLoginPassword { id, password }`** — never echo a
  `GetEntryMeta` result through `UpdateEntry`, which wipes notes (redaction sets
  them `None` and the merge treats `None` notes as a legitimate clear).
- **Locked-vault save prompt**: a submission made while the vault is locked is
  *deferred*, not dropped. Two rules keep that from silently losing the
  credential it exists to protect: the locked bar's 30 s auto-dismiss must
  **not** send `dismiss` (that clears the pending — only an explicit click
  may), and the 90 s TTL is **restarted once** at deferral, since the original
  window runs from the form submit and unlocking easily outlives what's left of
  it.
- **Validate with `just test-extension-unit` and `just test-extension-e2e`**
  before reporting done; `just pack-extension` for anything that changes the
  shipped file set.

## Validation

```sh
cargo check -p <crate>
cargo check -p cosmic-bwarden-agent --features tpm    # tpm code is invisible otherwise
just test                                            # whole Rust suite
just test-tpm-smoke                                  # TPM suite; auto-skips without swtpm
just test-extension-unit
just test-extension-e2e
just pack-extension
just test-ext-release
```

The full gate list (fmt, clippy, `--all-features` checks) is in
`CONTRIBUTING.md`.
