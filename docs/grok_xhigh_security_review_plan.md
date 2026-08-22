# COSMIC BWarden — remediation plan for the 2026-08-22 xhigh security review

Input: [`docs/grok_xhigh_security_review.md`](grok_xhigh_security_review.md)
(findings as of 2026-08-22). This file is the **fix plan**, not a restatement
of that review and not a patch. No application code is changed by landing this
document.

**S0 / S1:** none. There is nothing to remediate at those severities.

Each item below names an **observable outcome** after the change, a **UX
impact** (none, or the user-visible difference), and an **automated-test
impact** on the *existing* suites. “Harder” means those suites become flakier,
slower, or need a new kind of harness (timing, TPM fault injection,
constant-time measurement, live clipboard, real-browser fill). Adding a
normal unit/vitest/Playwright case of the kind we already run is **same**, not
harder.

---

## Out of this pass (accepted residuals — do not schedule as work)

These are recorded so they are not forgotten and are **not** in the waves
below. Treat a PR that “fixes” one of them as out of scope unless the threat
model itself changes.

| Residual | Why it stays |
|---|---|
| **A2** same-UID full IPC (generator history while locked, SSH sign, native-host as another same-UID client) | Documented architecture. Reprompt and lock raise cost; they are not a wall. |
| **RUSTSEC-2023-0071** (`rsa` Marvin) | No patched `rsa` 0.9 release. Keep the ignore in `.cargo/audit.toml` until RustCrypto ships a fix. |
| Generator **device-key-beside-ciphertext** | Deliberate: generation must work with the vault locked and with no account. Same 0600-file class as the vault JSON cache. |
| TPM **empty-PIN server-credentials** blob | Opt-in, still PCR{0,7}-bound. Unseals the hash for server re-auth, not the vault keys. |
| **Autofill leaves the password in the page** | Inherent to fill (A5). Do not try to “clear the DOM after fill.” |
| **`tss-esapi` 8.0.0-alpha.2** | Roadmap pin; bump when a stable crate exists. Not a finding in this review. |
| Unencrypted swap of **request-scoped** token copies | P1-4 accepted remainder; threading `locked::Token` through reqwest is not realistic. |
| Root / kernel / DMA | Out of `SECURITY.md` scope. |

---

## Recommended order

Cheap, no-UX, unit-testable items first. User-visible or awkward-to-prove
items last. Do not start Wave 3 until Wave 1 is in; Wave 2 can overlap Wave 1
once the crypto/agent pieces compile.

### Wave 1 — no UX, existing unit / IPC suites absorb

| Order | ID | Why first |
|---|---|---|
| 1 | **S2-3** | One `None` reject in `decrypt_common_symmetric`; parser already forbids it. |
| 2 | **S2-5** | `ZeroizeOnDrop` on `db::Secret`; no protocol or UI change. |
| 3 | **P1-3** | `ConstantTimeEq` on the reprompt (and `CheckLoginMatch`) byte compare. |
| 4 | **S3-4** | Check `prctl` return; log `error!` on failure. |
| 5 | **S3-1** | `0600` file + `0700` parent in `save_legacy`. |
| 6 | **S3-2** | Handwritten `Debug` on UI `Message`, matching `Action`/`Response`. |
| 7 | **S2-9** | Cap IPC **response** length at the same 8 MiB as requests. |
| 8 | **S2-4** | Clamp/reject KDF iterations to Bitwarden’s published range. |
| 9 | **S2-6** | `tpm::clear` must fail closed; disable must not `Ack` a live blob; unlock must honour `tpm_enabled`. |
| 10 | **S2-2** | Store and get tokens against the same Secret Service collection. |
| 11 | **S2-10** | One wipe helper for `LockResult` and `Event::Locked`. |
| 12 | **S3-6** | CLI `get` without `--show-secrets` sends `GetEntryMeta`. |

### Wave 2 — small, named UX; existing vitest / Playwright / UI tests absorb

| Order | ID | Why here |
|---|---|---|
| 13 | **S2-1** | Redact Identity PII / IBAN-class fields in bulk/meta; extend merge-None-means-unchanged. |
| 14 | **S2-8** | Drop the save-prompt pending only after agent `Ack`. |
| 15 | **S2-7** | Fill only visible password inputs (reuse `isVisible`). |
| 16 | **S3-7** | `openEntrySite` allows `http:`/`https:` only. |
| 17 | **S3-3** | Edit popup state does not persist a stored vault password. |
| 18 | **S3-5** | Autolock poll closer to the configured timeout. |

