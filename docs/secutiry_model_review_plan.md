# Security Model Review & Remediation Plan

Review date: 2026-07-02. Scope: agent (IPC, crypto, TPM, keyring, SSH agent, browser
host), core crypto/persistence, CLI, and the browser extension.

## Executive summary

The architecture is fundamentally sound: a per-user agent holds key material in
mlock'd, zeroize-on-drop buffers; clients are thin; TLS is rustls with native
roots; session tokens are kept out of the on-disk JSON cache and go to the Secret
Service. The IPC socket is verified with `SO_PEERCRED` (same-UID only) and is
`0600` in a `0700` runtime dir.

This plan fixes three serious issues and a set of medium/low hardening items.

## SO_PEERCRED decision (recorded)

The main IPC socket accepts a connection only when the peer UID equals our own
(`main.rs`), rejecting everyone else including root. **We keep this — we do not add
root to the allowlist.** Rationale:

- This is a *per-user* secret agent, not a multi-user system daemon. There is no
  legitimate root-owned client.
- Root gains nothing from being whitelisted: it can already reach the vault via
  `ptrace`, `/proc/<pid>/mem`, or `setuid(our_uid)` then `connect()` (which then
  passes the same-UID check). Whitelisting uid 0 only widens the surface (e.g. a
  partially-compromised root daemon that can reach a socket but cannot yet ptrace)
  with zero benefit.

The gap was that the **SSH-agent socket** did no peer-cred check at all — that is
fixed below to the same same-UID bar.

## Findings & fixes

### High

**H1 — TPM sealed blobs were not PCR-bound (docs claimed PCR{0,7}).**
`tpm.rs` sealed under a plain user-auth object with a deterministic Owner primary
and no `authPolicy`. An attacker who boots another OS can recreate the primary,
read the blob, and brute-force the PIN under TPM DA lockout only; the
server-credentials blob (empty PIN) unseals immediately.
*Fix:* seal under `authPolicy = PolicyPCR(sha256, {0,7}) ∧ PolicyAuthValue`, with
`userWithAuth=false` so the PIN is only usable through the policy, and unseal via a
policy session. Bump the blob format (`version` byte); an old blob or a changed
firmware/Secure-Boot state fails to unseal and the user falls back to master
password (and re-runs PIN setup). See also H1/M8 session encryption.

**H2 — Bulk/secondary reads bypassed master-password reprompt.**
`GetEntries` returned every entry fully decrypted (passwords, notes, SSH private
keys) with no reprompt check; `GetTotp` had no reprompt at all. Any client,
including the browser native host, could dump the vault in one call.
*Fix:* `GetEntries` returns entries with secrets redacted (same set as
`GetEntryMeta`, extended to hidden custom fields). Per-secret retrieval stays on
`GetEntry`/`GetPassword`/`GetTotp`; `GetTotp` now honours reprompt via the shared
verification path. Add a `password` field to `GetTotp`.

**H3 — Credentials leaked into logs.**
`Action`/`Response` derived plain `Debug`; `handler.rs` logged the full action at
`info!`, and `main.rs`/`browser_host.rs` logged request/response at `debug!`. A
`RUST_LOG=debug` (or default info) agent persists master passwords, PINs, and
fetched secrets to the journal.
*Fix:* hand-written `Debug` for `Action` and `Response` that prints the variant
name (plus non-secret scalars like versions/flags/error message) and never secret
payloads.

### Medium

- **M4 — Unauthenticated CBC accepted.** `cipherstring.rs` accepted 2-part type-2
  strings and only verified a MAC when present. Bitwarden type 2 always carries a
  MAC. *Fix:* require exactly 3 parts for type 2 (reject MAC-less).
- **M5 — SSH-agent socket skipped peer-cred check** and its override parent dir
  used default (0755) perms. *Fix:* wrap the accept loop with a `SO_PEERCRED`
  same-UID check; create the parent dir 0700.
- **M6 — Vault cache written with umask-default perms.** `Db::save` used
  `File::create`. *Fix:* write `0600` via `OpenOptions.mode`, write to a temp file
  and rename for crash-atomicity.
- **M7 — Argon2id panicked / unbounded memory.** `identity.rs` did
  `memory.unwrap()`/`parallelism.unwrap()` and had no cap. *Fix:* validate presence
  and clamp memory/parallelism to sane bounds.
- **M8 — No PIN policy + unencrypted TPM sessions.** Empty PIN accepted; null-auth
  sessions expose the unsealed key on the bus. *Fix:* enforce a minimum PIN length
  in the agent; use an encrypt/decrypt policy session for unseal (H1).
- **M9 — Extension domain matching used naive eTLD+1.** `popup.js` took the last
  two labels, so `co.uk` entries cross-matched. *Fix:* compare the full registrable
  host (strip only leading `www.`) and match by suffix boundary.

### Low / hardening

- **L1** — `cipherstring.rs` `ty[0] - b'0'` underflow on non-digit type: parse
  defensively.
- **L2** — main IPC read allocated up to 4 GiB attacker-controlled: cap message
  size (match browser host's 1 MiB).
- **L3** — remove stray `crates/cosmic-bwarden-core/src/protocol.rs.fix.tmp`.
- **L4** — `dirs::db_file` percent-encodes server but not email: encode email too.
- (Deferred, documented) KDF runs under the state lock on a current-thread runtime;
  reprompt hash compare is not constant-time; IPC plaintext passwords linger as
  `String`. Tracked for a follow-up; not addressed in this pass.

## Validation

- `cargo check` for core, agent (default), agent `--features tpm`, cli, ui.
- New unit tests: cipherstring rejects MAC-less type 2; `Action`/`Response` Debug
  redaction; `GetEntries` redaction (agent E2E).
- `just test-extension-unit` for the domain-match change.
- TPM smoke tests (`--features tpm-smoke`) require `swtpm`; the PCR path is
  exercised there when available.
