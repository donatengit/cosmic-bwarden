# Phase 5 — Testing & CI

Reviewed: 2026-07-04. Scope: invariant-coverage gap analysis, flakiness measurement,
CI bootstrap, parser-hardening property tests.

## Verdict

The E2E suite is broad and genuinely exercises real protocol flows, but three security
invariants had no regression test and the suite carried a measurable container-flake rate
with two fixable root causes (fixed). CI is authored and ready; it cannot run until the
repo is pushed to a forge (no git remote exists yet). **Gate: PASS** — with the caveat
that "CI green" is deferred to first push by necessity, not by choice.

## Flakiness measurement (baseline: two full runs + subsets)

| Run | Result | Failures (all rerun-clean) |
|-----|--------|----------------------------|
| Full #1 (Phase 0) | 50/52, 1062 s | `version` (stale-binary skew, env) · `applet_flow` (podman `SendRequest` inspect) |
| Full #2 (Phase 5) | 50/52, 1089 s | `security::test_lock_unlock` (2nd flake today) · `vault::crud::test_note_crud_lifecycle` (podman `SendRequest`) |
| security ×2, agent ×1, totp ×1 (subsets) | one `test_lock_unlock` flake, rest green | |

≈2 affected tests per 52-test run (~4 %), 100 % rerun-clean. Root causes:

1. **Fixed sleeps in the harness — FIXED.** `WaitFor::seconds(5)` for Vaultwarden and a
   flat 1 s wait for the agent socket. Now: poll Vaultwarden's `/alive` (30 s deadline)
   and poll for the socket file (10 s deadline, 50 ms steps). Under load, 5 s/1 s were
   not always enough — the likely mechanism behind the `test_lock_unlock` flakes.
2. **`podman` `SendRequest` on container inspect** — inside testcontainers↔podman, not
   our code. Mitigated by CI retry policy (below), tracked as external.
3. **Stale-binary skew — FIXED at the tooling level**: `just test-agent/cli/ui` now
   depend on `build-test-binaries` so the harness never launches binaries from an older
   commit (Phase 0 F9).

## Invariant-coverage gap analysis (Phases 1–2 → tests)

| Invariant | Test | Status |
|---|---|---|
| Locked agent refuses reads | `security::test_lock_unlock` | existed |
| Reprompt gates secrets | `security::test_reprompt` | existed |
| Tokens never in cache JSON | `security::test_token_leakage` | existed |
| Bulk reads redacted (H2) | agent E2E (`GetEntries` assertions) | existed |
| Debug never prints secrets (H3) | `protocol::debug_redaction_tests` | existed |
| MAC-less type 2 rejected (M4) | `cipherstring::type2_requires_mac` | existed |
| KDF param clamps (M7) | 4 `identity` tests | existed |
| 401 triggers refresh (F2-1) | `server::auth::tests` ×3 | Phase 2 |
| TOTP matches RFC 6238 (F2-2) | `totp::tests` ×5 | Phase 2 |
| **Socket modes 0600/0700** | `ipc_hardening::test_socket_file_modes` | **NEW** |
| **8 MiB request cap (L2)** | `ipc_hardening::test_oversized_request_is_rejected` | **NEW** |
| **Garbage framing → error, agent survives** | `ipc_hardening::test_garbage_request_gets_error_response` | **NEW** |
| **Cipherstring parser never panics** | `cipherstring::arbitrary_input_never_panics` (seeded mini-fuzz, 10 k inputs) | **NEW** |
| **Action decoder never panics** | `protocol::action_decode_from_arbitrary_bytes_never_panics` (10 k + truncations) | **NEW** |
| `SO_PEERCRED` rejects other UIDs | — | **untestable in-suite** (needs a second UID; would require a privileged/user-ns test env). Verified by code review (Phase 1); note kept here so it isn't mistaken for covered. |

The new `ipc_hardening` module is **container-free** (spawns the agent binary against a
temp profile) — 3 tests in 0.16 s, immune to the podman flake class, and included in
`just test-agent`.

The mini-fuzz tests are deliberate stand-ins for `cargo fuzz` (needs nightly + tooling):
deterministic seeds, 10 k inputs each, covering the two attacker-reachable parsers
identified in the threat model (A6 server → cipherstring; A2 client → postcard `Action`).
Real coverage-guided fuzzing stays on the roadmap.

## CI (`.github/workflows/ci.yml`)

- **Every push/PR**: rustfmt check · clippy (workspace + tpm) · check (all features) ·
  unit tests ×4 crates · extension vitest · extension Playwright `firefox-mock` (headless;
  no agent or native host needed) · extension zip built and shape-asserted · `cargo audit`
  (non-blocking on PRs — advisory DB drift shouldn't fail unrelated changes, but stays
  red-visible; ignore list with per-advisory justification in `.cargo/audit.toml`).
- **Nightly + manual**: full Rust E2E on the runner's Docker daemon, with **one retry**
  (policy justified by the measured 100 %-rerun-clean flake profile above).
- Submodules are never fetched (build doesn't need them — Phase 0).
- **Not yet exercised at review time**: the repo had no remote. *(Update 2026-08-10: the
  remote is `github.com/donatengit/cosmic-bwarden`; the first push is the activation step
  — expect one round of runner-environment fixes: apt package names, tss headers.)*

## rustfmt adoption

The tree predated any fmt gate: 105 of ~130 files had diffs. Applied a **one-time
`cargo fmt`** as a dedicated, purely mechanical commit (validated by clippy + full unit
suite afterwards), so the CI fmt gate starts green and future diffs stay style-noise-free.

## Actions

- [x] Harness readiness-polling (Vaultwarden `/alive`, agent socket) — de-flakes root cause 1.
- [x] `just test-*` depend on `build-test-binaries` — kills stale-binary skew.
- [x] 3 container-free IPC-hardening tests; 2 seeded parser mini-fuzz tests.
- [x] CI workflow authored (fmt/clippy/check/unit/audit/extension + nightly E2E w/ retry).
- [x] One-time `cargo fmt` + gate.
- [ ] Push to a forge; watch first CI run (expect small runner fixups).
- [ ] `cargo fuzz` proper (nightly) for the two parsers → roadmap.
- [ ] Peer-cred rejection test in a privileged test env → roadmap (nice-to-have).

## Gate assessment

Invariant coverage gaps closed, flake root causes fixed at the harness level, CI authored
with a measured retry policy. **Phase 5 gate: PASS** (CI activation pending first push).