### Wave 3 — user-visible or easy to over-test

| Order | ID | Why last |
|---|---|---|
| 19 | **S2-E1** | Background + browser-host allowlist: content scripts cannot `GetEntry` / `Quit`. |
| 20 | **P1-8** | Fill re-checks the active tab host against the credential before `FILL_FORM`. |
| 21 | **P1-9** | Extension clipboard auto-clear at 30 s (match UI/applet). Use fake timers; do **not** add a 30 s E2E. |
| 22 | **R-1** | Desktop detail selects via `GetEntryMeta`; secrets on explicit reveal. Largest UX change; also shrinks S2-5 / S2-10 blast radius in the UI process. |

`PROTOCOL_VERSION` stays put for every Wave 1–2 item and for R-1 if it reuses
existing actions. A new `Action` for per-field identity reveal (only needed if
`GetEntry` on reveal is rejected) **would** bump the protocol — prefer not to
add one.

---

## Related product choice (no finding ID in the review)

### R-1 — desktop (and CLI-adjacent) detail uses `GetEntry` instead of `GetEntryMeta`

**Where:** `vault_actions::fetch_entry` always builds `Action::GetEntry`;
`protocol.rs` documents `GetEntryMeta` as “use for detail/view UI”; the
extension already does that (`popup-detail.js` `showDetail`). The UI process
does not set `PR_SET_DUMPABLE`, so a long-lived `selected_entry` with
plaintext `Secret`s is the reason S2-5 and S2-10 matter on the desktop.

**Observable outcome:** `fetch_entry` for a plain selection sends
`GetEntryMeta`. Password / TOTP / card number / SSH private key / newly
redacted PII (S2-1) are fetched only from a reveal or edit builder
(`GetPassword`, `GetTotp`, or `GetEntry` with the reprompt password). The
existing `vault_actions.rs` unit tests assert the variant; they flip from
`GetEntry` to `GetEntryMeta` for the selection path. Reprompt fires on
**reveal/edit**, not on merely opening the row.

**UX:** named change. Selecting a login no longer puts the password on screen
(or in `selected_entry`) until the user clicks reveal — same pattern as the
extension. A `master_password_reprompt` item no longer prompts until that
click. First reveal pays one extra IPC round-trip. Copy-without-reveal can
keep using `GetPassword` without updating the pane.

**Automated-test impact: same.** Absorb into `vault_actions.rs` (already the
required seam) and `app/tests/lifecycle.rs` / detail interaction tests. Do
not add a live-agent UI E2E just to watch `GetEntryMeta`. The E2E suite that
hand-builds `GetEntry` is unaffected (it is not the desktop builder).

Folded here rather than as extra IDs under S2-5 / S2-10 / S3-6; those items
still have their own rows.

---

## S2 — new findings

### S2-1 — Bulk/meta redaction misses Identity PII and BankAccount IBAN/SWIFT

**Where:** `query::redact_entry_secrets`; `EntryData::{Identity,BankAccount,Passport}`
in `db/models.rs`; `merge::merge_redacted_secrets`.

**Observable outcome:** `GetEntries` / `GetEntryMeta` return `None` for at
least: Identity `ssn`, `license_number`, `passport_number`; BankAccount
`iban`, `swift_code`, `branch_number`; Passport
`national_identification_number`, `date_of_birth`. `merge_redacted_secrets`
treats incoming `None` on those slots as “unchanged” (same rule as login
password), so echoing a meta read through `UpdateEntry` cannot wipe the
server copy. `GetEntry` (reprompt-gated) still returns them.

Do **not** add a new `Action` for this. Promoting the fields to `db::Secret`
is optional follow-on (helps S2-5); redaction + merge is the security
outcome. No `PROTOCOL_VERSION` bump: the `Entry` schema does not gain fields.

