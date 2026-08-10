# Roadmap / Deferred Work

Backlog of health and hardening tasks that are intentionally deferred. Newest
concerns at the top of each section. Convert to issues once the project is on a
forge.

## Pre-publish (before making the repo public)

- [x] **Choose a license** — *done 2026-07 (owner decision: GPL-3.0-only, matching the
      COSMIC ecosystem)*: canonical `LICENSE` at repo root, `license = "GPL-3.0-only"`
      in all five crates, PKGBUILD updated. (Phase 7/8.)
- [ ] **Finalize .deb depends on a Debian host** — `dpkg-shlibdeps` over the manual
      list in `crates/cosmic-bwarden-ui/Cargo.toml [package.metadata.deb]`.
- [ ] **PKGBUILD**: `url`/`source`/`license` filled 2026-08 (repo is at
      `github.com/donatengit/cosmic-bwarden`). Still open: pin the real sha256 once a
      `vYYYY.MM.P` tag exists, then a clean-chroot build (`packaging/PKGBUILD`).
- [ ] **Extension store prep**: Firefox-flavoured manifest (`background.scripts`
      event page) alongside Chrome's `service_worker`; privacy policy; AMO
      source-review notes. Submit early to reserve `cosmic-bwarden@enikeev.com`.
- [x] **Rename the app ID out of System76's namespace** `[U4-1]` — *done 2026-07*:
      ID is now `com.enikeev.cosmic_bwarden` (desktop entry, applet .ron,
      `StartupWMClass`, `CONFIG_ID`/`APP_ID` in core/ui/agent). `config.json`
      needed no migration (keyed by `profile()`, not `CONFIG_ID` — see
      `docs/cosmic_integration.md`); pre-existing local keyring entries under the
      old ID are orphaned and require re-login. Still **temporary** — pick a
      permanent namespace (e.g. a real domain or `io.github.<owner>`) before
      Flatpak/store publishing.
- [x] Scrub git history and working tree for secrets / machine-specific paths. *(Done
      2026-08, before the first push: swept all commits for keys, tokens, `/home/don`
      paths and personal hostnames — only test fixtures found. No submodule or binary
      beyond the eight icon PNGs was ever committed; `.git` is 1.4 MB.)*
- [x] Review `docs/*_plan.md` scratch docs — keep or archive. *(Done 2026-07: completed
      plans moved to `docs/archive/`, see `docs/review/00_ground_truth.md` F3.)*
- [x] Add CI once on a forge (build + test + `clippy`). *(Done 2026-08 with the first
      push: `lint`, `unit`, `audit`, and `extension` run on every push/PR; the Rust E2E
      suite runs nightly and on manual dispatch. The `extension` job also builds the
      distributable zip so packaging breaks surface on push, not mid-release.)*

## Data integrity

- [ ] **The agent should own `config.json` outright** — *raised 2026-08 by the
      settings-clobber incident*. Two processes write the same file with different
      notions of its contents: the agent load-modify-saves, while the UI persisted its
      whole in-memory struct. The immediate fix makes the UI read-modify-write only the
      two fields its Settings pane owns (guarded on `config_loaded`, regression-tested in
      `ui/src/app/tests/settings_persist.rs`), but the durable fix is an `UpdateSettings`
      action so the agent remains the single writer. Needs a `PROTOCOL_VERSION` bump.
- [ ] **`server_name()` should filter empty strings like `base_url()` does** — with the
      server field left blank, `base_url` is `Some("")`, so the vault cache and TPM blob
      are keyed on `""` (`:user@example.com.json`, `tpm_sealed_<sha256("" ‖ 0 ‖ email)>`)
      while `base_url()` reports `api.bitwarden.com`. Any repair that "normalises" the
      config to `None` silently points the agent at a *different* cache file and an
      unfindable TPM blob. Fixing it means migrating existing cache/TPM filenames — do
      not change it without that migration.

## Security / supply chain

Items below tagged `[P1-n]` come from the Phase 1 security review
(`docs/review/01_security.md`, 2026-07-04). No S0/S1 open; these are S2 hardening.

