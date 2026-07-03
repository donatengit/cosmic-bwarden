# Roadmap / Deferred Work

Backlog of health and hardening tasks that are intentionally deferred. Newest
concerns at the top of each section. Convert to issues once the project is on a
forge.

## Pre-publish (before making the repo public)

- [ ] Scrub git history and working tree for secrets / machine-specific paths.
- [x] Review `docs/*_plan.md` scratch docs — keep or archive. *(Done 2026-07: completed
      plans moved to `docs/archive/`, see `docs/review/00_ground_truth.md` F3.)*
- [ ] Add CI once on a forge (build + test + `clippy`). Note: warnings are
      already denied at build time via `[workspace.lints]`, so CI mainly needs to
      run the suite and clippy.

## Security / supply chain

- [ ] **Dependency auditing** — add `cargo-deny` (advisories + license + bans) and
      wire `cargo audit` into the build/CI. Priority because this is a password
      manager and it pulls `tss-esapi 8.0.0-alpha.2` (an alpha crate handling key
      material). Track advisories against the whole tree, not just direct deps.
- [ ] Pin/track the alpha `tss-esapi` version deliberately; revisit when a stable
      release lands.

## Maintainability

- [ ] **File-size refactoring** — several files exceed the 500-line hard limit in
      `AGENTS.md`. Concrete split plan: [`docs/archive/file_size_refactoring.md`](archive/file_size_refactoring.md).

## Test coverage

- [ ] Grow the in-crate unit layer for pure logic (crypto, parsing, path
      handling). Started: `cipherstring`, `protocol`, `identity` (KDF params),
      `dirs` (path-traversal encoding). Still thin: `vault::decrypt` edge cases,
      `api` model (de)serialization round-trips.
