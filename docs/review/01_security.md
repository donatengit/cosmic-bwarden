# Phase 1 — Security Review

Reviewed: 2026-07-04. Method: read the crypto core, agent IPC/handlers, TPM module,
keyring, SSH agent, browser host, and the extension's page-facing scripts; verified the
prior security pass (`docs/security_model_review_plan.md`, 2026-07-02) against the actual
code rather than trusting its claims; then swept the areas that pass did not cover
(supply chain, clipboard, content-script fill, memory-locking scope, env-var behaviour,
TLS scheme).

Severity: S0 secret-leak/data-loss · S1 correctness · S2 hardening/medium · S3 low.

## Verdict

**No S0 or S1 findings.** The architecture is sound and the 2026-07-02 remediation is real —
all fourteen claimed fixes are present and, where testable, unit-tested. Remaining items are
medium/low hardening. This gate **PASSES** to Phase 2; the S2 items below should be
scheduled, not treated as blockers.

## Threat model (recorded)

Assets: master password, derived vault keys (enc‖mac), decrypted entry secrets, session
tokens, SSH private keys. Attacker classes:

| # | Attacker | Reachable surface | Primary defence |
|---|----------|-------------------|-----------------|
| A1 | Other local UID | IPC + SSH sockets, cache files | `SO_PEERCRED` same-UID, `0600` sockets, `0700` dirs |
| A2 | Same-UID hostile process | full IPC protocol | *by design can read the vault*; reprompt only raises cost |
| A3 | Memory scraping (core dump / swap) | agent RSS | `PR_SET_DUMPABLE=0`, `mlock`+zeroize for keys/plaintext |
| A4 | Stolen disk (cold) | cache JSON, TPM blob, keyring | vault key never on disk; TPM PCR-bound; tokens in Secret Service |
| A5 | Malicious web page | content script, popup | fill only on user gesture; runtime-only messaging |
| A6 | Compromised/MITM server | API responses, cipherstrings | rustls native roots; MAC-required type-2; KDF-param clamp |

A2 is explicitly out of scope for full mitigation: a process running as the same user can
`ptrace` the agent, so IPC reprompt is a speed-bump, not a wall. This is the correct call and
matches the recorded `SO_PEERCRED` decision.

## Prior fixes — verified present

| ID | Claim | Verified at | Test |
|----|-------|-------------|------|
| H1 | TPM sealed blobs PCR{0,7}-bound, `userWithAuth=false`, DA lockout on, encrypted unseal session | `tpm/policy.rs`, `tpm/ops.rs:119` | tpm-smoke |
| H2 | Bulk/meta reads redact all secrets; per-secret reads gate reprompt | `handler/vault/query.rs:11,187,257` | agent E2E |
| H3 | Manual `Debug` on `Action`/`Response`/`Secret` — no secret payloads | `protocol.rs:252,397`, `db/models.rs:184` | `protocol.rs` unit tests |
| M4 | MAC-less type-2 cipherstring rejected | `cipherstring.rs:50` | `type2_requires_mac` |
| M5 | SSH socket `SO_PEERCRED` + `0700` parent + `0600` socket | `ssh_agent.rs:22,62,69` | — |
| M6 | `Db::save` writes `0600` temp then atomic rename | `db/persistence.rs:62` | — |
| M7 | Argon2id params validated + clamped (16..=1024 MiB, 1..=16 p), no `unwrap` | `identity.rs:50` | 4 unit tests |
| M8 | Min-PIN policy + encrypt/decrypt TPM session | `tpm/ops.rs:131`, core `MIN_PIN_LEN` | tpm-smoke |
| L1 | Non-digit cipherstring type no underflow | `cipherstring.rs:35` | `non_digit_type_is_rejected` |
| L2 | IPC request length capped (8 MiB) | `lib.rs:210` | — |
| L4 | Email percent-encoded in cache path | `dirs.rs:54` | 3 traversal tests |

Also confirmed good: encrypt-then-MAC with the MAC over `iv‖ciphertext` and constant-time
`hmac::verify` (`cipherstring.rs:105,215`); IVs from `rand::rng().fill_bytes` (CSPRNG);
`locked::Vec` is `mlock`'d + zeroize-on-drop and secret types expose no `Debug`;
`PR_SET_DUMPABLE(0)` runs before any secret exists (`lib.rs:102`); `build.rs` takes no
untrusted input and makes no network calls.

## Environment-variable behaviour (requested check)

**Headline: no environment variable or CLI flag disables a security control.** A tree-wide
grep for `danger_accept`/`accept_invalid`/`INSECURE`/`verify_none`/`no_verify`/`ALLOW_*`
returns nothing — there is no TLS-verification bypass, no peer-cred bypass, and no
"insecure mode" switch. Every recognised env var only selects a path, a profile namespace,
or log verbosity, all within the invoking user's own privilege:

| Var | Effect | Assessment |
|-----|--------|------------|
| `COSMIC_BWARDEN_CONFIG` / `_SOCKET` / `_SSH_SOCKET` | override config/socket paths | same-UID; sockets still `0600`+peer-cred regardless of path. **But see P1-2.** |
| `COSMIC_BWARDEN_PROFILE` | namespaces data/config/cache dirs | same-UID; created `0700` via `make_all` |
| `COSMIC_PANEL_NAME` | UI applet-vs-window mode | cosmetic |
| `RUST_LOG` | log level | Debug redaction holds even at `debug`. **But see P1-6/P1-7.** |

