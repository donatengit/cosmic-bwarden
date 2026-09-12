# Cosmarden — source-based security review (2026-08-22)

Reviewed: tip of this tree (2026-08-22). Method: read current shipped source
for every in-scope surface in `SECURITY.md`, re-verify the 2026-07-02 /
2026-07-04 claims against today’s code rather than trusting them, and
classify findings against the project’s documented same-UID desktop threat
model.

Severity: **S0** secret-leak / data-loss · **S1** correctness · **S2**
hardening · **S3** low.

Labels: **new** (not in `SECURITY.md` / `docs/review/01_security.md` /
`docs/roadmap.md` as an accepted residual or open item) vs **previously
documented accepted residual** vs **previously documented, still open** vs
**previously documented, fixed after 2026-07 (re-verified present)**.

This is not a copy or rename of `docs/review/01_security.md`. Claims below
are taken from today’s source. Same-UID hostile process (A2), root / kernel /
DMA, and the documented `rsa` Marvin advisory (RUSTSEC-2023-0071) are **not**
scored as new S0/S1.

---

## Verdict

**No S0. No S1.** The agent-centric architecture still matches the recorded
threat model: other-UID reachability is closed by `SO_PEERCRED` + `0600`
sockets + `0700` runtime dirs; bulk IPC reads redact `Secret`-typed fields
and notes; per-secret reads honour master-password reprompt; cipherstrings
are encrypt-then-MAC with MAC-required type-2 parse; TPM blobs are
PCR{0,7} ∧ PolicyAuthValue with DA lockout and encrypted unseal; the
extension cannot be driven by a web page into native messaging.

What is solid: the A1 (other-UID) boundary, the H2 bulk-redaction /
reprompt split for modeled secrets, TLS (`https` / loopback-only `http`),
log/`Debug` redaction of `Action`/`Response`/`Secret`, token `mlock`,
on-disk cache `0600` without session tokens, native-messaging origin
allowlist, and PSL domain matching on the agent.

What remains open: a set of **S2** hardening items — some carried from
2026-07 (non-constant-time reprompt, fill-time domain re-check, extension
clipboard lifetime), some new in this pass (`Secret` is not wiped on drop;
desktop UI fetches full `GetEntry` for the detail pane; `tpm::clear`
swallows unlink errors; the extension background is an unrestricted `Action`
proxy; decrypt still *accepts* a missing MAC if one is constructed
internally). None of these, on this threat model, rise to secret-leak or
data-loss S0.

Independent third-party audit: still none (`SECURITY.md`). Weigh that
before trusting a vault you cannot afford to lose.

---

## Threat model

Matches `SECURITY.md` and the 2026-07 recorded model. Trust boundary on a
single-user desktop is **same-UID**.

### Assets

| Asset | Where it lives when the vault is unlocked |
|---|---|
| Master password / PIN | Transient IPC `String`; UI form buffers (zeroized after use on the lock/login paths) |
| Derived vault keys (`enc_key` ‖ `mac_key`) | `locked::Keys` in agent `State` (`mlock` + zeroize-on-drop) |
| Decrypted entry secrets | Agent heap as `db::Secret`; desktop `selected_entry`; extension only on reveal / fill / edit |
| Session tokens | `locked::Token` in agent memory; Secret Service via `keyring.rs` when `persist_session` |
| SSH private keys | Decrypted in the SSH-agent accept path under the same `State` lock |
| TPM-sealed blobs | `<data_dir>/tpm_sealed_*.bin` (0600); unseal needs PIN + matching PCR{0,7} |
| Generator history | Device-global ciphertext + adjacent device key (documented weaker than the vault) |

### Attacker classes

| # | Attacker | Reachable surface | Primary defence | In scope? |
|---|----------|-------------------|-----------------|-----------|
| **A1** | Other local UID | Main IPC socket, SSH-agent socket, cache/data files, native-messaging host, D-Bus | `SO_PEERCRED` same-UID reject, sockets `0600`, dirs `0700`, Firefox `allowed_extensions` pin | **Yes** — a break is S0 |
| **A2** | Hostile process as the vault owner | Full IPC protocol, generator history, SSH agent, `ptrace` subject to `PR_SET_DUMPABLE` | *By design can read the vault once unlocked.* Reprompt and lock raise cost, they are not a wall | **Accepted residual** (`SECURITY.md`) — not scored as new S0/S1 |
| **A3** | Memory scraping (core dump / swap) | Agent RSS | `PR_SET_DUMPABLE=0` before secrets exist; `mlock` for keys/tokens; logind delay inhibitor on sleep | In scope for dumps / mlock failures |
| **A4** | Stolen disk (cold) | Vault JSON cache, TPM blob, generator key+history, `config.json` | Vault key never on disk as plaintext; tokens `#[serde(skip)]` + keyring; TPM PCR-bound | In scope |
| **A5** | Malicious web page | Content scripts, in-page bar, fill | Isolated world; no `externally_connectable`; fill on user gesture; save-bar never carries the password | **Yes** |
| **A6** | Compromised / MITM server | API, cipherstrings, KDF params | rustls native roots; `ensure_transport_security`; MAC-required type 2; Argon2id clamp | **Yes** |