**UX:** named change on **CLI list / `get` without `--show-secrets`**: IBAN /
SSN-class values that currently print from a bulk read become blank; use
`--show-secrets` (which already sends `GetEntry`). Extension identity detail
(`popup-detail.js` `renderDetail`) does not currently render SSN, so the
visible popup does not change; `currentEntry` simply no longer carries those
strings. Desktop detail is unchanged until R-1 (it uses `GetEntry`). If a
later popup row shows SSN, it must use an on-demand fetch (reveal), not meta.

**Automated-test impact: same.** Extend agent-side unit coverage of
`redact_entry_secrets` / `merge_redacted_secrets` and
`crates/cosmic-bwarden-tests/src/security.rs` `test_reprompt` (meta must not
include the new slots; `GetEntry` after a correct password still does).
`cli_secret_mask_test.rs` should assert IBAN/SSN are absent without
`--show-secrets`. No new harness.

### S2-2 — Keyring store/get collection mismatch

**Where:** `keyring::store_tokens` falls back to
`create_collection("cosmic-bwarden")`; `get_tokens` / `delete_tokens` only
open `default_collection`.

**Observable outcome:** store, get, and delete share one collection helper.
If `default_collection` fails, **do not** create a second collection that
get cannot see — return `Err` so the existing `log::error!` at the callers
(`login.rs`, `unlock.rs`, `tpm_pin/unlock.rs`, `server/auth.rs`) fires.
A `store_tokens` that returns `Ok` is visible to a subsequent `get_tokens`
for the same server+email.

**UX:** named change on a rare path. If Secret Service has no default
collection, “remember session” / PIN-unlock token restore **fails loudly**
instead of storing into a collection that will never be read. The user
unlocks the keyring or creates the default collection; vault decrypt is
unaffected.

**Automated-test impact: same** if the collection decision is a pure helper
with a unit test (default vs error — no live D-Bus). **Harder** if someone
adds a live Secret Service E2E — do not. Existing suites have no keyring
round-trip; leave it that way.

### S2-3 — Decrypt path still accepts a missing MAC

**Where:** `cipherstring.rs` `decrypt_common_symmetric`.

**Observable outcome:** `mac: None` returns `Error::InvalidMac` (or a dedicated
“MAC required” error) and does not construct a `Decryptor`.
`CipherString::new` already rejects MAC-less type 2 (`type2_requires_mac`);
this is defense-in-depth for internally constructed `Symmetric { mac: None }`.

**UX:** none. Real vault ciphertext is type-2 with a MAC.

**Automated-test impact: same.** One unit test next to `type2_requires_mac`.
Existing parse/mini-fuzz tests keep passing. Do not add a padding-oracle
timing test.

### S2-4 — KDF iteration count unbounded (A6 DoS on unlock)

**Where:** `identity::Identity::new`.

**Observable outcome:** iteration count is rejected (same style as Argon2id
memory/parallelism, which already error out of range) when outside
Bitwarden’s published maxima: **PBKDF2 100_000..=2_000_000**, **Argon2id
iterations 2..=10**. Zero is already rejected. Hostile prelogin
`iterations ≈ u32::MAX` fails fast with a typed error instead of stalling
unlock. Legitimate Bitwarden-range accounts are unchanged.

**UX:** named change only for a self-hosted account configured **above** those
caps: unlock/login returns an error naming the cap, rather than hanging.
Document the cap next to the memory/parallelism messages. Default 600k / 3
is unaffected.

**Automated-test impact: same.** Four existing tests in `identity.rs`; add
“too-large PBKDF2 / Argon2id iterations rejected.” Do not add a test that
actually runs a huge KDF.

### S2-5 — `db::Secret` is not wiped on drop

**Where:** `db/models.rs` `Secret` — `Zeroize` impl, no `Drop` /
`ZeroizeOnDrop`.

**Observable outcome:** `Secret` implements `ZeroizeOnDrop` (or an equivalent
`Drop` that `zeroize`s the inner `String`). Dropping `Response::Entry`,
`selected_entry`, or a CLI `Entry` wipes those buffers. `expose()` behaviour
and `Debug`/`Display` `********` stay as they are.

**UX:** none.

**Automated-test impact: same.** A unit test can assert the trait (or that
`zeroize` + drop of a `ManuallyDrop` clone leaves zeros **before** free). Do
**not** try to read freed heap in E2E. Existing `protocol/tests.rs`
`action_debug_never_prints_secrets` is unrelated and stays.

