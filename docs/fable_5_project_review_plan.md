# Project Review Plan — Road to a SOTA COSMIC Password Manager

**Reviewer stance**: senior architect reviewing `cosmic-bwarden` end-to-end, with the goal of
making it the reference-quality Bitwarden client for the COSMIC desktop.

**Scope snapshot** (2026-07): 5 workspace crates (~23k lines of Rust), a vanilla-JS browser
extension (~630 lines + tests), a `just`-driven build, Docker-based E2E tests, optional TPM
feature. 44 commits, no CI yet. Reference repos (`bitwarden-official-clients`, `libcosmic`,
`cosmic_examples`, `rbw_reference`) are git submodules.

---

## How to run this review

- **One phase per working session.** Each phase produces a findings file
  `docs/review/NN_<phase>.md` with issues ranked by severity, so later phases can build on
  earlier conclusions instead of re-deriving them.
- **Severity scale**: `S0` security/data-loss (fix before anything else), `S1` correctness,
  `S2` architecture/maintainability, `S3` polish/UX, `S4` nice-to-have.
- **Rule**: a phase is not "done" until its S0/S1 findings are either fixed or explicitly
  accepted with a written rationale.

**Why this order**: the product is a password manager, so trust is the product. We review in
order of *blast radius*: first what can leak secrets or lose data, then what makes the code
wrong, then what makes it hard to change, then what makes it feel native and polished.
Cheap-to-fix structural issues come before UX because every later fix lands on top of them.

---

## Phase 0 — Ground truth & repo hygiene (½ day)

Establish a reproducible baseline before judging anything.

- [ ] Fresh-clone build: `git clone --recurse-submodules` into a clean dir, follow
      `docs/build_and_run.md` verbatim. Every deviation is a doc bug.
- [ ] Run the full validation matrix from `AGENTS.md` and record pass/fail/flaky per suite
      (unit, agent, CLI, UI, tests crate, extension unit, extension E2E, TPM smoke).
- [ ] Repo hygiene: are all four reference submodules still needed, or should they move to a
      `docs/references.md` list of URLs + pinned commits? (~4 submodules bloat clone time and
      confuse packaging.) Check `.gitignore` covers `test-results/`, `browser-extension/test-results/`.
- [ ] Docs drift: `docs/` has 20 files, several of them historical plans
      (`*_plan.md`, `file_size_refactoring*.md`). Decide keep/archive/delete so the doc set
      reflects the present, not the journey.
- [ ] Stale top-level files: `GEMINI.md` vs `AGENTS.md` — one source of truth for agent rules.

**Output**: baseline report + hygiene fix list. Nothing here should take long to fix.

## Phase 1 — Security review (2–3 days; the core of the review)

This is where SOTA is won or lost. Review against the invariants in `AGENTS.md` §Security,
but *verify in code*, don't trust the docs.

1. **Threat model first** (write it down if absent — `docs/threat_model.md`):
   attacker classes: other local users, same-user malicious process, memory scraping after
   crash/suspend, stolen disk, malicious page vs browser extension, compromised server.
   Every later finding maps to an attacker class.
2. **Key material lifecycle**: trace master key / `enc_key_expanded` / `mac_key_expanded`
   from derivation → memory-locked storage (`core/src/locked.rs`) → zeroize-on-drop.
   Check: no `Debug`/`Display` impls on secret types, no clones into unlocked heap,
   no secrets in `format!`/logs/panic messages, `PR_SET_DUMPABLE` set before any secret exists.
3. **Crypto correctness** (`core/crypto/`): AES-256-CBC needs strict MAC-then-verify order
   (Encrypt-then-MAC, constant-time compare); PBKDF2/Argon2id parameters ≥ Bitwarden
   defaults; RNG source for IVs; cipherstring parsing against malformed input (fuzz candidate).
   Diff behavior against `rbw_reference` and official clients for the known-tricky paths
   (key rotation, org keys, attachment keys if supported).
4. **IPC surface** (agent): `SO_PEERCRED` on *every* accept path (main socket, SSH agent
   socket, browser host); socket mode `0600` and dir perms; request size limits / DoS on the
   length-prefixed protocol; what a hostile same-UID client can extract while *locked*.
5. **TPM PIN path** (`agent/src/tpm/`): PCR{0,7} policy correctness, PIN retry/lockout
   (dictionary-attack protection: TPM `DA` params?), blob file perms, behavior on
   firmware-update PCR change (graceful re-enroll vs data loss).
6. **Browser extension**: native-messaging origin allowlist, the `GetEntryMeta`-vs-secrets
   invariant (no plaintext in JS state from passive browsing), content-script injection
   surface (`content.js`) against hostile pages — fill only on user gesture, frame/origin
   checks, no `innerHTML` with vault data.