A2 is the correct call for this architecture and is **not** a finding. Root,
kernel, and physical DMA are out of scope (`SECURITY.md`).

---

## Prior 2026-07 claims — re-verified against today’s tree

Used only as a checklist. Each row is current source, not the old review’s
word.

| ID | Claim | Today | Test / note |
|----|-------|-------|-------------|
| H1 | TPM PCR{0,7}, `userWithAuth=false`, DA on, encrypted unseal | **Holds** — `tpm/policy.rs` `pcr_selection_list`, `sealed_template`; `tpm/ops.rs` `unseal_with_policy` | tpm-smoke / `tpm_lifecycle/lockout.rs` |
| H2 | Bulk/meta redact secrets; per-secret reads gate reprompt | **Holds for `Secret`-typed fields + notes + hidden custom fields** — `query::redact_entry_secrets`, `handle_get_entries`, `handle_get_entry`, `ops::handle_get_totp` | `crates/cosmarden-tests/src/security.rs` `test_reprompt` |
| H3 | Manual `Debug` on `Action`/`Response` — no secret payloads | **Holds** — `protocol/debug_impls.rs`; `Secret` Debug/Display is `********` | `protocol/tests.rs` `action_debug_never_prints_secrets` |
| M4 | MAC-less type-2 rejected at parse | **Holds** — `CipherString::new` requires 3 parts | `cipherstring.rs` `type2_requires_mac` |
| M5 | SSH `SO_PEERCRED` + parent `0700` + socket `0600` | **Holds** — `SshAgentFactory::new_session`, `SshAgent::run` | `ipc_hardening.rs` `test_socket_file_modes` |
| M6 | `Db::save` 0600 temp + atomic rename | **Holds** — `db/persistence.rs` `Db::save` | `security.rs` `test_token_leakage` |
| M7 | Argon2id params validated + clamped | **Holds** — `identity::Identity::new` | four unit tests in `identity.rs` |
| M8 | Min-PIN + encrypted TPM session | **Holds** — `tpm_pin::validate_pin` (`MIN_PIN_LEN` = 6); `unseal_with_policy` encrypt/decrypt attrs | tpm-smoke |
| L1 | Non-digit cipherstring type no underflow | **Holds** | `non_digit_type_is_rejected_without_panic` |
| L2 | IPC request length capped (8 MiB) | **Holds** — `run()` accept loop `MAX_REQUEST_BYTES` | `ipc_hardening.rs` `test_oversized_request_is_rejected` |
| L4 | Email percent-encoded in cache path | **Holds** — `dirs::db_file` | three traversal tests in `dirs.rs` |
| P1-1 | Enforce `https://` on `base_url` | **Fixed after 2026-07** — `api/client/mod.rs` `ensure_transport_security` | transport_security unit tests |
| P1-2 | Main-socket parent dir `0700` | **Fixed after 2026-07** — `agent/lib.rs` `run` `DirBuilder::mode(0o700)` | `ipc_hardening.rs` |
| P1-4 | `mlock` session tokens | **Fixed after 2026-07** for tokens — `locked::Token`; `protected_*` stay `Secret` by design (ciphertext, serialized to cache) | `locked.rs` `token_tests` |
| P1-5 | `cargo audit` in CI | **Fixed after 2026-07** — `.github/workflows/ci.yml` + `.cargo/audit.toml` | — |
| P1-6 | Browser host must not log raw parse-failed body | **Fixed after 2026-07** — `browser_host::run` logs serde error only | — |
| P1-7 | Cap third-party HTTP-stack log verbosity | **Fixed after 2026-07** — `agent/lib.rs` `run` `filter_module`; `ui/main.rs` `setup_logs` | — |

Still open from that pass (restated below, not re-invented): **P1-3**
constant-time reprompt, **P1-8** fill-time domain re-check, **P1-9**
clipboard auto-clear on the *extension* (UI/applet are now 30 s).

---

## 1. Other-UID reachability (A1)

### Main IPC socket — holds

`cosmarden_agent::run` (`crates/cosmarden-agent/src/lib.rs`):

- `libc::prctl(PR_SET_DUMPABLE, 0)` on Linux **before** config-driven
  secrets are loaded into `State`.
- `dirs::make_all()` creates cache / runtime / data at `0700`.
- Socket parent is created `0700` even when `COSMARDEN_SOCKET` points
  at a fresh directory (P1-2).
