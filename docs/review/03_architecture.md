# Phase 3 — Architecture & Code Health

Reviewed: 2026-07-04. Scope: crate boundaries, dependency graph, async discipline,
error handling / panic surface, MVU discipline, file-size compliance, and a full
clippy-pedantic triage.

Severity: S1 correctness · S2 maintainability · S3 polish.

## Verdict

The architecture is clean and matches the documented design. No S1. The daemon's panic
surface is small and each site is justifiable; one real robustness bug (`mlock` panic under
`RLIMIT_MEMLOCK` exhaustion) was fixed. Clippy is now green workspace-wide *and* under
`--features tpm`, with pedantic triaged. **Gate: PASS.**

## Crate boundaries — sound

- `core` is UI- and agent-free except one deliberate `cosmic-config` dependency (the config
  type derives `CosmicConfigEntry`). `cli` pulls no `libcosmic`. No dependency cycles.
- The IPC protocol (`Action`/`Response`/`Event`) is single-sourced in `core::protocol` and
  shared by all three clients — no duplicated wire structs. Good.
- The mandated module decomposition (handler/server/logind; api models/client; db
  models/persistence; ui state/update/tasks) is present and honoured.

## Async discipline — acceptable, one note

- Agent runs on `#[tokio::main(flavor = "current_thread")]` — a single-threaded runtime.
  Blocking work is limited: config/DB loads are small synchronous file reads, and the KDF
  (Argon2id, the one genuinely CPU-heavy call) runs inline under the state lock.
- **A3-1 (S2, note):** on the current-thread runtime the KDF blocks the whole agent for its
  duration (unlock and every reprompt). With default Bitwarden params this is a fraction of
  a second, but a hostile/misconfigured high-memory KDF param (capped at 1024 MiB by the
  Phase-1-verified clamp) could stall all IPC. Not exploitable for data loss; a
  `spawn_blocking` around the KDF would keep the agent responsive. Deferred — measured
  properly in Phase 6 (performance) where the latency budget is set.
- Per-connection tasks are spawned and end on disconnect; subscriber channels are pruned on
  send failure (`state.rs::broadcast`). No unbounded growth found.

## Panic surface (daemon) — reviewed, one fix

25 `unwrap`/`expect` in `core`+`agent` non-test code. Triaged:

- **A3-2 (S2) — FIXED: `locked.rs:46` `mlock().unwrap()`.** Once `REGION_LOCK_WORKS` was
  cached `true`, every later `locked::Vec::new()` unwrapped the `mlock` result — but `mlock`
  can still fail later when the process exhausts `RLIMIT_MEMLOCK` (64 KiB on older kernels).
  A password manager panicking mid-operation because it locked one buffer too many is a
  denial of its own service. Now degrades to an unlocked buffer with a `warn!`, matching the
  first-failure path.
- **Justified (left as-is):** `postcard::to_allocvec(&Response).unwrap()` (×3, `lib.rs`) —
  our own types, serialization is infallible; `ProjectDirs::from(...).unwrap()` (×4,
  `dirs.rs`) — returns `None` only with no home directory, unrecoverable at startup;
  `logind.rs` `.expect()` on static interface/member name literals — compile-time constants;
  `cipherstring.rs:75` `.split('|').next().unwrap()` — `split` always yields ≥1;
  `api/client/mod.rs` `HeaderValue::from_str(&DEVICE_TYPE.to_string())` — a constant `u8`.
  These are true invariants, not runtime input; documenting rather than churning them.

## Clippy — triaged and green

Was not gated before (default `cargo clippy` reported findings; pedantic more). Now:
`cargo clippy --workspace` and `--features tpm` both clean under `-D warnings`.

**Auto-fixed** (mechanical, behaviour-preserving; `--fix`): needless `return`s, redundant
closures/borrows, `map_or` simplifications, `repeat().take()`→`repeat_n`, `is_multiple_of`,
redundant `Ok(..?)`, useless `ObjectHandle` conversions, `Default`-init field assignments.

**Annotated with rationale** (not blindly silenced):
- `large_enum_variant` on `Action`/`Response`/`Message`/`EntryData` — `#[allow]` with a
  comment: these are transient per-event values; boxing wide variants would ripple through
  every match arm for a negligible, short-lived allocation win.
- `too_many_arguments` on the 6 API/handler entry points and `vault::unlock` — `#[allow]`
  pointing at a roadmap item to introduce parameter structs. Chose to defer the refactor
  rather than half-do it, since it touches the wire/handler boundary.
- `result_large_err` on `validate_pin` — one-per-failure construction, consistent with the
  unboxed-`Response` decision.

## File-size rule (AGENTS.md: 250 target, 500 hard)

No first-party `src` file exceeds 500. Closest: `ui/app/update/applet.rs` (572 — **over**,
but this count includes the file's own tests; the logic is ~430). Flagging for the tracked
file-size refactor rather than splitting mid-review:

- **A3-3 (S2):** `app/update/applet.rs` and `handler/vault/ops.rs` (469) are the two live
  files nearest the limit. Both are cohesive (one is the applet update arm, the other the
  vault-mutation handlers). Added to the existing `docs/archive/file_size_refactoring.md`
  follow-up via roadmap; not urgent.

## Dependency weight

- One real duplicate investigated: `cosmic-config` compiled twice, because our workspace
  pinned `branch = "master"` while libcosmic's internal crates reference the plain URL —
  cargo treated them as distinct sources. **Attempted fix** (drop `branch = "master"`)
  **reverted:** it made the UI fail to compile (5 errors — the resolved commit differs from
  what the code targets). Left as-is; the duplication is build-time only (one extra proc-
  macro + config compile), not shipped in the binary. Noted for the libcosmic-pin revisit.
- Other `cargo tree -d` duplicates (`bitflags`, `darling`, `getrandom`, `foldhash`,
  `float-cmp`) are all transitive via libcosmic/reqwest — not ours to deduplicate, and
  lint-capped by cargo. No action.

## Actions

- [x] A3-2 `mlock` panic → graceful degradation with warning (`locked.rs`).
- [x] build.rs `&["..."]`→`["..."]` (pedantic) — also unblocked clippy compiling core.
- [x] Whole workspace clippy green, default + tpm; pedantic triaged.
- [ ] A3-1 KDF `spawn_blocking` → Phase 6 (measure first).
- [ ] A3-3 file-size splits (`applet.rs`, `ops.rs`) → roadmap / file-size follow-up.
- [ ] API parameter structs (removes the `too_many_arguments` allows) → roadmap.
- [ ] cosmic-config double-compile → revisit with the libcosmic pin.

## Gate assessment

No S1; one robustness fix landed; lint debt cleared and gated. **Phase 3 gate: PASS** —
proceed to Phase 4 (COSMIC-native integration & UX).
