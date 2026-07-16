# Password Generator — Implementation Plan

## Context

A password generator spanning every surface of the app: a new pane on the main Vault window (checkboxes for character groups, length slider, generate/reset buttons, reveal/copy on the result), a 7-day local history of every generated password (listed on the same pane, same reveal/copy), a quick-generate entry in the COSMIC applet's popup menu, and both a right-click context-menu entry and an inline per-field icon in the browser extension. Nothing like this exists in the codebase today (confirmed by search — it was an open item in `docs/roadmap.md`). Because every surface (desktop UI, applet, CLI, browser extension) must share "last-used settings" and a single history, the generation logic, settings, and history live in the **agent** (the one process all surfaces already talk to), not in any one client.

Character groups are 4 checkboxes, Bitwarden-style — **Uppercase, Lowercase, Numbers, Special** (confirmed with the user, not a single combined "Letters" checkbox).

This plan was produced after exploring the protocol/agent/core layer, the UI (state/view/update/applet-menu), and the browser-extension/CLI layer in depth, then validating crypto/storage claims directly against the source (`cipherstring.rs`, `locked.rs`, `dirs.rs`).

## Protocol design (`cosmic-bwarden-core`)

**Pre-requisite**: `protocol.rs` is already 552 lines, over AGENTS.md's 500-line hard cap. Split it *first*, as its own commit, before adding anything:
- `protocol.rs` — `Action`/`Response`/`Event` enums + inline DTOs + `variant_name()` (~300 lines after the move).
- `protocol/debug_impls.rs` — the two manual `Debug` impls (~130 lines).
- `protocol/tests.rs` — `debug_redaction_tests` + the arbitrary-bytes fuzz test (~150 lines).

New DTOs (inline in `protocol.rs`, same style as existing `TpmDaStatus`):
```rust
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratorSettings {
    pub uppercase: bool,
    pub lowercase: bool,
    pub numbers: bool,
    pub special: bool,
    pub length: u8, // 8..=32, enforced agent-side
}
impl Default for GeneratorSettings {
    fn default() -> Self { Self { uppercase: true, lowercase: true, numbers: true, special: true, length: 14 } }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct GeneratorHistoryEntry {
    pub password: String,
    pub created_at: u64, // unix epoch seconds
}
// Manual Debug: never print `password`.
```

New `Action` variants: `GeneratePassword { settings: Option<GeneratorSettings> }`, `GetGeneratorSettings`, `GetPasswordHistory`.
New `Response` variants: `GeneratedPassword { password: String }`, `GeneratorSettings { settings: GeneratorSettings }`, `PasswordHistory { entries: Vec<GeneratorHistoryEntry> }` (pruned to 7 days, newest-first).