- Bound socket is `chmod 0600`.
- Every `accept` path calls `UnixStream::peer_cred()` and compares
  `cred.uid()` to `rustix::process::getuid()`; mismatch logs `warn!` and
  `continue`s (connection dropped, no request read). Failed `peer_cred`
  is `error!` + drop.

No environment variable or CLI flag skips the UID check (tree grep:
no `danger_accept` / `INSECURE` / `verify_none` / `no_verify`).

### SSH-agent socket — holds

`SshAgentFactory::new_session` (`crates/cosmarden-agent/src/ssh_agent.rs`)
repeats the same-UID `peer_cred` check. Unauthorized sessions answer
`request_identities` as empty and `sign` as `"unauthorized peer"` — they
do not see key comments. `SshAgent::run` creates the parent `0700` and
the socket `0600`.

### D-Bus / logind — holds (client only)

`listen_to_logind` (`crates/cosmarden-agent/src/logind.rs`) connects
to the **system** bus as a client, `AddMatch`es `Session.Lock` /
`PrepareForSleep` / `PrepareForShutdown`, and calls `Inhibit` for a
delay fd so `State::lock()` can zeroize before a hibernate image.
It does **not** export an object, a name, or a method other UIDs can
invoke. A foreign session’s `Lock` broadcast would only lock this vault
(fail-safe), never unlock it.

### Native-messaging host — holds (A1)

Production install (`tests/browser-extension/register_host.py`
`register_firefox`, invoked from the justfile) writes
`~/.mozilla/native-messaging-hosts/com.enikeev.cosmarden.json` with
`allowed_extensions: ["cosmarden@enikeev.com"]` (the gecko id in
`browser-extension/manifest.json`). Pages cannot `connectNative`. There
is no `externally_connectable` and no `onMessageExternal`. Chrome
`allowed_origins` is pinned in the E2E helper
(`tests/browser-extension/playwright/chrome-full.spec.js`
`registerNativeHost`); `just install` does not register a Chrome host
(fail-closed packaging gap, not an A1 bypass).

The host process (`browser_host::run`) is still same-UID and then
connects to the main IPC socket, so A1 is the allowlist + the
`SO_PEERCRED` hop, not a reduced protocol (see S2-E1).

### Socket / dir modes — holds, with one S3 residual

| Path | Mode | Where |
|---|---|---|
| Main IPC socket | 0600 | `run` |
| SSH-agent socket | 0600 | `SshAgent::run` |
| Runtime / cache / data dirs | 0700 | `dirs::make_all` |
| Vault cache JSON | 0600 + tmp/rename | `Db::save` |
| TPM blob | 0600 | `tpm/blob.rs` `write_blob` |
| Generator key / history | 0600 | `handler/generator/storage.rs` |

`CosmardenConfig::save_legacy` (`crates/cosmarden-core/src/config.rs`)
uses `create_dir_all` + `File::create` (umask, typically 0644) and
`make_all` does not cover the config dir. Config holds email / URLs / TPM
flags, not tokens. **S3-1**, new, hardening.

---

## 2. Secrets leaving the agent

### Bulk / meta vs per-secret + master-password reprompt — holds for modeled secrets

`query::redact_entry_secrets` strips login password/TOTP, card number/CVV,
SSH private key, bank `account_number` / `routing_number` / `pin`,
driver-licence number, passport number, **notes**, and hidden custom
fields. `handle_get_entries` and `handle_get_entry_meta` decrypt then
redact; they do **not** call `verify_reprompt`. `handle_get_entry` and
`GetPassword` (via `handle_get_entry`) and `ops::handle_get_totp` do.

`merge::merge_redacted_secrets` treats incoming `None` on those secret
slots as “unchanged”, so a client cannot echo a bulk read through
`UpdateEntry` and wipe the server copy. Browser password updates use
`UpdateLoginPassword` (`browser_save::handle_update_login_password`),
which decrypts inside the agent.

**Close-out:** H2 holds for every field typed `db::Secret` plus notes and
hidden custom fields.

**S2-1 (new):** several high-sensitivity fields are plain `String` and
therefore survive bulk/meta, including on a `master_password_reprompt`
item: Identity `ssn` / `license_number` / `passport_number`
(`EntryData::Identity`); BankAccount `iban` / `swift_code` /
`branch_number`; Passport `national_identification_number` /
`date_of_birth`. `redact_entry_secrets` documents this as leaving
“identity/card non-secret fields”. That is inconsistent with
`account_number` / `passport_number` being `Secret`. The desktop detail
pane uses `GetEntry` (reprompt-gated); the extension detail uses
`GetEntryMeta` (`popup-detail.js` `showDetail`), so Identity SSN is in
JS `currentEntry` without a reveal. Not scored S1: it matches the
explicit 2026-07 “non-secret fields” split, but it is a modeling hole
for the new type-6–8 ciphers and for Identity PII.