### S2-6 — `tpm::clear` swallows unlink errors; disable still `Ack`s; unlock ignores `tpm_enabled`

**Where:** `tpm::clear`; `handle_disable_tpm_pin`; `handle_unlock_with_pin`.

**Observable outcome:**

1. `tpm::clear` returns `Ok(())` if the path is already absent (`NotFound`);
   any other `remove_file` error is `Err` (and is **not** preceded by
   “cleared”).
2. `handle_disable_tpm_pin` does **not** set `tpm_enabled = false` /
   `tpm_configured = false` and does **not** return `Ack` if either vault-key
   blob `clear` failed. The existing `error!` branch can now fire.
3. `handle_unlock_with_pin` refuses with a clear error when
   `!config.tpm_enabled` (and/or `!state.tpm_configured`), even if a sealed
   file is still on disk.

Both (2) and (3): disable is honest about revoke; flags are the authorization
gate if a blob is planted after disable.

**UX:** named change. “Disable PIN” can fail (dialog / CLI error) instead of
a success that left PIN unlock live. After a successful disable, a leftover
file cannot be used to unlock. Happy-path disable/unlock is unchanged.

**Automated-test impact: same.** Filesystem unit tests of `clear` (missing
file → Ok; path is a directory or parent not writable → Err) need no TPM.
`handle_unlock_with_pin` flag check is unit-testable with a config fixture
(`COSMIC_BWARDEN_*` overrides). Existing `tpm-smoke` / `tpm_lifecycle` happy
paths stay. Do **not** add live TPM fault-injection or `swtpm` unlink hooks —
that would be harder.

### S2-7 — Fill writes every password input, including hidden

**Where:** `browser-extension/content.js` `fillForm`.

**Observable outcome:** the password loop uses the same `isVisible` helper as
`findUsernameInput` (`content-heuristics.js`). Hidden / `display:none` /
zero-size honeypot `input[type=password]` are not filled. Visible
`current-password` and `new-password` on a change-password form **are** still
filled (that is the feature). Multi-step “username only” path is unchanged.

**UX:** none on ordinary login forms. Named change only on pages with hidden
password fields: those nodes stay empty (honeypot / CSRF traps no longer
receive the vault password).

**Automated-test impact: same.** `tests/browser-extension/playwright/autofill.spec.js`
already injects `content-heuristics.js` + `content.js` and fills a visible
field — that case still passes. Add a sibling case with a hidden password
input that must stay empty. No live-site fill harness.

### S2-8 — Save-prompt pending dropped before agent Ack

**Where:** `background-save.js` `onBarAction` — `clearPendingSave` runs after
`sendToAgent` even when the response is not `Ack`, and again in `catch`.

**Observable outcome:** `clearPendingSave` + `HIDE_SAVE_BAR` run only when the
agent returns `Ack`. On `Error` or throw: pending stays, bar gets
`SAVE_BAR_ERROR`, the user can retry. Dismiss / TTL / tab-close still clear.
Mode-mismatch (click “update” on a `save` pending) may still clear — that
path never sent a write.

**UX:** named change. A failed save/update leaves the bar in an error/retry
state instead of vanishing with the credential forgotten. Successful save is
unchanged. 90 s TTL still applies while retrying.

**Automated-test impact: same.** `background-save.test.js` `onBarAction`
already covers save/clear. Add: agent `Error` keeps pending; throw keeps
pending. Existing success-clear and “no pending” tests stay. No Playwright
timing of the bar.

### S2-9 — IPC response length uncapped

**Where:** `agent/lib.rs` write path; `AgentClient::do_send`
(`vec![0u8; len]`). Request cap is `MAX_REQUEST_BYTES = 8 MiB`.

**Observable outcome:** claimed response length `> 8 MiB` is refused on **both**
sides: the agent does not write it; the client does not allocate it. Share
the constant (move to `cosmic_bwarden_core` if both crates need it). Browser
host inbound stays 1 MiB.

**UX:** none for legitimate responses (vault payloads are far under 8 MiB).
A buggy/hostile agent can no longer force a ~4 GiB alloc in UI/CLI.

