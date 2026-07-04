# Phase 0 — Ground Truth & Repo Hygiene

Reviewed: 2026-07-03. Toolchain: rustc/cargo 1.96.1, just 1.55.1.
Container runtime: podman via user socket (no Docker daemon; a `docker` group exists for
socket compat, so `sg docker` recipes work *on this machine* — see F5).

## Baseline: build & test matrix

| Check | Result | Notes |
|---|---|---|
| `cargo check --workspace` | ✅ pass | warnings-as-errors enforced workspace-wide |
| `cargo check -p agent --features tpm` | ✅ pass | |
| Fresh clone (no submodules) + `cargo check --workspace` | ✅ pass (35 s) | **build has zero dependence on the 4 submodules** |
| Unit tests: core | ✅ 24/24 | |
| Unit tests: ui | ✅ 115/115 | |
| Unit tests: cli | ✅ 2/2 | |
| Unit tests: agent | ⚠️ 3 tests (merge only) | *(corrected in Phase 2 — a tail-truncated read reported 0; now 11 after Phase 2 fixes)* — see F7 |
| Rust E2E (`cosmic-bwarden-tests`, `--test-threads=1`) | ✅ 50/52, 2 env-failures rerun clean | 1062 s wall time; see F9 |
| Extension unit (vitest) | ✅ 9/9 | |
| Extension E2E (firefox-mock, Playwright) | ✅ 14/14 | |
| Extension E2E (firefox-full `full.spec.js`) | ❌ 0/4 known-broken | Firefox MV3 `service_worker` unsupported via debug protocol; needs MV2 manifest variant (pre-existing, tracked) |
| TPM smoke | ⏭️ skipped | `swtpm` not installed on review machine |

**Rust E2E detail**: first full run 50 passed / 2 failed; both failures reproduce-negative
after a rebuild (2/2 pass on rerun):

- `version::test_cli_version_subcommand_match` — **stale-binary skew, not a bug**: the
  harness runs whatever `target/debug/` binaries exist; rebuilding only the agent after a
  new commit gave it version `2026.07-252762-55c284b` while the CLI still carried
  `2026.07-232857-977d01c`, and the compatibility check (correctly) rejected the pair.
- `applet_flow::test_logout_clears_account_state` — container-runtime flake
  (`failed to inspect a container: hyper SendRequest` against the podman socket).

## Findings

Severity scale: S0 security/data-loss · S1 correctness · S2 maintainability · S3 polish · S4 nice-to-have.

### F1 (S2) — Reference submodules add ~280 MB and zero build value
`bitwarden-official-clients` (176 MB), `libcosmic` (66 MB), `cosmic_examples` (36 MB),
`rbw_reference` (3 MB) are git submodules used only as reading material; the fresh-clone
check proves the build never touches them (libcosmic is a git dependency in Cargo.toml).
They quintuple clone size, will slow every CI checkout, and confuse packagers.
**Recommendation**: remove all four submodules; record their URLs + pinned commits in a new
`docs/references.md`. Keep local clones outside the repo if desired.
**Decision (Phase 0)**: deferred to end-of-review cleanup — `cosmic_examples` is comparison
material for Phase 4, and CI checkouts skip submodules by default, so nothing is blocked.

### F2 (S1) — `docs/build_and_run.md` documents a binary that doesn't exist
It instructs `./target/release/cosmic-bwarden-ui`, but the UI crate's `[[bin]]` is
`cosmic-applet-bwarden` (justfile installs that name). Any new user following the doc fails
at step 2. Also: prerequisites don't mention `just`, podman/docker (for tests), or `npm`
(for the extension). **Recommendation**: fix binary name, add a prerequisites matrix,
add a "verify your setup" one-liner per component.

### F3 (S2) — Docs directory mixes living docs with 6+ completed plan documents
Historical plans (`browser_extension_plan.md`, `browser_extension_e2e_tests_plan.md`,
`file_size_refactoring.md`, `file_size_refactoring_tests.md`, `test_implementation_plan.md`,
`ui_redesign.md`) sit beside living reference docs, and `secutiry_model_review_plan.md` has
a typo'd filename. **Recommendation**: move completed plans to `docs/archive/` (history is
in git anyway), rename the security doc `security_model_review_plan.md`, and make
`docs/summary.md` link the living set.