- [ ] **`rsa` Marvin timing sidechannel (RUSTSEC-2023-0071)** — *accepted, not fixed
      (2026-08)*. No patched release exists in the `rsa` 0.9 line, and RSA is reachable
      from all three shipped binaries (core's private-key/org-key unwrap; `ssh-key` via
      `ssh-agent-lib`). Rationale for acceptance is in `.cargo/audit.toml`, summarised
      in SECURITY.md. **Drop the ignore and bump the moment RustCrypto ships a fix** —
      this is the only ignored advisory that reaches shipped code.
- [x] **Dependency auditing** `[P1-5]` — *done 2026-08*: `cargo audit` runs in CI on
      every push (`.github/workflows/ci.yml`, `audit` job), reading the reviewed ignore
      list at `.cargo/audit.toml` so CI and a laptop agree. Each ignore records why the
      advisory is unreachable (build-time-only `quick-xml`; `thirtyfour`/`testcontainers`
      test-only paths; hickory pulled in only by workspace feature unification) and was
      verified with `cargo tree -p <shipped-crate> -e normal -i <crate>`. Still open:
      `cargo-deny` for license + bans, which `cargo audit` does not cover.
- [ ] Pin/track the alpha `tss-esapi` version deliberately; revisit at publish
      time — likely stable by then (owner, 2026-07), so this may reduce to a
      version bump rather than a risk assessment.
- [x] **Enforce `https://` on `base_url`** `[P1-1]` — *done 2026-07*: enforced in
      `api/client/mod.rs::ensure_transport_security`, called from
      `reqwest_client()` — the single funnel every API request passes through,
      so no per-call-site check to forget. `https` always allowed; `http` only
      for loopback (`localhost`, `*.localhost` per RFC 6761, `127.0.0.0/8`,
      `::1`) with a once-per-process `warn!`; anything else (public-host http,
      LAN IPs, missing scheme, other schemes) fails with a dedicated
      `Error::InsecureServerUrl`/`InvalidServerUrl`. Unit-tested including the
      `localhost.example.com` suffix-spoof case.