**Automated-test impact: same.** `ipc_hardening.rs` already speaks raw frames
to a bare agent (`test_oversized_request_is_rejected`). Add a client-side
test: a stub socket that prefixes `u32::MAX` must not allocate that size
(assert the error, not a 4 GiB `Vec`). Do not send a real 8 MiB body through
the E2E container suite.

### S2-10 — Autolock `Event::Locked` wipes less than `LockResult`

**Where:** `lifecycle.rs` `EventReceived(Locked)` vs `auth.rs` `LockResult`.

**Observable outcome:** one `wipe_session_secrets` helper used by
`LockResult`, `LogoutResult`, and `Event::Locked` (and, if they currently
skip it, `PinRequested` / `UnlockRequested`). It at least: clears
`selected_entry` / `editing_entry` / `revealed_fields` / entries lists;
`zeroize`s `login_password`, `unlock_password`, `unlock_pin`,
`main_window_pin`, `reprompt_password`; resets `notes_content`; clears
`generator_history_revealed` and any in-memory generator plaintext. Unlock
**mode** (PIN-first vs password-first) may stay, matching the current
`Event::Locked` comment.

**UX:** named change on autolock. The PIN/password box and notes/generator
reveal state are empty after idle lock (today a leftover PIN can still sit in
the form). Explicit lock already did most of this. Re-unlock is the same
number of clicks.

**Automated-test impact: same.** `app/tests/lifecycle.rs`
`test_lock_logout_clears_state` plus a sibling that feeds
`Message::EventReceived(Event::Locked)` and asserts the same buffers. No
sleep-until-autolock E2E.

### S2-E1 — Extension background + browser-host forward any `Action`

**Where:** `background.js` `onMessage` fallthrough `return sendToAgent(message)`;
`browser_host::run` deserializes any JSON `Action`.

**Observable outcome:** messages with `sender.tab` (content scripts) may
forward only an allowlist: `GeneratePassword` (context-menu / inline icon)
and any other action those scripts already send **on purpose**.
`GetEntry`, `GetPassword`, `GetTotp`, `Quit`, `Unlock`, `UnlockWithPin`,
`AddEntry`, `UpdateEntry`, `DeleteEntry`, `UpdateLoginPassword` from a tab
sender are dropped (no native write). Popup / extension-page senders
(`sender.url` is the extension origin, no tab) keep the full popup surface.
`LOGIN_SUBMITTED` / `SAVE_BAR_ACTION` / `SetTheme` stay local (already).
The native host can remain a generic `Action` JSON pipe — the allowlist in
the background is the control. Optional belt: host allowlist — only if it
does not break the popup; that is a second PR.

**UX:** none if the allowlist matches today’s content-script traffic
(`GeneratePassword` only). A future content-script bug can no longer
`GetPassword` / `Quit`.

**Automated-test impact: same.** `background.test.js` already loads
`background.js` with a fake native port. Add cases: content-script-shaped
`sender.tab` + `GetPassword` does not call the port; popup-shaped sender
still does; `GeneratePassword` from a tab still does. Do not add an
isolated-world exploit E2E.

---

## S2 — previously documented, still open

### P1-3 — Reprompt hash compare is not constant-time

**Where:** `query::verify_reprompt` (`!=` on hash bytes). Same class:
`CheckLoginMatch` (`p.expose() == password`) on a value the client already
holds.

**Observable outcome:** both compares go through `subtle::ConstantTimeEq` (or
equivalent). Wrong password still returns `incorrect password` /
`password_matches: false`. Add `subtle` only if it is not already in the
graph via `hmac`.

**UX:** none.

**Automated-test impact: same.** `security.rs` `test_reprompt` stays the
functional gate. **Do not** add a timing/cache-line E2E or a “constant-time
must pass on CI” bar — that is the kind of harness this plan calls harder,
and it is not justified on an A2-bounded compare.

### P1-8 — Fill does not re-check the active tab’s host

**Where:** `popup.js` `fillEntry` — `GetEntry` then `tabs.sendMessage` to the
current active tab, no `hosts_match` against `tab.url`. Fill is offered on
every Login row (search/favourites included).