### F4 (S3) — `GEMINI.md` duplicates agent guidance
17-line pointer file that restates the testing hierarchy already owned by `AGENTS.md` /
`docs/testing.md`; it references `tests/agent.rs`-style paths that don't match the current
`--lib` module layout. **Recommendation**: delete it or reduce it to a single line pointing
at `AGENTS.md` (some tools look for `GEMINI.md` by name, which justifies keeping a stub).

### F5 (S2) — justfile test recipes hardcode `sg docker -c`
`test-agent`/`test-cli`/`test-ui` wrap cargo in `sg docker`. Works here only because a
`docker` group happens to exist; on a stock podman machine (or CI) it fails, and the test
harness (`common.rs`) auto-detects the podman socket anyway, making the wrapper unnecessary
for socket access. **Recommendation**: drop `sg docker` from recipes; document
`systemctl --user start podman.socket` as the prerequisite (it already is in
`docs/testing.md`'s spirit). This also unblocks straightforward CI.

### F6 (S3) — Stray temp artifacts in the working tree
`browser-extension/background.js.tmp` (June 16), `agent_test.log` at repo root and inside
`crates/cosmic-bwarden-tests/`. All gitignored, so low risk — but the `.tmp` file suggests
an interrupted tool run, and test logs landing at the repo root suggests a harness writing
to CWD. **Recommendation**: delete strays; point the test-harness log at `target/` or a
temp dir. Also observed: a Vaultwarden container from a previous session still running
after 7 h — testcontainers cleanup doesn't always fire; worth a `just clean-containers`
recipe.

### F7 (S2) — Agent crate has zero unit tests
The security-critical daemon (5.1 k lines: IPC auth, TPM, SSH agent, handlers) is tested
only end-to-end. E2E coverage is genuinely good, but pure logic (request routing, policy
checks, cipher-string edge cases in handlers) should regression-test in milliseconds, not
via containers. Feeds Phase 5 (test gap analysis); no action in Phase 0.

### F9 (S2) — E2E harness is sensitive to stale debug binaries and runtime flakes
Two expressions of the same weakness, both observed in the baseline run:
(a) the suite launches prebuilt `target/debug/` binaries, so building crates at different
commits fails the version test with a confusing message — the harness should either rebuild
`agent`+`cli` itself (a `just` dependency) or print "rebuild both binaries" in the panic;
(b) one container-inspect flake in 52 tests (~2 %) — rerun-clean, but at this rate CI would
red-flag roughly every other full run. Feeds Phase 5 (flakiness measurement + retry policy).
Note the version failure is *also* live evidence for the versioning concern in the review
plan (Phase 2/7): seconds-since-month-start makes every rebuild a new "incompatible"
protocol version.

### F8 (S4) — No CI (restating the plan's headline gap)
All of the above matrix runs are manual. F1 + F5 are the two blockers that make CI
expensive today; once both land, a fmt→clippy→check→unit pipeline is an afternoon of work
(Phase 5).

## Phase 0 actions applied

- [x] Baseline matrix recorded (this document)
- [ ] F1 submodule removal — deferred to end-of-review cleanup (see decision above)
- [x] F2 build doc fix — binary name corrected, prerequisites expanded, submodule note added
- [x] F3 docs archive shuffle — 6 completed plans → `docs/archive/`, security doc renamed,
      `roadmap.md` links updated
- [x] F4 GEMINI.md reduced to a drift-proof stub
- [x] F5 justfile `sg docker` wrappers removed (harness auto-detects podman socket)
- [x] F6 stray files deleted (`background.js.tmp`, 2× `agent_test.log`)

## Gate assessment

Build is reproducible from a fresh clone; test suites are green apart from the pre-existing,
understood firefox-full MV3 failures. **Phase 0 gate: PASS** once the checklist above is
applied. Proceed to Phase 1 (security review).
