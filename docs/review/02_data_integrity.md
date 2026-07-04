# Phase 2 — Data Integrity & Correctness

Reviewed: 2026-07-04. Scope: mutation failure paths, sync semantics, offline behaviour,
token/session lifecycle, crash consistency, TOTP correctness, protocol versioning.

Severity: S0 data-loss · S1 correctness · S2 robustness · S3 polish.

## Verdict

Two **S1 correctness bugs found and fixed** (dead token-refresh path; wrong TOTP key
derivation). The integrity architecture itself is solid: mutations are server-first (no
optimistic divergence for add/update/delete), every failure path logs `error!` *and*
surfaces to the UI via `sync_failed` → `Response::Config` → sidebar banner, and the vault
cache write is crash-atomic. **Gate: PASS** with the two fixes landed.

## Fixed this phase

### F2-1 (S1) — token refresh never triggered; expired sessions were permanently dead
`server/auth.rs` matched `Error::Other(msg)` containing `"401"` to decide when to refresh —
but no API call produces that shape. The client returns `Error::RequestUnauthorized`
(sync, add) or `Error::RequestFailed { status: 401 }` (update/delete/favorite). The refresh
arm was unreachable: once the access token expired (~1 h), every server operation failed
until the user logged out and back in, despite holding a valid refresh token.
**Fix:** extracted `is_unauthorized()` matching both real shapes (plus the old string check
as a safety net for wrapped errors); unit-tested against all four shapes
(`server/auth.rs::tests`). *Follow-up (F2-4) below on the refresh race.*

### F2-2 (S1) — TOTP codes did not match real authenticator apps
`handler/vault/ops.rs` fed the **base32 text itself** as raw HMAC key bytes instead of
decoding it, and for `otpauth://` URLs it discarded the URL's
`algorithm`/`digits`/`period`, hardcoding SHA1/6/30. Result: codes that disagree with
Google Authenticator/Aegis for the same secret (and 8-digit/SHA256 accounts could never
work). The E2E test only asserted "6 digits, all numeric" — self-consistent, so it passed.
**Fix:** new `handler/vault/totp.rs` — bare seeds are normalized (case/whitespace) and
base32-decoded; otpauth URLs go through `from_url_unchecked` honouring their parameters;
`*_unchecked` constructors accept the short (80-bit) seeds real providers issue. Verified
against the RFC 6238 appendix-B vector (`generate(59) == "287082"`), URL-parameter
honouring (8-digit vector), and bare/URL path agreement. 5 unit tests.

## Verified sound

- **Mutation failure paths** (`handler/vault/ops.rs`): add/update/delete/favorite all log
  `error!` on server rejection and set `sync_failed`+`last_sync_error`; the UI renders this
  (sidebar banner via `Response::Config`). Matches the AGENTS.md no-silent-failure rule.
- **Update cannot wipe secrets**: redacted (`None`) fields in an incoming update are
  restored from the stored entry before encrypt+PUT (`merge_redacted_secrets`, 3 unit
  tests) — a bulk-read-built edit can't clear passwords server-side.
- **Server-first writes**: no optimistic local commit for add/update/delete; a failed call
  changes nothing locally (favorite is the one optimistic path, and its revert-by-sync is
  logged loudly). Offline edits therefore fail visibly rather than being silently lost.
- **Offline story**: reads work while unlocked; unlock works fully offline (silent re-auth
  failure is tolerated with a `warn!`, vault stays usable; sync unavailable until online).
  `needs_login()` is deliberately token-independent, so token loss never forces a re-login.
- **Crash consistency**: `Db::save` = 0600 temp file → `sync_all` → atomic rename (verified
  Phase 1); a crash mid-save cannot truncate the cache. Load failure falls back with
  `error!` per the logging invariant.
- **Lock hygiene** (`state.rs::lock`): keys/org-keys/hash dropped (zeroize-on-drop), session
  tokens zeroized, encrypted entries retained for offline unlock. Subscriber list is pruned
  on send failure (`broadcast` retain), so dead UI connections don't accumulate.
- **Keyring lifecycle**: tokens restored on unlock (keyring → in-memory fallback → silent
  re-auth); refresh persists new tokens to keyring and disk-DB save failures log `error!`.

## Open findings

**F2-3 (S2) — concurrent-edit conflicts are last-write-wins, undetected.**
`update_entry_on_server` PUTs the full cipher without a `revisionDate` precondition; an
edit made from another client between our last sync and our PUT is silently overwritten.
Bitwarden's official clients behave similarly (server does not do optimistic locking on
PUT), so this is acceptable — but it must be a *documented* decision. → Note added here;
revisit only if multi-device editing becomes a complaint.

**F2-4 (S2) — token refresh has no mutual exclusion.**
Now that refresh actually fires (F2-1), two concurrent 401s can both call
`exchange_refresh_token`. Vaultwarden rotates refresh tokens, so the loser may persist a
stale token. Low probability (needs two in-flight calls straddling expiry), self-heals via
re-login, but worth a `tokio::sync::Mutex<()>` around the refresh block. → roadmap.

**F2-5 (S3) — decrypted sidebar cache is cleared but not zeroized on lock.**
`sidebar_cache` holds decrypted names/usernames in plain `String`s; `lock()` clears the Vec
without zeroizing. Names are low-sensitivity; note only.

**F2-6 (S3) — protocol version equality makes every rebuild "incompatible".**
`check_protocol_compatibility` requires exact equality of build versions, and the version
embeds seconds-since-month-start — so any two separately-built binaries mismatch (observed
live in Phase 0's E2E baseline). Today the check is informational (CLI `version` command
only), so this is cosmetic; before external users arrive, `protocol_version` should become
an independent small integer bumped on breaking changes only. → Phase 7 decision, already
flagged in the review plan.

Correction to Phase 0 F7: the agent crate had 3 unit tests (merge), not 0 — the earlier
read was a tail-truncated test output. With this phase it has 11.

## Validation

- `cargo check -p cosmic-bwarden-agent` (default + earlier tpm run) — clean, warnings-as-errors.
- `cargo test -p cosmic-bwarden-agent` — 11/11 (3 pre-existing merge + 8 new).
- E2E: `security` suite 4/4 after podman socket restart (one initial container flake,
  rerun-clean — consistent with Phase 0 F9's ~2 % rate);
  `test_get_totp_from_login_entry` re-run against the rebuilt agent with the fix.

## Gate assessment

Two S1 bugs fixed with regression tests; no S0; remaining items scheduled (F2-4 → roadmap,
F2-6 → Phase 7). **Phase 2 gate: PASS** — proceed to Phase 3 (architecture & code health).
