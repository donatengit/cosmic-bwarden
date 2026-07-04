# Roadmap / Deferred Work

Backlog of health and hardening tasks that are intentionally deferred. Newest
concerns at the top of each section. Convert to issues once the project is on a
forge.

## Pre-publish (before making the repo public)

- [ ] **Rename the app ID out of System76's namespace** `[U4-1]` —
      `com.system76.CosmicBWarden` (desktop entry, applet .ron, StartupWMClass,
      `CONFIG_ID` in core) must become a namespace we control, e.g.
      `io.github.<owner>.CosmicBWarden`, with a config migration for the old ID.
      Blocks Flatpak/store publishing.
- [ ] Scrub git history and working tree for secrets / machine-specific paths.
- [x] Review `docs/*_plan.md` scratch docs — keep or archive. *(Done 2026-07: completed
      plans moved to `docs/archive/`, see `docs/review/00_ground_truth.md` F3.)*
- [ ] Add CI once on a forge (build + test + `clippy`). Note: warnings are
      already denied at build time via `[workspace.lints]`, so CI mainly needs to
      run the suite and clippy.

## Security / supply chain

Items below tagged `[P1-n]` come from the Phase 1 security review
(`docs/review/01_security.md`, 2026-07-04). No S0/S1 open; these are S2 hardening.

- [ ] **Dependency auditing** `[P1-5]` — add `cargo-deny` (advisories + license + bans)
      and wire `cargo audit` into the build/CI. Priority because this is a password
      manager and it pulls `tss-esapi 8.0.0-alpha.2` (an alpha crate handling key
      material). Track advisories against the whole tree, not just direct deps.
- [ ] Pin/track the alpha `tss-esapi` version deliberately; revisit when a stable
      release lands.
- [ ] **Enforce `https://` on `base_url`** `[P1-1]` — `config.rs` accepts any scheme and
      appends `/api`; an `http://` server sends the master-password hash and bearer
      tokens in cleartext. Require https except for loopback hosts
      (`localhost`/`127.0.0.0/8`/`::1`); `warn!` on a non-loopback http fallback.
- [ ] **Constant-time reprompt compare** `[P1-3]` — `handler/vault/query.rs:90` compares
      the master-password hash with `!=`; use `subtle::ConstantTimeEq`. Bounded impact
      (attacker is already same-UID) but cheap. (Was the prior pass's deferred item.)
- [ ] **`mlock` session tokens** `[P1-4]` — access/refresh tokens and `protected_key`
      live in a plain `String`-backed `Secret` (`db/models.rs`), so they can page to
      swap. Move token storage onto `locked::Vec`, or document the residual as accepted
      for encrypted-swap setups.
- [ ] **Serialize token refresh** `[F2-4]` — `with_refresh` (server/auth.rs) has no mutual
      exclusion; two concurrent 401s can both exchange the refresh token, and Vaultwarden
      rotates them, so the loser may persist a stale token. Guard the refresh block with a
      `tokio::sync::Mutex` (double-check token freshness after acquiring).
- [ ] **Cap third-party log verbosity** `[P1-7]` — `reqwest`/`hyper`/`rustls` at
      `RUST_LOG=trace` can emit `Authorization: Bearer` headers. Add a default filter that
      caps those crates at `info`, and/or a warning in `docs/build_and_run.md`.

## Maintainability

- [ ] **File-size refactoring** — several files exceed the 500-line hard limit in
      `AGENTS.md`. Concrete split plan: [`docs/archive/file_size_refactoring.md`](archive/file_size_refactoring.md).
      Phase 3 `[A3-3]` update: no first-party file is over 500 *excluding tests*;
      `ui/app/update/applet.rs` (572 incl. its tests) and `handler/vault/ops.rs` (469)
      are the two to split first.
- [ ] **API/handler parameter structs** — `handle_login`, `handle_add_card`,
      `handle_add_identity`, `Client::{add_cipher,add_ssh_key,add_card,add_identity,
      update_cipher}`, and `vault::unlock` take 8–15 positional args (clippy
      `too_many_arguments`, currently `#[allow]`'d with a pointer here). Introduce
      `AddCardParams`-style structs at the wire/handler boundary; removes the allows.
- [ ] **KDF off the runtime thread** `[A3-1]` — the agent is `current_thread`; Argon2id
      runs inline under the state lock, briefly blocking all IPC on unlock/reprompt. Wrap
      in `spawn_blocking` once Phase 6 sets the latency budget.
- [ ] **Upgrade libcosmic to recent master** — the workspace is locked to commit
      `8fa6a01d`; current master (`ee5d9659`+) has API changes that break the UI build
      (~5 errors seen when the pin floated during the Phase 3 dedup attempt: E0061
      arg-count changes, E0277/E0616/E0631). Plan: bump Cargo.lock deliberately, fix the
      UI call sites, run the full UI test suite + `just restart-panel` smoke. Do this
      before Phase 7 packaging so we ship against a current toolkit.
- [ ] **cosmic-config double-compile** — the workspace pins libcosmic `branch = "master"`
      while its internal crates use the plain git URL, so cargo compiles `cosmic-config`
      (and its derive) twice. Dropping `branch` breaks the UI build (resolves a newer
      commit); fold into the libcosmic upgrade above (dropping `branch = "master"` at
      that point both unifies the source and picks up the new commit).

## UX backlog (Phase 4 review — ranked in docs/review/04_cosmic_ux.md)

- [ ] Clipboard auto-clear after copy (UI, applet, extension) `[P1-9]` — top parity gap.
- [ ] TOTP code display/copy in UI+applet (agent's GetTotp already works).
- [ ] Password generator (applet quick-gen + edit form).
- [ ] Extension: re-check active tab's domain at fill time `[P1-8]`.
- [ ] Folder/collection navigation in the vault window.
- [ ] Replace emoji button icons (📂🔗🔑) with symbolic icons + tooltips `[U4-4]`.
- [ ] Keyboard: Escape to dismiss, arrow-key list navigation, global shortcut `[U4-5]`.
- [ ] Second locale to prove the Fluent pipeline `[U4-6]`.
- [ ] Branded symbolic panel icon installed into the icon theme `[U4-7]`.
- [ ] Read PrepareForSleep's bool arg; don't re-lock on resume `[U4-8]`.
- [ ] Test stricter systemd sandboxing (ProtectHome + ReadWritePaths, ProtectSystem=strict,
      SystemCallFilter) against keyring/TPM/network, then adopt.
- [ ] Research: Wayland autotype; passkeys/FIDO2; attachments.

## Test coverage

- [ ] Grow the in-crate unit layer for pure logic (crypto, parsing, path
      handling). Started: `cipherstring`, `protocol`, `identity` (KDF params),
      `dirs` (path-traversal encoding). Still thin: `vault::decrypt` edge cases,
      `api` model (de)serialization round-trips.