**Observable outcome:** before `FILL_FORM`, the popup requires the tab’s host
to match at least one entry URI (exact or label-boundary subdomain; a
PSL-less JS check is fail-closed versus agent `hosts_match` and needs no
protocol bump). On mismatch: status message, no `sendMessage`, no
`window.close()`. Defense in depth: `FILL_FORM` carries `expectedHost`;
`content.js` no-ops when `location.hostname` does not match (missing
`expectedHost` → no-op, fail-closed).

**UX:** named change. Filling a search/favourites hit into a tab for a
different site is **refused** (copy password still works). Filling the
current site’s match is unchanged. A tab that navigated between click and
inject is not filled.

**Automated-test impact: same**, with one-time fixture updates.
`popup.spec.js` fill cases must give the mock tab a `url` whose host matches
the fixture login; `autofill.spec.js` must pass `expectedHost` in the
injected `FILL_FORM`. Add a mismatch case that must **not** send
`FILL_FORM`. Do not add a live two-tab navigation race harness.

### P1-9 — Extension clipboard has no auto-clear

**Where:** `popup-detail.js` `makeCopyBtn`; `popup-list-actions.js` Copy
Password; `content-generate.js` `GENERATE_COPY_TO_CLIPBOARD`. UI/applet
already use `CLIPBOARD_CLEAR_SECS = 30` with generation-counter + readback
wipe (`copy_to_clipboard_with_autoclear`).

**Observable outcome:** every extension write of a **secret** (password, TOTP,
generated password) schedules a 30 s clear that overwrites the clipboard
**only if** it still holds our value (same readback rule as the UI). Username
/ public key copies are not secrets; they may stay. Chrome MV3 keeps
relaying the write through the content script; use `browser.alarms` (or the
existing alarm helper) so a worker restart does not lose the timer. One
shared helper, not three copies.

**UX:** named change, matching the desktop: a copied password disappears
after 30 seconds if the user has not copied something else.

**Automated-test impact: same** if the timer is unit-tested with **fake
timers** (vitest/`sinon` clock) and existing Playwright copy specs keep
asserting the **write** only. **Harder** if someone adds a real 30 s
Playwright wait or a live-clipboard permission E2E — do not. Headless
clipboard is already stubbed (`popup.spec.js`, `generate-password.spec.js`);
leave those stubs.

---

## S3 — new findings

### S3-1 — `config.json` / config dir not forced `0600` / `0700`

**Where:** `CosmicBWardenConfig::save_legacy` (`create_dir_all` +
`File::create`, umask-dependent).

**Observable outcome:** parent dir `0700` (same `DirBuilder::mode` pattern as
`agent/lib.rs` / `dirs::make_all`); file `0600` after write (temp + rename
is nicer and matches `Db::save`, but chmod-after-create is enough). Config
still holds email/URLs/TPM flags, not tokens.

**UX:** none.

**Automated-test impact: same.** Mode assertion next to existing dir-mode
checks (`ipc_hardening.rs` `test_socket_file_modes`, or a `config.rs` unit
test under a `COSMIC_BWARDEN_CONFIG` override). No extra process.

### S3-2 — UI `Message` derived `Debug` can carry secrets

**Where:** `crates/cosmic-bwarden-ui/src/message.rs`.

**Observable outcome:** handwritten `Debug` for `Message` (variant +
non-secret scalars), same contract as `protocol/debug_impls.rs`. Clipboard /
password-change / generator / PIN payloads never appear in `{:?}`. Nothing
today `tracing`s a `Message`; this is iced-debug / future-log hardening.

**UX:** none.

**Automated-test impact: same.** One unit test in the UI crate,
`message_debug_never_prints_secrets`, cloned from
`action_debug_never_prints_secrets`. Existing MVU tests stay.

### S3-3 — Edit `savePopupState` copies a stored password into `storage.session`

**Where:** `popup-state.js` `snapshotPopupState` — every `f-${key}` value,
including a password prefilled from `GetEntry` in `showEdit`.

**Observable outcome:** the persisted draft stores user-**typed** dirty
fields, but not an unchanged stored password (omit the key, or store a
sentinel that restore treats as “re-fetch via `GetEntry`”). Lock still
clears state. Session storage remains extension-only.

**UX:** named change. Close and reopen mid-edit: an untouched password box
is empty until restore re-fetches (today it can reappear from
`storage.session`). Typed-but-unsaved edits of other fields still restore.