Semantics of `GeneratePassword { settings }`: `Some(s)` persists `s` as the new last-used settings **and** generates with it (this is what the UI pane's Generate button and CLI-with-flags send); `None` reuses whatever is currently persisted, falling back to `GeneratorSettings::default()` on first run (this is what the applet menu, browser extension, and CLI-with-no-flags send). **Every** call appends to history regardless of `Some`/`None`.

Reject, don't clamp, at the trust boundary (CLI/browser JSON can send anything): all four booleans false → `Response::Error{"select at least one character group"}`; `length` outside `8..=32` → `Response::Error{"length must be between 8 and 32"}`.

Required alongside the new variants (easy to miss, both are enforced by existing tests): add arms to `variant_name()`; add `GeneratePassword`'s settings to the manual `Debug for Action`; add **no** arms for the 3 new `Response` variants (they must fall through to the redacting fallback so `password`/`entries` never print); add new cases to `debug_redaction_tests` asserting `Response::GeneratedPassword`/`PasswordHistory` never print their payload in `{:?}`.

## Generation algorithm (`cosmic-bwarden-agent`, new `handler/generator/algorithm.rs`)

- Charsets: `UPPER`/`LOWER`/`DIGITS` standard; `SPECIAL = "!@#$%^&*"` (Bitwarden's basic set — deliberately excludes quotes/backslash/whitespace since generated passwords land in CLI args, JSON, and shells).
- Force one character from each *selected* pool, fill the remainder from the union, then cryptographically shuffle.
- **Must use `rand::rngs::OsRng`**, never `rand::rng()`/`thread_rng()`/the seeded RNG already used in protocol fuzz tests — this is a hard security requirement, called out in review.
- Reject empty pool-selection and out-of-range length with `Result<String, String>`.
- Unit tests: per-class guarantee, length correctness, output-charset containment, empty-selection rejection, out-of-range rejection, a coarse distribution smoke test.

## Where the handler lives

New top-level `crates/cosmic-bwarden-agent/src/handler/generator/` (`mod.rs`, `algorithm.rs`, `storage.rs`), registered as its **own dispatch group** in `handler.rs`, parallel to `auth`/`vault`/`subscription_handler` — not folded into `handler/vault/`. Every existing `vault::*` handler assumes an unlocked vault (`state.db`); password generation must work locked, and even pre-login (fresh install, no account yet). Keeping it a sibling module makes "works without unlock" structurally obvious from the dispatch table, and avoids growing `vault/mod.rs` (already 178 lines).

## At-rest protection for the 7-day history

**Reuse existing crypto — no new dependency.** `cipherstring.rs` already implements `CipherString::encrypt_symmetric(keys: &locked::Keys, plaintext) / decrypt_symmetric(...)` (AES-256-CBC + HMAC-SHA256), and `locked::Keys::new(Vec<u8>)` / `.enc_key()` / `.mac_key()` just wrap 64 raw bytes regardless of where they came from.

- **Key**: on first use, generate 64 random bytes via `OsRng`, wrap in `locked::Keys`, persist to a new `dirs::generator_key_file()` → `data_dir().join("generator_key.bin")`, written 0600 (same `OpenOptions::mode(0o600)` pattern as `tpm/blob.rs`). **Device-global, not per-account** — no server/email in the path, since generation must work with zero accounts configured.
- **History entries**: each password is `CipherString::encrypt_symmetric`'d before touching disk, stored as its `Display` string; decrypted on read.
- **History file**: new `dirs::generator_history_file()`, postcard-encoded `Vec<{created_at, ciphertext}>`, atomic tmp+rename+0600 write (same pattern as `db/persistence.rs`).
- **Threat model, stated plainly in docs**: this protects against a different user, a stray backup, or misconfigured permissions elsewhere reading the file directly — it does **not** protect against another process running as the same local user (root, or a compromised session), since the key sits unguarded next to the ciphertext by design (generation must work without a master password). This is the same protection level `Db`'s 0600 JSON already has; document it next to that precedent, don't oversell it.
- Prune-on-read **and** prune-on-write (every `GeneratePassword` and `GetPasswordHistory` call filters to `now - created_at <= 7*86400` before returning/saving) — no separate background sweep needed; the file self-heals on every one of its own access paths.

## Non-secret settings persistence

New sibling type `GeneratorSettings::load()/save()` in a new `crates/cosmic-bwarden-core/src/generator_settings.rs`, plain JSON at `dirs::generator_settings_file()` (`data_dir().join("generator_settings.json")`) — **not** folded into `CosmicBWardenConfig`, which is account-shaped (email/server/TPM/lock-timeout) and would be the wrong owner for a value that must be readable/writable with zero accounts configured (bare CLI `generate` before any login). No `cosmic_config` ceremony needed for "4 bools + a u8" — mirrors `CosmicBWardenConfig`'s own `load_legacy`/`save_legacy` plain-JSON path.

## CLI (`cosmic-bwarden-cli`)

New `Commands::Generate { uppercase: bool, lowercase: bool, numbers: bool, special: bool, length: Option<u8>, history: bool }` (short flags `-U -l -n -s`, `length` clap-range-validated `8..=32`, `history` `conflicts_with_all` the others). If none of `-U/-l/-n/-s/--length` given → `Action::GeneratePassword{settings: None}` (reuse last-saved). If any given → treat as "fully specifying this run's checkboxes" (clap bools can't express explicit-false), fetch current length via `GetGeneratorSettings` first if `--length` omitted, then send `Some(GeneratorSettings{...})`. `--history` sends `GetPasswordHistory` and prints `created_at | password` lines. Print **only** the bare password to stdout on success (scriptable: `cosmic-bwarden-cli generate | xclip`). New `crates/cosmic-bwarden-cli/src/commands/generator.rs`; verify `preprocess_args` doesn't collide with `generate`/`history` keywords; add an `after_help` example.

## Desktop UI pane (`cosmic-bwarden-ui`)

- `message.rs`: `View::PasswordGenerator`; ~12 new `Message` variants (`GeneratorViewClicked`, one Toggled per checkbox, `GeneratorLengthChanged`, `GeneratorResetClicked` — **local-only, does not call the agent**, `GeneratorGenerateClicked`, `GeneratorGenerated(Result<String,String>)`, `GeneratorRevealToggled`, `GeneratorSettingsReceived`, `GeneratorHistoryReceived`, `GeneratorHistoryRevealToggled(usize)`).
- `app/state.rs`: `generator_settings: GeneratorSettings` (current draft pane state), `generator_result: Option<String>`, `generator_result_revealed: bool`, `generator_history: Vec<GeneratorHistoryEntry>`, `generator_history_revealed: HashSet<usize>` (index-keyed — the pane always refetches+replaces the whole Vec together, so index keys don't go stale mid-session), `generator_error: Option<String>`.
- `view/vault/mod.rs`: extend the right-panel dispatch with `View::PasswordGenerator => self.view_generator()`; add `mod generator;`.
- New `view/vault/generator.rs` (~200-240 lines): checkboxes (`cosmic::widget::checkbox` — **first use in this crate**, verify its `.on_toggle` signature against the vendored libcosmic version), slider (reuse the exact `view/settings.rs` lock-timeout template, range `8u32..=32u32`), result row using `secure_input(_, pw, Some(Message::GeneratorRevealToggled), !revealed)` + `button::icon(icon::from_name("edit-copy-symbolic")).on_press(Message::CopyToClipboard(pw.clone()))` (both patterns copied verbatim from `view/vault/detail.rs`), Generate/Reset buttons, and a history list reusing the same reveal/copy row shape per entry.
- `view/vault/sidebar.rs`: add a nav button next to Settings/Lock/Logout (`bottom_row` currently pushes 3 `Length::Fill` buttons — a 4th likely needs wrapping into two rows of two; decide by eye at implementation time).
- New `app/update/pwgen.rs` (~150-200 lines), registered in the `update_app` chain (`app/update/mod.rs`) alongside `update_lifecycle`/`update_auth`/`update_vault`/`update_applet`. `GeneratorResetClicked` just resets the pane's draft `generator_settings` to `GeneratorSettings::default()` locally (no IPC — matches "no live-updating" spec). `GeneratorGenerateClicked` sends `Action::GeneratePassword{settings: Some(current)}`, and on success also re-fetches history so the new entry shows up.
- `app/tasks.rs`: `fetch_generator_settings()` / `fetch_generator_history()`, following the existing `fetch_applet_secret`-style `Task::perform` template.
- i18n: add ~10 kebab-case keys to `crates/cosmic-bwarden-ui/i18n/en/cosmic_bwarden_ui.ftl` (`password-generator`, `uppercase`, `lowercase`, `numbers`, `special-characters`, `password-length = Length: { $length }`, `generate`, `reset`, `no-password-generated-yet`, `recent-passwords`) — compile-time `fl!` validation catches typos for free.

## Applet menu entry

Add an unconditional (not lock-gated) icon button in `view/applet/menu.rs`'s `header_row()`, tooltip `fl!("generate-password")`, `.on_press(Message::AppletGeneratePasswordRequested)`. Handled in `app/update/applet.rs` alongside the existing `AppletCopySecret`/`AppletSecretReceived` pair: sends `Action::GeneratePassword{settings: None}` (last-saved), and on success reuses the existing `applet_copy_to_clipboard(pw)` helper verbatim (toast + 30s auto-clear, already wired). Critically, this handler **never assigns `self.view`**, so it works identically whether the popup shows the unlock form, search results, or the "not configured" message — satisfying "generate regardless of lock state."

## Browser extension

**Context menu** (`manifest.json` + `background.js`): add `"contextMenus"` permission; `browser.contextMenus.create({..., contexts: ['editable']})`; `onClicked` sends `{GeneratePassword: {settings: null}}` through the existing `sendToAgent` FIFO (zero changes needed in `browser_host.rs` — it generically (de)serializes whatever `Action`/`Response` contain). Because a Chrome MV3 background script is a service worker with no clipboard/DOM access (Firefox's persistent background page would have it, but using one code path for both is simpler), the actual clipboard write is relayed to the target tab's content script via `browser.tabs.sendMessage(tab.id, {type: 'GENERATE_COPY_TO_CLIPBOARD', password})`, which calls `navigator.clipboard.writeText` where DOM access exists.

**Inline per-field icon** (new `content-generate.js`, loaded after `content-heuristics.js`): shows a small icon anchored to a password `<input>` only when (a) the form has ≥2 password fields (registration/change-password), or (b) it has exactly 1 password field **and** a detected username/email field (registration with one password box) — a lone password field with no username field (e.g. a plain login form or PIN gate) gets no icon, which directly encodes "never offer to overwrite a login password." Positioning uses a per-input shadow-DOM host (`content-bar.js`'s shadow-DOM-injection mechanics, but `position: fixed` anchored to `getBoundingClientRect()` instead of a static top bar), repositioned via `ResizeObserver` + scroll/resize listeners + a debounced `MutationObserver` for SPA re-renders — this is the one genuinely novel piece of client logic in the whole feature, with no in-repo precedent to copy. On click: **fill the field(s) directly** (matching Bitwarden's actual UX, not clipboard-copy) via `browser.runtime.sendMessage({GeneratePassword: {settings: null}})`, filling both the password and its detected confirm-password sibling using the native-setter+dispatchEvent trick needed for framework-controlled inputs.

Every browser-extension-originated password (context menu or inline icon) round-trips through the agent's `GeneratePassword` handler like every other caller, so it lands in the shared 7-day history automatically — no extra plumbing required.

## File-by-file summary

| Area | Files |
|---|---|
| Protocol split (prereq) | `protocol.rs` → `protocol.rs` + `protocol/debug_impls.rs` + `protocol/tests.rs` |
| Core | `protocol.rs` (new DTOs/variants), `generator_settings.rs` (new), `dirs.rs` (+3 fns), `lib.rs` (mod registration) |
| Agent | `handler.rs` (+dispatch arm), `handler/generator/{mod,algorithm,storage}.rs` (new) |
| CLI | `args.rs` (+`Commands::Generate`), `commands/mod.rs` (+routing), `commands/generator.rs` (new), `utils.rs` (verify `preprocess_args`) |
| UI | `message.rs`, `app/state.rs`, `view/vault/mod.rs`, `view/vault/generator.rs` (new), `view/vault/sidebar.rs`, `app/update/mod.rs`, `app/update/pwgen.rs` (new), `app/update/applet.rs`, `app/tasks.rs`, `i18n/en/cosmic_bwarden_ui.ftl` |
| Browser extension | `manifest.json`, `background.js`, `content-generate.js` (new) |
| Docs | `docs/browser_integration.md` (new "Generate Password" section), `docs/roadmap.md` (check off the backlog item), `CONTEXT.md`, `AGENTS.md` (new subsection: key/history file paths + threat model) |
| Tests | `handler/generator/algorithm.rs` (inline unit tests), `crates/cosmic-bwarden-tests/src/generator.rs` (new E2E), `tests/browser-extension/fixtures/change_password.html` (new), `tests/browser-extension/playwright/generate-password.spec.js` (new) |

## Phasing

1. **Protocol + agent + storage + algorithm** — the security-sensitive foundation (RNG choice, cipherstring reuse, pruning). Validate: `cargo check -p cosmic-bwarden-core`, `cargo check -p cosmic-bwarden-agent`, new unit tests, `cargo test -p cosmic-bwarden-tests -- --test-threads=1` (new `generator.rs` E2E: generate combos, reject-empty/reject-bad-length, `None`-reuse, history round-trip + pruning).
2. **CLI** — cheap second client that stress-tests the protocol shape before the big UI investment. Validate: `cargo check -p cosmic-bwarden-cli`, CLI-driven E2E case.
3. **Desktop UI pane** — the largest chunk. Validate: `cargo check -p cosmic-bwarden-ui` (catches `fl!` typos), manual smoke test in a running instance.
4. **Applet menu entry** — small, isolated; explicitly test "generate while locked." Validate: `cargo check -p cosmic-bwarden-ui`, extended `applet_flow.rs`-style E2E case.
5. **Browser extension context menu** — simpler of the two extension pieces (no positioning logic). Validate: `just test-extension-unit`, new Playwright spec invoking the `contextMenus.onClicked` handler directly (real right-clicks aren't Playwright-drivable).
6. **Browser extension inline field icon** — sequenced last: the one piece with no precedent (anchoring/positioning/mutation-observing) and the highest real-world-site-breakage risk. Validate: `just test-extension-unit`, `just test-extension-e2e` (Firefox) **and** `just test-extension-e2e-chrome` — this phase is the only place the plan relies on Chrome-specific behavior (service-worker clipboard relay), so skipping the Chrome E2E run here specifically would risk silently shipping a broken feature on half the supported browsers.

Every bug found during any phase gets a regression test in the nearest suite, per AGENTS.md's existing "Fixing failing tests" workflow.

## Verification

- Per phase: the specific `cargo check`/`cargo test` commands listed above, plus `just test-extension-unit` / `just test-extension-e2e` / `just test-extension-e2e-chrome` for phases 5-6.
- End-to-end manual pass after phase 3: run the app, open the new pane, toggle checkboxes, drag the slider, Generate, reveal/copy the result, Reset, confirm history list populates and reveal/copy works per row.
- After phase 4: lock the vault, use the applet's quick-generate button, confirm clipboard gets the password without opening the vault window.
- After phase 6: load both Playwright fixtures (login page vs. change-password page) in Firefox and Chrome, confirm the icon appears only where the heuristic says it should, and that clicking it fills the right field(s).