The one env-reachable hardening gap is **P1-2** (overridden main-socket parent dir mode).

## Findings

### S2 — medium hardening

**P1-1 — `base_url` scheme is not validated; `http://` sends credentials in cleartext.**
`config.rs:109` accepts any `base_url` and only appends `/api`. A config with an
`http://` server (reachable by pointing `COSMIC_BWARDEN_CONFIG` at a crafted file, or a
mistyped self-host URL) makes the agent POST the master-password hash and bearer tokens over
plaintext HTTP with no warning. *Recommend:* require `https://` unless the host is
loopback (`localhost`/`127.0.0.0/8`/`::1`), and log `warn!` when falling back to http for a
non-loopback host.

**P1-2 — overridden main IPC socket parent dir is not forced to `0700`.**
`lib.rs:117` does `create_dir_all(parent)` with no mode, so when `COSMIC_BWARDEN_SOCKET`
points at a fresh directory it is created `0755` (umask default). The SSH-agent path already
fixes this (`ssh_agent.rs:62`, explicit `0700`); the main socket should match. Mitigated by
the `0600` socket + peer-cred, so low real impact, but the two paths should be consistent.
*(Trivial fix applied — see below.)*

**P1-3 — reprompt hash comparison is not constant-time.**
`query.rs:90` compares the derived master-password hash with `!=` on byte slices. This is
the deferred item from the prior pass and is still open. Impact is bounded (the attacker is
already same-UID per A2), but a constant-time compare (`subtle::ConstantTimeEq`) is cheap
insurance. *Recommend:* schedule.

**P1-4 — session tokens and protected keys live in non-`mlock`'d heap.**
`Secret` (`db/models.rs:176`) wraps a plain `String`; access/refresh tokens and the
encrypted `protected_key` are stored this way in agent memory. They are zeroized on drop and
kept off disk, and `PR_SET_DUMPABLE(0)` blocks core dumps — but unlike `locked::Vec` they
are not `mlock`'d, so they can be paged to swap. Bearer tokens grant server access.
*Recommend:* move tokens into `locked::Vec`-backed storage, or document the residual as
accepted (encrypted-swap environments make it moot). Overlaps the prior deferred "IPC
plaintext lingers as String" item.

**P1-5 — no supply-chain gate.**
`cargo audit` / `cargo deny` are not wired (couldn't run here — not installed, and the
sandbox blocks network fetch). The tree pulls `tss-esapi 8.0.0-alpha.2`, an alpha crate on
the key-sealing path. Already tracked in `roadmap.md`; restating because it belongs to the
security surface and should land with the Phase 5 CI work.

### S3 — low

**P1-6 — browser host logs raw message body on parse failure.**
`browser_host.rs:42` logs `String::from_utf8_lossy(&buf)` at `debug!` when an `Action` fails
to deserialize. A near-miss message (valid-ish JSON carrying a `password`/`pin`) would land
verbatim in the journal under `RUST_LOG=debug`. *(Trivial fix applied — see below.)*

**P1-7 — third-party crates can log credentials at `RUST_LOG=trace`.**
Our own logging is redacted, but `reqwest`/`hyper`/`rustls` at `trace` can emit request
headers including the `Authorization: Bearer …` token. Same-UID only, and opt-in, but worth
a one-line note in `build_and_run.md` warning against `trace` on the agent, or a targeted
default filter that caps those crates at `info`.

**P1-8 — form-fill does not re-verify the active tab's domain.**
`content.js:9` fills every password field on whatever page is active when the popup sends
`FILL_FORM`; it trusts the user's pick and the popup's domain filter (M9) but does not
re-check that the current tab's origin matches the credential's domain at fill time. This is
standard autofill behaviour, but a domain re-check (or at least not auto-filling
cross-origin) is the SOTA bar. Messaging is runtime-only, so a hostile page cannot *trigger*
a fill — the risk is purely mis-fill into a look-alike tab. *Recommend:* Phase 4 UX item.

**P1-9 — clipboard copy has no auto-clear.**
Both the applet (`app/update/applet.rs:563`) and the main view (`app/update/mod.rs:126`),
and the extension (`popup-detail.js:49`), copy secrets to the clipboard with no timed clear.
Every mainstream password manager clears the clipboard after 10–30 s. Functional gap rather
than a code vuln. *Recommend:* Phase 4 (UX) with a security rationale; track here so it
isn't lost.

## Actions

- [x] **P1-2** fixed — main socket parent dir now created `0700` (matches SSH path).
- [x] **P1-6** fixed — raw parse-failed body no longer logged; only the serde error is.
- [ ] P1-1, P1-3, P1-4, P1-5 → added to `roadmap.md` (security section).
- [ ] P1-7 → doc note (Phase 8) or default-filter tweak.
- [ ] P1-8, P1-9 → Phase 4 UX backlog with security rationale.

## Gate assessment

No S0/S1. Prior remediation verified genuine. Two trivial hardening fixes applied; the rest
are scheduled. **Phase 1 gate: PASS** — proceed to Phase 2 (data integrity & correctness).