**Automated-test impact: same.** Unit-test `snapshotPopupState` with a
prefilled password field: serialized draft must not contain the vault
password. Existing popup-state restore tests need a restore path that
re-`GetEntry`s. No new browser-session harness.

### S3-4 — `prctl(PR_SET_DUMPABLE, 0)` return ignored

**Where:** `cosmic_bwarden_agent::run`.

**Observable outcome:** non-zero `prctl` logs `error!` (AGENTS.md: anything
that can affect the dumpable bit is not silent). Startup still continues
(failing closed on `prctl` would brick the agent on exotic kernels; logging
is the invariant). No skip flag.

**UX:** none (journal line only).

**Automated-test impact: same.** Do not try to inject `prctl` failure in
E2E. A `#[cfg(test)]` wrapper is optional; grep-level review is enough.
Existing `ipc_hardening` spawn still proves the agent starts.

### S3-5 — Autolock poll is 5 minutes

**Where:** `timeout.rs` `CHECK_INTERVAL = 300s`. A 5-minute timeout can fire
at 5–10 minutes.

**Observable outcome:** the timer sleeps the **remaining** time until
`last_activity + lock_timeout` (capped, e.g. at 30 s, so a duration change
is noticed) instead of a fixed 5-minute poll. A 5-minute setting fires
within ~30 s of the deadline. `TimerHandle::reset` / `set_duration` stay
atomic, no per-keystroke timer churn.

**UX:** named change. Vault locks closer to the minutes chosen in Settings
(today it can overshoot by almost one poll). Slightly more wakeups on a
30 s cap — still cheap.

**Automated-test impact: same.** Unit-test remaining-time arithmetic with a
fake clock if the sleep is injected; do not add a 5-minute wall-clock E2E
(that would be harder). Existing settings persist tests are unrelated.

### S3-6 — CLI `get` fetches `GetEntry` even when stdout is masked

**Where:** `commands/vault.rs` `Commands::Get` without `--all`.

**Observable outcome:** without `--show-secrets`, the CLI sends
`GetEntryMeta` (or uses the already-redacted `GetEntries` row). With
`--show-secrets`, it still sends `GetEntry` (reprompt may then apply).
Stdout masking in `output.rs` remains as a second line of defense, not the
only one. Builder stays a named function if this grows a match arm — the
CLI analogue of `vault_actions::fetch_entry`.

**UX:** none for stdout (secrets already masked). The CLI process no longer
holds those `Secret`s on a normal `get`. Combined with S2-1, IBAN/SSN-class
fields follow the same rule.

**Automated-test impact: same.** `cli_secret_mask_test.rs` still checks
stdout; add an assertion on the **action** if the CLI test harness can see
it, or a unit test of the builder. Do not require `--show-secrets` E2E to
prove IPC.

### S3-7 — `openEntrySite` accepts any `scheme://` URI

**Where:** `popup-list-actions.js` `openEntrySite` — if the URI matches
`^[a-zA-Z][a-zA-Z0-9+.-]*://`, it is passed to `tabs.create` unchanged
(`javascript:`, `data:`, `file:`).

**Observable outcome:** only `http:` and `https:` (optional: `http://` added
when the URI has no scheme, as today). Anything else: status message, no
tab. User click still required.

**UX:** named change. A vault URI of `javascript:…` or `data:…` no longer
navigates; the user sees “unsupported URL scheme” (or equivalent) instead of
a new tab.

**Automated-test impact: same.** Unit or Playwright case: `javascript:alert(1)`
does not call `tabs.create`; `https://example.com` still does. Existing open-
site tests keep using `https` fixtures.

---

## Per-ID summary (fix / UX / test hardness)