- [ ] **Constant-time reprompt compare** `[P1-3]` — `handler/vault/query.rs:90` compares
      the master-password hash with `!=`; use `subtle::ConstantTimeEq`. Bounded impact
      (attacker is already same-UID) but cheap. (Was the prior pass's deferred item.)
- [x] **`mlock` session tokens** `[P1-4]` — *done 2026-07*: `Db.access_token`/
      `refresh_token` moved from `String`-backed `Secret` to the new
      `cosmic_bwarden_core::locked::Token` (mlocked buffer, zeroize-on-drop, `Debug`
      redacted, unit-tested), so they can no longer page to swap while the vault is
      unlocked. Oversized tokens (> 4 KiB, `locked::Vec`'s fixed capacity — where
      `ArrayVec::extend` would panic) degrade to a zeroize-on-drop heap `String`
      with a warning, mirroring `locked::Vec`'s own `RLIMIT_MEMLOCK` degradation.
      `From<String>` zeroizes the source, scrubbing the transient copy the token
      arrived in. The `protected_*` fields stay `Secret` deliberately: they're
      ciphertext and are serialized to the on-disk vault cache anyway. **Accepted
      residual**: request-scoped copies (the `String` passed into `with_refresh`'s
      closure, reqwest's `Authorization` header) remain pageable for the seconds a
      request lives — eliminating those would mean threading locked types through
      reqwest, which isn't realistic. Hibernate-image exposure is separately
      mitigated by the logind delay inhibitor (see `[U4-8]`).
- [x] **Serialize token refresh** `[F2-4]` — *done 2026-07*: `with_refresh`
      (server/auth.rs) now takes a per-agent `State::refresh_lock` around the
      refresh step only (not the whole request), with a double-check after
      acquiring it — if a concurrent caller already refreshed while this one
      waited, retry with the now-current access token instead of exchanging the
      (single-use, Vaultwarden-rotated) refresh token a second time.
- [x] **Cap third-party log verbosity** `[P1-7]` — *done 2026-07*: `reqwest`,
      `hyper`, `hyper_util`, `h2`, `rustls` capped at `info` in both the agent's
      env_logger setup (`agent/lib.rs`) and the UI's tracing EnvFilter
      (`ui/main.rs`), regardless of `RUST_LOG` (module directives added after the
      env parse also beat an explicit `RUST_LOG=hyper=trace`). Documented in
      `docs/build_and_run.md`.

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
- [x] ~~KDF off the runtime thread~~ `[A3-1]` — *closed by Phase 6 measurement*
      (docs/review/06_performance.md): worst-case KDF stall is ~131 ms (Argon2id
      defaults, release), once per unlock/reprompt — acceptable on the current-thread
      runtime. Revisit only if the runtime goes multi-thread for other reasons.
- [x] **Upgrade libcosmic to recent master** — *done 2026-07*: bumped `8fa6a01d` →
      `ee5d9659`. Fixed the 5 call-site breaks: `app_popup` gained a leading
      `live_settings: Fn(&App) -> LiveSettings` param (`app/update/applet.rs`,
      passed `|_| Default::default()`); `cosmic_theme::Theme::background` is now
      `background(transparent: bool) -> &Container` instead of a public field
      (`view/style.rs`, passed `false`); `theme::Container::Dialog` is now
      `Dialog(is_overlay: bool)` (`main.rs`, passed `true` for the full-window
      modal case). Full workspace `cargo check`/`cargo test` clean (115 UI +
      26 core/agent/cli tests). `cargo clippy` has 12 pre-existing lint errors
      (`items_after_test_module`, `needless_borrows_for_generic_args` in
      `core/cipherstring.rs`, `field_reassign_with_default` in UI tests) —
      confirmed present on the old pin too, so unrelated to this bump; still
      open as separate maintainability debt. Live `just restart-panel` smoke
      not performed (would touch the running desktop session).
- [ ] **`cargo clippy --all-targets` fails workspace-wide** (12 errors, found while
      validating the libcosmic bump above) — `items_after_test_module` and
      `needless_borrows_for_generic_args` in `core/cipherstring.rs`,
      `field_reassign_with_default` in `ui/app/tests/main_window.rs`. `cargo
      check`/`cargo test` are unaffected (these are clippy-only lints); AGENTS.md's
      "clippy+rustfmt now gated" note (Phase 3) evidently covers `cargo clippy`
      without `--all-targets`, which doesn't lint test code — reconcile before
      relying on clippy as a merge gate.
- [ ] **cosmic-config double-compile** — the workspace pins libcosmic `branch = "master"`
      while its internal crates use the plain git URL, so cargo compiles `cosmic-config`
      (and its derive) twice. Dropping `branch` breaks the UI build (resolves a newer
      commit); fold into the libcosmic upgrade above (dropping `branch = "master"` at
      that point both unifies the source and picks up the new commit).

## UX backlog (Phase 4 review — ranked in docs/review/04_cosmic_ux.md)

- [x] ~~Clipboard auto-clear after copy (UI, applet)~~ `[P1-9]` — *done 2026-07 for
      UI + applet*: every copy (main window `CopyToClipboard`, applet
      username/secret) arms a 30 s timer (`CLIPBOARD_CLEAR_SECS`,
      `ui/app/update/mod.rs`); on expiry the clipboard is read back and wiped
      **only if it still holds the copied value** — content the user copied
      elsewhere meanwhile is never clobbered, and a newer copy supersedes the
      pending clear (generation counter, state-machine unit tests in
      `app/tests/interactions.rs`). Still open: the **browser extension**'s copy
      path (separate JS codebase) has no auto-clear.
- [ ] TOTP code display/copy in UI+applet (agent's GetTotp already works).
- [x] ~~Password generator (applet quick-gen + edit form)~~ — *done 2026-07*:
      full-page generator pane (checkboxes, length slider, reveal/copy,
      7-day local history) at `crates/cosmic-bwarden-ui/src/view/vault/generator.rs`;
      applet quick-gen entry; `cosmic-bwarden-cli generate`; browser extension
      context menu + inline field icon. Settings/history/algorithm live in the
      agent (`handler/generator/`) so every surface shares them. See
      `docs/password_generator_plan.md`.
- [ ] Extension: re-check active tab's domain at fill time `[P1-8]`.
- [ ] Folder/collection navigation in the vault window.
- [ ] Replace emoji button icons (📂🔗🔑) with symbolic icons + tooltips `[U4-4]`.
- [ ] Keyboard: Escape to dismiss, arrow-key list navigation, global shortcut `[U4-5]`.
- [ ] Second locale to prove the Fluent pipeline `[U4-6]`.
- [x] ~~Branded symbolic panel icon~~ `[U4-7]` — *done 2026-07 for the applet
      button*: `resources/icons/cosmic-bwarden-symbolic.svg` embedded via
      `icon::from_svg_bytes(...).symbolic(true)` + `icon_button_from_handle`
      (see `docs/cosmic_integration.md`), no icon-theme install needed. The
      `.desktop` `Icon=` field (window/dock context) now points at
      `com.enikeev.cosmic_bwarden-symbolic` (the simplified monochrome mark,
      installed into hicolor `scalable/apps/`; the full-color
      `icons/black.svg` + raster set is installed alongside as
      `com.enikeev.cosmic_bwarden` by `just install`/`clean-install`/
      `user-install`). The `.ron` `icon:` field (panel-applet listing in COSMIC
      Settings) also uses the branded `com.enikeev.cosmic_bwarden-symbolic`
      (installed alongside the full-color set) — rendered small/recolored
      like the panel button.
- [x] ~~Read PrepareForSleep's bool arg; don't re-lock on resume~~ `[U4-8]` — *done
      2026-07*: `logind.rs` now deserializes the bool from
      `PrepareForSleep`/`PrepareForShutdown` (fails toward locking), skips the
      spurious re-lock on resume, and additionally takes a logind **delay
      inhibitor** (`Inhibit("sleep:shutdown", …, "delay")`) so the zeroize is
      guaranteed to finish before a hibernate image of RAM hits disk — released
      after lock, re-armed on resume; degrades to the old racy behavior with a
      warning if polkit denies it.
- [ ] **State-dependent applet panel icon** — every stateful official COSMIC
      applet (`cosmic-applet-network`, `-bluetooth`, `-audio`, `-battery` in
      `tmp_code_examples/cosmic_examples/cosmic-applets`) recomputes its icon
      name from current state and swaps it on every relevant change (e.g.
      bluetooth enabled/disabled, audio mute/volume-tier). Ours
      (`view/applet/mod.rs`) always renders the same static embedded
      `APPLET_ICON_SVG` regardless of locked/unlocked/needs-login/sync-failed
      state. No COSMIC applet has a "locked" concept to copy an icon name
      from, so this needs a freeform choice (e.g. a padlock overlay or dimmed
      variant when locked) — the applicable *pattern* from the examples is
      "state field → icon lookup," not a specific icon name.
- [ ] **`.desktop`/`.ron` panel-overflow metadata** — 17 of ~18 official
      applets set `X-CosmicShrinkable=true` plus a numeric `X-OverflowPriority`
      (verified against `cosmic-panel-bin/src/space/wrapper_space.rs` in
      `tmp_code_examples/cosmic_examples/cosmic-panel`, current as of
      2026-07-24); ours sets neither, so on a crowded panel we get whatever
      default behavior applies to unmarked applets instead of a deliberate
      choice. Add both fields to
      `crates/cosmic-bwarden-ui/resources/com.enikeev.cosmic_bwarden.desktop`
      and the justfile's inline `.ron` generation (mirrors the `.desktop` file
      per `docs/cosmic_integration.md`).
- [ ] Test stricter systemd sandboxing (ProtectHome + ReadWritePaths, ProtectSystem=strict,
      SystemCallFilter) against keyring/TPM/network, then adopt.
- [ ] Research: Wayland autotype; passkeys/FIDO2; attachments.

## Test coverage

- [ ] Grow the in-crate unit layer for pure logic (crypto, parsing, path
      handling). Started: `cipherstring`, `protocol`, `identity` (KDF params),
      `dirs` (path-traversal encoding). Still thin: `vault::decrypt` edge cases,
      `api` model (de)serialization round-trips.