7. **Peripheral leaks**: clipboard (is there an auto-clear timer? SOTA requirement),
   secrets over the wire in IPC responses that don't need them, keyring (`oo7`) contents,
   `Db` JSON on disk (`#[serde(skip)]` claims — verify with a hexdump of a real cache file),
   lock-on-suspend/lid-close via `logind.rs`.
8. **Supply chain**: `cargo audit`, `cargo deny` (licenses + duplicate crypto crates),
   count and justify every `unsafe` block, review `build.rs` files.

**Output**: `docs/review/01_security.md` + threat model. S0 findings block all other work.
Note: `docs/secutiry_model_review_plan.md` exists (typo and all) — fold it in or supersede it.

## Phase 2 — Data integrity & correctness (1–2 days)

The second way to destroy trust: losing the user's edits.

- [ ] **Optimistic-mutation audit**: for each Add/Update/Delete/Favorite, trace the failure
      path — server rejects, network drops mid-flight, agent crashes before sync. The
      AGENTS.md rule says failures must log `error!`; check they also *surface to the UI*
      (a log line is not UX) and that local state reconciles sanely on next sync.
- [ ] **Offline story**: what works locked/unlocked without network? Is read-only offline
      access guaranteed? Edits made offline — queued, rejected, or silently lost?
- [ ] **Sync conflicts**: concurrent edit from another client between local edit and sync.
      `revisionDate` handling; last-write-wins is acceptable if *deliberate and documented*.
- [ ] **Crash consistency**: `db.save` atomicity (write-temp-then-rename?), partial-write
      recovery, behavior when disk is full.
- [ ] **Token/session lifecycle**: refresh-token expiry mid-session, keyring unavailable at
      startup, clock skew.
- [ ] **Protocol versioning**: the `YYYY.MM-N-<git>` scheme — verify mixed-version
      agent/CLI/UI degrade gracefully, and decide now whether protocol_version should become
      an independent integer *before* there are external users.

**Output**: `docs/review/02_data_integrity.md` + a test case per confirmed gap
(per the "every bug fix requires a test" rule).

## Phase 3 — Architecture & code health (1–2 days)

Now that we know what the system must guarantee, check whether the structure can carry it.

- [ ] **Crate boundaries**: does `core` stay UI-free and agent-free? Any dependency cycles
      via shared types? Is the IPC protocol a single-source-of-truth module or duplicated
      structs?
- [ ] **Error-handling strategy**: `anyhow` at edges, typed errors in `core`? Consistent?
      Any `unwrap`/`expect` on runtime paths in the daemon (each is a potential
      secrets-holding-process crash)? `rg 'unwrap\(\)|expect\(' crates/*/src --stats`.
- [ ] **Async discipline** in the agent: blocking calls (TPM, keyring, file I/O) on the tokio
      runtime? Task cancellation on client disconnect? Unbounded channels?
- [ ] **MVU discipline** in the UI: logic-free views, `update/` handler chain cohesion,
      state that should be derived rather than stored.
- [ ] **File-size & module rules** from AGENTS.md: `find crates -name '*.rs' | xargs wc -l | sort -rn | head`
      — anything over 500 lines is a mandated split.
- [ ] **Dependency weight**: `cargo tree -d` for duplicates; justify each heavy dep;
      `cargo bloat` on the release build.
- [ ] **Clippy at pedantic**: `cargo clippy --workspace --all-features -- -W clippy::pedantic`,
      triage rather than blanket-fix.

**Output**: `docs/review/03_architecture.md` — refactor list sized S2, sequenced so it
doesn't conflict with Phase 1/2 fixes.

## Phase 4 — COSMIC-native integration & UX (1–2 days)

"SOTA for COSMIC" means it feels like it shipped with the desktop.

- [ ] **Applet conventions**: compare against `cosmic_examples/cosmic-applets` — panel icon
      states (locked/unlocked), popup sizing/autosize behavior, context-menu structure,
      correct `.desktop` metadata and icon naming per `docs/cosmic_integration.md`.
- [ ] **Theming**: full light/dark/high-contrast pass; no hardcoded colors; icon set matches
      COSMIC symbolic style (check the `icons/` black/white PNG approach vs proper symbolic
      SVGs — PNGs are a smell for panel icons).
- [ ] **i18n**: `fl!` coverage audit (the AGENTS.md rule) — grep for bare string literals in
      widgets; ship at least one non-English locale to prove the pipeline.
- [ ] **Keyboard-first UX**: global "quick access" shortcut story, Tab/arrow navigation in
      applet popup and vault window, Escape behavior, Enter-to-submit everywhere.
- [ ] **Accessibility**: screen-reader labels on icon-only buttons (there are several in the
      applet), focus indicators, minimum hit-target sizes.