### Logs / `Debug` — holds

- `Action` / `Response` have handwritten `Debug` in
  `protocol/debug_impls.rs` (variant name + non-secret scalars; never
  passwords, PINs, entries, TOTP, generated passwords, 2FA token).
- `db::Secret` Debug/Display is `********` (`db/models.rs`).
- `locked::Token` Debug is `********`.
- Agent logs `Received action: {:?}` at info (`handler::handle_request`)
  and request/response at debug (`run`). Browser-host parse failures log
  the serde error, not the body (`browser_host::run`).
- HTTP stacks capped at `info` in both agent (`run`) and UI
  (`setup_logs`), beating `RUST_LOG=hyper=trace`.

**S3-2 (new):** UI `Message` is `#[derive(Debug)]` and can carry
clipboard / password-change / generator strings. Nothing currently
`tracing`s a `Message`; iced debug tooling would. Residual, not a
journal leak today.

### On-disk cache / keyring / generator history — holds, with documented residuals

- `Db` `#[serde(skip)]` on `access_token` / `refresh_token`
  (`db/persistence.rs`). `test_token_leakage` asserts the JSON has
  neither key.
- `protected_key` / `protected_private_key` / entries stay ciphertext.
- `keyring::store_tokens` writes to Secret Service (`oo7`) when the
  `keyring` feature is on; `delete_tokens` on logout. Callers
  `log::error!` on failure.
- Generator history: AES-256-CBC + HMAC via `CipherString::encrypt_symmetric`,
  device-global key at `dirs::generator_key_file()` mode 0600, 7-day prune
  on read/append (`handler/generator/storage.rs`). **Accepted residual
  (documented in `AGENTS.md`):** the key sits next to the ciphertext;
  same-UID can decrypt; generation (and history dump) work with the vault
  **locked** (`handler.rs` dispatch). That is weaker than vault lock on
  purpose.

**S2-2 (new, availability):** `keyring::store_tokens` falls back to
creating a `"cosmarden"` collection when `default_collection` fails;
`get_tokens` only searches the default. A store/get mismatch loses
session restore after PIN unlock (sync, not vault decrypt). Callers log
store failures; the mismatch itself is silent success.

### Clipboard lifetime — UI holds; extension still open

UI/applet: `copy_to_clipboard_with_autoclear`
(`crates/cosmarden-ui/src/app/update/mod.rs`),
`CLIPBOARD_CLEAR_SECS = 30`, generation counter, readback wipe only if
the clipboard still holds our value, `zeroize` of the pending copy.
Applet goes through `applet_copy_to_clipboard` → the same helper.

Extension: `navigator.clipboard.writeText` in `popup-detail.js`
`makeCopyBtn`, `popup-list-actions.js` Copy Password, and
`content-generate.js` `GENERATE_COPY_TO_CLIPBOARD`, with **no** timer.
**P1-9, previously documented, still open on the extension** (UI half is
fixed after 2026-07).

---

## 3. Crypto

### Cipherstring parse / MAC / encrypt-then-MAC — holds, with one decrypt-side hole

`CipherString::new` (`crates/cosmarden-core/src/cipherstring.rs`):

- Type byte must be a single ASCII digit (L1).
- Type 2 requires exactly `iv|ct|mac` (M4). MAC-less is `InvalidCipherString`.
- Types 4 and 6 are RSA (asymmetric); anything else is too-old or unimplemented.

`encrypt_symmetric` HMACs `iv ‖ ciphertext` then stores the MAC
(encrypt-then-MAC). `decrypt_common_symmetric` verifies with
`hmac::Mac::verify` (constant-time) **when a MAC is `Some`**. IVs are 16
random bytes (`random_iv`, `rand::rng()` — CSPRNG ThreadRng; the generator
path separately requires `OsRng`).

PKCS7 padding is checked only after MAC verify, so a MAC-less padding
oracle is not reachable from `CipherString::new`. Mini-fuzz
`arbitrary_input_never_panics` covers attacker-shaped parse input (A6).

**S2-3 (new):** `decrypt_common_symmetric` still decrypts if `mac` is
`None`. Parser and `encrypt_symmetric` never produce that, but the
decrypt path is not defense-in-depth. Refusing `None` would match M4.