| ID | Observable fix | UX | Existing-suite hardness |
|---|---|---|---|
| S2-1 | Meta/bulk `None` for Identity PII + IBAN-class; merge treats `None` as unchanged | CLI list hides those fields; popup `currentEntry` no longer carries them | **same** — `redact`/`merge` units + `security.rs` / CLI mask |
| S2-2 | store/get/delete share one collection; no silent fallback collection | Rare: persist-session fails instead of storing unreadably | **same** — pure helper unit; no live keyring E2E |
| S2-3 | `decrypt_common_symmetric` rejects `mac: None` | none | **same** — `cipherstring.rs` unit |
| S2-4 | KDF iterations rejected outside Bitwarden range | Error (not hang) if a server sets absurd iterations | **same** — `identity.rs` units; never run a huge KDF |
| S2-5 | `Secret: ZeroizeOnDrop` | none | **same** — trait/zeroize unit; no freed-heap E2E |
| S2-6 | `clear` fail-closed; disable no `Ack` if blob remains; unlock honours flags | Disable PIN can error; leftover blob cannot unlock | **same** — filesystem + config units; no TPM fault harness |
| S2-7 | `fillForm` fills only `isVisible` password inputs | Hidden honeypot fields stay empty | **same** — extra `autofill.spec.js` fixture |
| S2-8 | Pending cleared only on agent `Ack` | Failed save bar stays for retry | **same** — `background-save.test.js` |
| S2-9 | Response length capped at 8 MiB (agent + client) | none | **same** — `ipc_hardening.rs` + stub socket; no 4 GiB alloc |
| S2-10 | Shared wipe helper for lock button and `Event::Locked` | Autolock empties PIN/notes/generator reveal state | **same** — `lifecycle.rs` MVU |
| S2-E1 | Content-script `sender.tab` cannot proxy arbitrary `Action`s | none if allowlist matches today | **same** — `background.test.js` sender shapes |
| P1-3 | `ConstantTimeEq` on reprompt (and `CheckLoginMatch`) | none | **same** — `test_reprompt`; **no** timing E2E |
| P1-8 | Fill refused unless tab host matches entry URI; content script checks `expectedHost` | Off-domain Fill from search/favourites errors | **same** — popup/autofill fixture `url` / `expectedHost` |
| P1-9 | 30 s secret clipboard clear with readback, matching UI | Copied password vanishes after 30 s | **same** with fake timers; **do not** 30 s Playwright |
| S3-1 | config dir `0700`, file `0600` | none | **same** — mode unit |
| S3-2 | Manual `Debug` on `Message` | none | **same** — UI debug unit |
| S3-3 | Popup edit draft does not persist a stored password | Reopen mid-edit re-fetches password | **same** — `popup-state` unit |
| S3-4 | `prctl` failure logs `error!` | none | **same** — no `prctl` injection |
| S3-5 | Autolock sleeps remaining time (cap ~30 s) | Lock fires nearer the setting | **same** — arithmetic unit; no 5 min wall clock |
| S3-6 | CLI `get` without `--show-secrets` uses `GetEntryMeta` | none on stdout | **same** — CLI mask + builder unit |
| S3-7 | `openEntrySite` only `http`/`https` | `javascript:` / `data:` click is refused | **same** — popup unit/Playwright |
| R-1 | Desktop selection sends `GetEntryMeta`; secrets on reveal | Detail hides secrets until click; reprompt on reveal | **same** — `vault_actions.rs` |

---

## Overall testing-suite impact

Wave 1–2 should **not** make `just test`, `just test-extension-unit`, or
`just test-extension-e2e` slower or flakier. They add unit/vitest/Playwright
cases of kinds that already exist (`security.rs`, `ipc_hardening.rs`,
`cipherstring.rs`, `identity.rs`, `lifecycle.rs`, `autofill.spec.js`,
`background-save.test.js`, `cli_secret_mask_test.rs`).

The three items that **become harder if implemented with the wrong test**
are P1-3 (timing), P1-9 (real 30 s / live clipboard), and S2-6 / S2-2
(live TPM unlink / live Secret Service). This plan forbids those harnesses:
functional tests only.

R-1 and P1-8 need fixture updates in tests that assumed “selection ⇒
`GetEntry`” or “Fill ⇒ current tab, any host”. That is one-time alignment
with the new contract, not a new class of flake.

No `PROTOCOL_VERSION` bump is required for the recommended shapes. New
user-facing strings (disable-PIN error already exists; Fill mismatch;
unsupported URL scheme) go through `fl!` on the UI crate and through the
extension’s existing `showStatus` English strings on the popup.

---

## What this plan is not

It is not a patch, not a re-score of the review, and not a change to
`SECURITY.md`, `docs/roadmap.md`, or `docs/review/01_security.md`. Implement
in the wave order above; keep A2 and the other accepted residuals off the
board.
)