- [ ] **Session integration**: lock on suspend/lid/idle (logind), applet autostart, agent
      systemd unit hardening (`ProtectSystem=`, `NoNewPrivileges=`, `MemoryDenyWriteExecute=`
      — audit the unit file like a service, because it is one).
- [ ] **SOTA feature-parity gaps** (rank, don't build yet): TOTP display + auto-copy,
      passkey/FIDO2 story, password generator in applet, autotype/autofill on Wayland,
      folder/collection navigation, attachment support.

**Output**: `docs/review/04_cosmic_ux.md` + prioritized UX backlog merged into
`docs/roadmap.md`.

## Phase 5 — Testing & CI (1 day)

- [ ] **Gap analysis against Phases 1–2 findings**: every security invariant should have a
      test that fails when it regresses (e.g., a test asserting socket perms, a test that a
      locked agent refuses `GetPassword`). Invariants only enforced by review will regress.
- [ ] **Flakiness**: run the E2E suite 5× (`--test-threads=1`), record intermittents.
- [ ] **CI bootstrap** (there is none — this is the single highest-leverage gap in the
      project): GitHub Actions with stages `fmt → clippy → check (all features) → unit →
      cargo audit`, E2E behind a label or nightly (Docker + Vaultwarden), extension unit
      tests. Cache `~/.cargo` and `target/`. Submodules make CI clones expensive — feeds the
      Phase 0 submodule decision.
- [ ] **Fuzzing seed**: `cargo fuzz` targets for cipherstring parsing and IPC frame decoding
      (the two attacker-reachable parsers).

**Output**: working CI pipeline + `docs/review/05_testing.md`.

## Phase 6 — Performance & footprint (½–1 day)

- [ ] Cold-start to usable popup time; unlock latency (Argon2id cost vs UX budget ~1s).
- [ ] Large-vault behavior: generate a 5k-entry vault in Vaultwarden, measure search latency
      and decryption-cache memory; check the cache is bounded and zeroized on lock.
- [ ] Agent idle footprint (RSS, wakeups) — it runs 24/7 on the user's session.
- [ ] Binary size sanity (`cargo bloat`); the `opt-level="z"` + fat-LTO profile — verify it
      doesn't hurt unlock latency (crypto code at `-Oz` can be measurably slower; consider
      `opt-level = 3` overrides for the crypto crates).

**Output**: `docs/review/06_performance.md` with numbers, not adjectives.

## Phase 7 — Packaging, distribution & release (1 day)

- [ ] **Versioning**: `YYYY.MM-N-<git>` with N = seconds-into-month is not comparable by
      package managers within a month across rebuilds and looks unstable to users. Propose
      SemVer-ish `YYYY.MM.PATCH` for releases, keep the git id in `--version` output only.
- [ ] **Targets**: Flatpak manifest (primary for COSMIC/Pop!_OS users), `.deb` recipe, AUR
      PKGBUILD. The agent's systemd user unit + native-messaging host manifest need install
      paths that work in all three (native messaging inside Flatpak is a known hard problem —
      investigate early, it may constrain the architecture).
- [ ] **Extension shipping**: AMO (Firefox) and Chrome Web Store submission requirements;
      review `pack-extension` output against store validation.
- [ ] Release automation: tag → CI builds → signed artifacts + changelog.

**Output**: `docs/review/07_packaging.md` + at minimum a working Flatpak or deb.

## Phase 8 — Documentation & first-run experience (½ day)

- [ ] README for humans (current docs are excellent for agents, thin for users): screenshots,
      one-paragraph pitch, install matrix, security model summary with honest limitations.
- [ ] First-run flow review: from "installed" to "first password copied" with zero terminal
      use — count the steps, kill the unnecessary ones.
- [ ] Consolidate `docs/` per the Phase 0 keep/archive decision; fix the
      `secutiry_model_review_plan.md` filename typo while at it.

**Output**: shippable README + pruned doc tree.

---

## Sequencing summary

| # | Phase | Effort | Gate |
|---|-------|--------|------|
| 0 | Ground truth & hygiene | 0.5d | reproducible build |
| 1 | Security | 2–3d | no open S0 |
| 2 | Data integrity | 1–2d | no open S0/S1 |
| 3 | Architecture | 1–2d | refactor list agreed |
| 4 | COSMIC UX | 1–2d | UX backlog ranked |
| 5 | Testing & CI | 1d | CI green |
| 6 | Performance | 0.5–1d | numbers recorded |
| 7 | Packaging | 1d | one installable artifact |
| 8 | Docs & first-run | 0.5d | README shippable |

Total: roughly two focused weeks. Phases 5–8 can interleave once 1–2 are clean; 0–2 are
strictly ordered.
