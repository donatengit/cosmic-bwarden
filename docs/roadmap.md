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

## Test coverage

- [ ] Grow the in-crate unit layer for pure logic (crypto, parsing, path
      handling). Started: `cipherstring`, `protocol`, `identity` (KDF params),
      `dirs` (path-traversal encoding). Still thin: `vault::decrypt` edge cases,
      `api` model (de)serialization round-trips.
