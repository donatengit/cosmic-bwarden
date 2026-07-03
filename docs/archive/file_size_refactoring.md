# File-Size Refactoring Plan

`AGENTS.md` sets a **500-line hard limit** (150–250 target). These files are over
and need splitting. Each split keeps one responsibility per module and preserves
the existing public function names (call sites unchanged). Ordered by priority.

> An older, now-completed plan lives in
> [`file_size_refactoring_tests.md`](file_size_refactoring_tests.md).

## ✅ 1. `handler/auth/tpm_pin.rs` — 724 → `tpm_pin/` (DONE)

Split into `handler/auth/tpm_pin/`: `mod.rs` (shared helpers + re-exports),
`status.rs`, `setup.rs`, `unlock.rs`, `disable.rs`, `server_credentials.rs`
(all ≤221 lines). Per-function `#[cfg(feature = "tpm")]` gating preserved.

## ✅ 2. `src/tpm.rs` — 567 → `tpm/` (DONE)

Split into `tpm/`: `mod.rs` (open_context, is_available, da_status, clear,
diagnostics + public re-exports), `policy.rs` (templates + PCR/auth policy),
`blob.rs` (v2 on-disk format), `ops.rs` (seal/unseal), `tests.rs` (all ≤151
lines). Whole module stays gated at `#[cfg(feature = "tpm")] mod tpm;`.

## ✅ 4a. `tests/src/tpm_lifecycle.rs` — 919 → `tpm_lifecycle/` (DONE)

Split into `tpm_lifecycle/`: `mod.rs` (shared fixtures/helpers, re-exported),
`full_lifecycle.rs`, `errors_and_setup.rs`, `cycles.rs`,
`server_credentials.rs` (all ≤249 lines). Gate `#[cfg(all(test, feature =
"tpm-smoke"))]` unchanged.

## 3. `cosmic-bwarden-ui/src/app/update/applet.rs` — 561 (TODO, non-TPM)

`update_applet` is one large `match`. Delegate groups to submodules under
`update/applet/`, each exposing `fn handle(&mut self, msg) -> Option<Task>` for
its subset; the top-level match becomes a short dispatcher:

- **`lifecycle.rs`** — icon/surface/exit, lock/logout-and-quit, open-vault, token.
- **`unlock.rs`** — `AppletUnlock*`, `AppletPin*`, reprompt, reveal toggles,
  `AppletUseMasterPasswordInstead`.
- **`list.rs`** — `AppletSearch*`, `AppletCopy*`, `AppletOpen*`, secret received.
- **`tpm.rs`** — `TpmSetup*`, `TpmDisable*`.

(Note: view code already lives in `view/applet/`; this is the update half.)

## 4b. UI test files (TODO, non-TPM, target 150–250)

- **`cosmic-bwarden-ui/src/app/tests/lifecycle.rs` — 536** → separate the TPM
  state-machine tests (already a marked section) into `tests/tpm.rs`.
- **`cosmic-bwarden-ui/src/app/tests/applet.rs` — 443** → split rendering tests
  from interaction/quit-menu tests.

## Execution notes

- One file at a time; `cargo check` (warnings are denied) + `cargo test` after each.
- Pure `mod` moves — no behavior change, so existing tests are the safety net.
- Update `CONTEXT.md` module map if it enumerates these files.