RSA private-key unwrap is OAEP-SHA1 (`decrypt_locked_asymmetric`).
RUSTSEC-2023-0071 (PKCS#1 v1.5 Marvin) remains an **accepted residual**
in `SECURITY.md` / `.cargo/audit.toml` — not scored as new S0/S1.

### KDF clamp — holds for Argon2id memory/parallelism; iterations unbounded

`Identity::new` (`crates/cosmarden-core/src/identity.rs`):

- Iterations must be `NonZeroU32`.
- Argon2id requires memory and parallelism; memory clamped to 16..=1024
  MiB, parallelism 1..=16 (Bitwarden’s range). Missing params error, no
  `unwrap`.

**S2-4 (new):** neither PBKDF2 nor Argon2id **iteration count** has an
upper bound. A hostile prelogin (A6) can set `iterations` near `u32::MAX`
and stall unlock. Memory/parallelism DoS is already closed.

### TOTP — holds

`handler/vault/totp.rs` `build` / `generate_code`: bare seed is
base32-decoded (not used as raw ASCII); `otpauth://` honours
algorithm/digits/period via `from_url_unchecked`; short 80-bit seeds
accepted to match real authenticators. RFC 6238 vector tests included.
`handle_get_totp` runs `verify_reprompt` before `generate_code`.

### Zeroize / `mlock` — mixed (keys/tokens hold; `Secret` does not)

`locked::Vec` (`crates/cosmarden-core/src/locked.rs`): `mlock` of a
fixed 4 KiB region, `zero()` + `zeroize` on `Drop`, degrades to unlocked
heap with `warn!` if `RLIMIT_MEMLOCK` is exhausted. `Keys`, `Password`,
`PasswordHash`, `PrivateKey` wrap it. `Token` uses the locked buffer up
to 4 KiB, else a zeroize-on-drop heap `String` with a warning;
`From<String>` zeroizes the source.

`State::lock` (`crates/cosmarden-agent/src/state.rs`) drops
`keys` / `org_keys` / `master_password_hash` (locked types wipe) and
clears `access_token` / `refresh_token`. Encrypted `db.entries` stay for
offline unlock. Logind delay inhibitor aims to finish this before
hibernate.

**S2-5 (new; contradicts the 2026-07 P1-4 prose “zeroized on drop”):**
`db::Secret` implements `Zeroize` but **not** `Drop` / `ZeroizeOnDrop`
(no `ZeroizeOnDrop` anywhere in the tree). Decrypted passwords, notes,
card numbers, etc. are ordinary `String`s; dropping `Response::Entry` /
`selected_entry` / a CLI `Entry` frees them without wipe. Explicit
`.zeroize()` is used on UI *form* buffers (`login_password`, PINs,
`clipboard_pending_clear`), not on `Secret`. This is A3 residual (dumps
are off; swap still exists), not an A1 leak.

---

## 4. TPM path (seal / PIN / DA / PCR)

Holds for the named invariants:

| Invariant | Where |
|---|---|
| PCR{0,7} SHA-256 | `tpm/policy.rs` `pcr_selection_list` |
| Policy digest = PolicyPCR ∧ PolicyAuthValue | `compute_policy_digest` |
| `userWithAuth=false` (PIN only via policy) | `sealed_template` |
| DA lockout on (`with_no_da` never set) | `sealed_template` comment + attributes |
| Encrypted unseal session | `ops::unseal_with_policy` `.with_decrypt(true).with_encrypt(true)` |
| Blob version 2; v1 refused | `tpm/blob.rs` `SEALED_BLOB_VERSION`, `read_blob` |
| Blob 0600 | `write_blob` |
| `MIN_PIN_LEN = 6`, agent-enforced on setup | `core::MIN_PIN_LEN`; `tpm_pin::validate_pin` |
| Wrong PIN vs PCR change vs lockout | `tpm::classify_unseal_failure` → `ERR_TPM_UNSEAL_FAILED` / `ERR_TPM_STATE_CHANGED`; UI uses `GetTpmDaStatus` |
| Server-credentials blob empty PIN, still PCR-bound | `handle_enable_tpm_server_credentials` → `seal_bytes(..., "")`; documented trade-off on `CosmardenConfig.tpm_store_server_credentials` |

`handle_unlock_with_pin` unseals vault keys under the PIN, optionally
unseals the hash blob with `""`, restores tokens from keyring / in-memory
pre-lock copy, and `error!`s if neither token nor hash is available so
sync failure is visible.

**S2-6 (new):** `tpm::clear` does `let _ = std::fs::remove_file(blob_path)`
and **always** returns `Ok(())`, then `info!`s “cleared”.
`handle_disable_tpm_pin` has `if let Err(e) = crate::tpm::clear(...)`
error logging that can never fire. It then sets `tpm_enabled = false` /
`tpm_configured = false` and returns `Ack`. `handle_unlock_with_pin`
does not consult those flags — it unseals whatever file still exists.
A failed unlink leaves PIN unlock live after a successful “disable”.
Violates the AGENTS.md “no silent security failures” bar on the revoke
path. (Need unlink to actually fail; rare.)

**Accepted residual:** empty-PIN server-credentials blob is opt-in;
same TPM + matching PCRs can unseal the *hash* without the vault PIN,
which authenticates to the server but does not decrypt the vault.

---

## 5. Browser extension

### Page reachability of vault data — holds

- Manifest: no `externally_connectable`, no `web_accessible_resources`,
  content scripts default isolated world, `all_frames` unset (top frame
  only), `host_permissions: <all_urls>` required for fill.
- Only `background.js` `connect()` calls `browser.runtime.connectNative`.
- `content.js` `FILL_FORM` is `runtime.onMessage` (extension-only). A
  page cannot send it.
- Save bar (`content-bar.js` `showSaveBar`) uses `textContent` (no HTML
  injection) and an open shadow root that **never receives the
  password**. Page JS can click/remove the host; that only confirms a
  save of credentials the page already has (accepted, documented in the
  file comment).

Autofill writes the password into the page DOM — that is the feature
(A5 accepted for the filled origin).

### Domain matching / PSL — holds for list / badge / save; fill-time still open

The extension does **not** compute eTLD+1. `background.js` /
`popup.js` `extractDomain` strip only a leading `www.` and send the full
host. Matching is `domain::hosts_match`
(`crates/cosmarden-core/src/domain.rs`): exact, label-boundary
subdomain both ways, then PSL eTLD+1 when the `public_suffix_list`
feature is on (agent **default** features include it —
`cosmarden-agent/Cargo.toml`). `evil.co.uk` vs `mybank.co.uk` does
not match; IPs/dotless hosts match exactly. Save-prompt uses the same
helper (`browser_save::entry_matches_domain`).

**P1-8, previously documented, still open:** `popup.js` `fillEntry`
`GetEntry`s then `tabs.sendMessage` to the **current** active tab with
no `hosts_match` against `tab.url`. The Fill button is offered on every
Login row (search/favourites included). Mis-fill into a look-alike or a
tab that navigated is the residual. Messaging is still runtime-only, so
a hostile page cannot *trigger* fill.

**S2-7 (new, adjacent to P1-8):** `content.js` `fillForm` writes the
password into **every** `input[type=password]` in the top document,
including hidden / honeypot / `current-password` fields.
`findUsernameInput` uses a visibility helper; the password loop does
not.

### Secrets in JS state — holds for the documented invariant, with S3 draft residual

| Path | Secrets? |
|---|---|
| List / badge | `GetSidebarEntries` → `SidebarEntry` (id, name, username, public_key) |
| Detail | `GetEntryMeta` then `GetPassword` / `GetTotp` on reveal/copy (`popup-detail.js` `showDetail`, `makeSecretRow`) |
| Fill / edit | `GetEntry` on explicit gesture (`fillEntry`, `showEdit`) |
| Save prompt | Password stays in background `storage.session` (`background-save.js` `setPendingSave`); `SHOW_SAVE_BAR` is mode/domain/username/entryName only |

`popup-state.js` persists an **edit draft** (including password field
values) in `storage.session`, scoped to the tab domain, and **clears on
lock**. Session storage is not content-script visible (`setAccessLevel`
never raised). **S3-3 (new):** opening Edit copies the stored vault
password into that draft, not only a user-typed value. Process-lifetime,
same class as pending-save.

### Save-prompt path — holds the AGENTS.md invariants

- Comparison is `CheckLoginMatch` inside the agent
  (`browser_save::handle_check_login_match`); stored secrets never go to
  JS for the decision.
- Update is `UpdateLoginPassword`, never `UpdateEntry` of a meta read.
- Locked bar 30 s auto-dismiss does **not** send `dismiss`
  (`content-bar.js`); pending stays until TTL or explicit Dismiss.
- TTL restarted **once** at first `awaitingUnlock`
  (`evaluatePendingSave`).
- `VAULT_UNLOCKED` re-evaluates deferred tabs (`onVaultUnlocked`).
- `browser.alarms` + `storage.session` so a Chrome MV3 worker restart
  does not drop the pending.

**S2-8 (new):** `onBarAction` `clearPendingSave`s **before** checking
Ack. An agent `Error` (or throw) drops the capture; the bar cannot
retry. Integrity of the *prompt*, not of the vault (failed write does
not mutate the server). The password may still sit in the page form.

### Unrestricted agent proxy — S2-E1 (new)

`background.js` `runtime.onMessage` handles `SetTheme`,
`LOGIN_SUBMITTED`, `SAVE_BAR_ACTION`, `Lock`/`Logout`, `VAULT_UNLOCKED`,
then **`return sendToAgent(message)`**. `browser_host::run` forwards any
deserialized `Action`. Content scripts are extension senders:
`content-generate.js` already sends `GeneratePassword` this way. There
is no `sender.tab` allowlist that would stop a content script from
`GetPassword` / `GetEntry` / `Quit`.

**Today this is not a page exploit** (no `eval` of page data, no
`window.postMessage` bridge). It is defense-in-depth: a future isolated-
world bug becomes a full vault client. The native host is not a reduced
protocol.

---

## 6. IPC / protocol framing

Holds for requests:

- 4-byte little-endian length prefix, then postcard `Action`.
- `MAX_REQUEST_BYTES = 8 MiB` in `run`; oversized → `error!` + drop
  connection, no allocation of the claimed size
  (`ipc_hardening.rs` `test_oversized_request_is_rejected`).
- Garbage body → `Response::Error { "invalid request: …" }` then the
  task returns (`test_garbage_request_gets_error_response`).
- `protocol/tests.rs` `action_decode_from_arbitrary_bytes_never_panics`
  (10k seeded postcard decodes).
- Browser host: 1 MiB cap, JSON `Action`, same “error + continue”
  behaviour; parse failures do not log the body.
- Persistent connections: one `tokio::spawn` per socket, inner loop;
  `Subscribe` stays long-lived. Failed writes `error!`.

**S2-9 (new vs L2):** response length is `u32` with **no** cap.
`AgentClient::do_send` (`crates/cosmarden-core/src/agent_client.rs`)
does `vec![0u8; len]` on the claimed size. Same-UID can already speak
the protocol (A2); a hostile or buggy agent can force a ~4 GiB alloc in
UI/CLI. Browser-host inbound is the 1 MiB cap only.

Malformed frames do not panic the agent. Subscribe events are
`Locked` / `Unlocked` / `VaultChanged` / `UnlockRequested` /
`PinRequested` / `OpenEntry { id }` — no secret payloads
(`protocol.rs` `Event`).

---

## AGENTS.md core invariants — current-code verdict

| Invariant | Verdict | Evidence |
|---|---|---|
| `PR_SET_DUMPABLE=0` on daemon startup | **Holds.** Return value is not checked (**S3-4**, new). | `run`, Linux `prctl` |
| IPC `SO_PEERCRED` + socket `0600` | **Holds** on main and SSH sockets. Parent dirs `0700`. | `run`; `SshAgentFactory::new_session`; `SshAgent::run` |
| Memory-locked key material | **Holds for vault keys, org keys, master-password hash, session tokens.** `db::Secret` (decrypted entry plaintext) is heap `String`, not `mlock`’d — documented P1-4 family residual, plus S2-5 (no drop wipe). | `locked.rs`; `State::lock` |
| No silent security / data-loss failures | **Mostly holds** (API `Client::request_failed` `error!`; decrypt `warn!` with entry id + field; `Db::save` / keyring / socket-write `error!`; fallback-from-load `error!`). **Gap: S2-6 `tpm::clear`.** | see those functions |
| Thin-client (UI/CLI not holding long-lived plaintext secrets) | **Holds for applet** (`GetSidebarEntries` + on-demand `GetPassword`) **and extension detail** (`GetEntryMeta`). **Does not hold for the main window or CLI `get`:** `vault_actions::fetch_entry` is `GetEntry`; `selected_entry` keeps the decrypted `Entry` for the viewing session; CLI `commands/vault.rs` `Get` without `--all` always `GetEntry` then masks stdout unless `--show-secrets`. Lock button (`LockResult`) clears `selected_entry` and form buffers; autolock `Event::Locked` (`lifecycle.rs`) clears `selected_entry` but not generator history / `notes_content` / form PINs (**S2-10**, new). | cited files |
| Secret redaction in logs / `Debug` | **Holds** for `Action`, `Response`, `Secret`, `Token`. UI `Message` Debug is the S3 residual. | `protocol/debug_impls.rs`; `db/models.rs` |

---

## Findings (this pass)

### S0 / S1

None.

### S2 — new

| ID | Title | Citation |
|----|-------|----------|
| **S2-1** | Bulk/meta redaction misses Identity PII and BankAccount IBAN/SWIFT (plain `String`, including on reprompt items) | `query::redact_entry_secrets`; `EntryData::{Identity,BankAccount,Passport}` in `db/models.rs` |
| **S2-5** | `db::Secret` is not wiped on drop | `db/models.rs` `Secret` — `Zeroize` impl, no `Drop` |
| **S2-6** | `tpm::clear` swallows unlink errors; disable PIN still `Ack`s; unlock ignores `tpm_enabled` | `tpm::clear`; `handle_disable_tpm_pin`; `handle_unlock_with_pin` |
| **S2-E1** | Extension background + browser-host forward *any* `Action`; content scripts share that proxy | `background.js` `onMessage` fallthrough `sendToAgent`; `browser_host::run` |
| **S2-3** | Decrypt path still accepts a missing MAC | `decrypt_common_symmetric` |
| **S2-4** | KDF iteration count unbounded (A6 DoS on unlock) | `Identity::new` |
| **S2-7** | Fill writes every password input, including hidden | `content.js` `fillForm` |
| **S2-8** | Save-prompt pending dropped before agent Ack | `background-save.js` `onBarAction` |
| **S2-9** | IPC **response** length uncapped (request is 8 MiB) | `run` write path; `AgentClient::do_send` |
| **S2-10** | Autolock `Event::Locked` does not wipe generator/notes/form secrets as thoroughly as `LockResult` | `lifecycle.rs` `EventReceived(Locked)` vs `auth.rs` `LockResult` |
| **S2-2** | Keyring store/get collection mismatch | `keyring::store_tokens` vs `get_tokens` |

Main-window `GetEntry` on select is invariant drift vs the protocol
comment on `GetEntryMeta` (“use for detail/view UI”). It is the product
choice that makes S2-10 and S2-5 matter in the UI process (which does
not set `PR_SET_DUMPABLE`). Not a cross-UID leak.

### S2 — previously documented, still open

| ID | Title | Citation |
|----|-------|----------|
| **P1-3** | Reprompt hash compare is `!=` on byte slices, not `subtle::ConstantTimeEq`. Bounded: attacker is already A2. | `query::verify_reprompt` |
| **P1-8** | Fill does not re-check the active tab’s host against the credential | `popup.js` `fillEntry` |
| **P1-9** | Extension clipboard has no auto-clear (UI/applet **fixed**, 30 s) | `popup-detail.js` `makeCopyBtn`; `content-generate.js` |

`CheckLoginMatch` (`p.expose() == password`) is the same non-CT class as
P1-3 on a value the client already holds.

### S3 — new

| ID | Title | Citation |
|----|-------|----------|
| **S3-1** | `config.json` / config dir not forced `0600`/`0700` | `CosmardenConfig::save_legacy` |
| **S3-2** | UI `Message` derived `Debug` can carry secrets | `crates/cosmarden-ui/src/message.rs` |
| **S3-3** | Edit `savePopupState` copies a stored password into `storage.session` | `popup-state.js` `snapshotPopupState` |
| **S3-4** | `prctl(PR_SET_DUMPABLE, 0)` return ignored | `run` |
| **S3-5** | Autolock poll is 5 minutes, so a 5-minute timeout can fire at 5–10 minutes | `timeout.rs` `CHECK_INTERVAL` |
| **S3-6** | CLI `get` fetches `GetEntry` even when stdout is masked | `commands/vault.rs` `Commands::Get` |
| **S3-7** | `openEntrySite` accepts any `scheme://` URI from the vault (e.g. `javascript:`) on user click | `popup-list-actions.js` `openEntrySite` |

### Previously documented accepted residuals (not findings)

- **A2** same-UID full IPC (including generator history while locked, SSH
  sign, native-host as another same-UID client).
- Unencrypted swap paging tokens that exist only for the seconds of an
  HTTP request (roadmap P1-4 accepted remainder).
- Root / kernel / DMA.
- **RUSTSEC-2023-0071** (`rsa` Marvin) — `.cargo/audit.toml`, `SECURITY.md`.
- Generator device-key-beside-ciphertext (`AGENTS.md` Password Generator).
- TPM server-credentials empty PIN, PCR-bound, opt-in.
- Autofill leaves the password in the page’s form fields (A5, inherent).
- `tss-esapi 8.0.0-alpha.2` on the seal path (roadmap; still true).

---

## Environment variables (requested check, re-verified)

No env var or CLI flag disables a security control. Recognised vars only
select a path, a profile namespace, or log verbosity, all within the
invoking user’s privilege:

| Var | Effect |
|-----|--------|
| `COSMARDEN_CONFIG` / `_SOCKET` / `_SSH_SOCKET` | path overrides; sockets still `0600` + peer-cred |
| `COSMARDEN_PROFILE` | namespaces dirs created `0700` via `make_all` |
| `COSMIC_PANEL_NAME` | applet vs window |
| `RUST_LOG` | log level; HTTP crates still capped at `info` |
| `TSS2_TCTI` | TPM device (tests / operator); same-UID |

---

## What this review did not do

Live pentest, fuzzing campaign, formal verification, `cargo audit`
network fetch, TPM hardware, or the nine-phase product review
(`docs/review/02_*`–`08_*`). Bitwarden/Vaultwarden server bugs are out
of scope. `tmp_code_examples/` and `kb/` were not treated as product
code. Existing tests in `security.rs` / `ipc_hardening.rs` / TPM / SSH /
extension unit tests were used as evidence, not as a substitute for
reading the shipped paths.
)
