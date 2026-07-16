# Browser Extension Integration

Cosmic BWarden includes a native browser extension (currently targeting Firefox) that provides secure, high-performance vault access directly within the browser.

## Architecture: The Native Messaging Bridge

The extension follows a **"Thin Client" Invariant**: it contains no cryptographic logic and stores no secrets. Instead, it acts as a UI proxy for the `cosmic-bwarden-agent`.

Communication is handled via the [WebExtensions Native Messaging API](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging).

### 1. The `browser-host` Subcommand

The `cosmic-bwarden-agent` includes a specialized mode triggered by the `browser-host` subcommand. This mode is responsible for:
- Reading JSON messages from `stdin` (sent by the browser).
- Translating JSON to the internal `Action` protocol (using Postcard).
- Forwarding the action to the main agent process via the local Unix socket.
- Translating the agent's `Response` back to JSON.
- Writing the JSON response to `stdout` (read by the browser).

### 2. Configuration & Compilation

The browser integration is guarded by a Cargo feature flag in `cosmic-bwarden-agent`:

- **Feature Name**: `browser-host`
- **Default**: Enabled
- **Crate**: `crates/cosmic-bwarden-agent`

To compile the agent without browser support:
```bash
cargo build -p cosmic-bwarden-agent --no-default-features
```

The `browser-host` feature requires the `io-std` feature of `tokio` for asynchronous access to standard input and output streams.

## Security Model

- **No Local Storage**: The extension does not use `browser.storage` for vault data. All searches and fetches are performed on-demand via the agent.
- **Process Isolation**: The `browser-host` subcommand runs as a separate process from the main agent daemon, providing an additional layer of isolation.
- **Native Endianness**: The Native Messaging protocol requires a 4-byte length prefix in the CPU's native endianness, which the bridge correctly handles to avoid cross-platform issues.

## Registration & Installation

Before the extension can communicate with the agent, the host must be registered with the browser.

### Automatic Registration
Use the provided `just` task:
```bash
just register-browser-host
```
This runs `tools/register_browser_host.py`, which:
1. Detects the agent's location in `target/debug`.
2. Creates a wrapper script to handle the `browser-host` argument.
3. Generates the host manifest JSON in `~/.mozilla/native-messaging-hosts/com.8bit.cosmic_bwarden.json`.

### Manual Installation
1. Open Firefox and navigate to `about:debugging#/runtime/this-firefox`.
2. Click **Load Temporary Add-on...**.
3. Select `browser-extension/manifest.json`.

## Protocol Translation

| Source | Format | Protocol |
| :--- | :--- | :--- |
| Extension Popup | JSON | `{"GetConfig": null}` |
| `browser-host` (Bridge) | Postcard | `Action::GetConfig` |
| Main Agent | Postcard | `Response::Config { ... }` |
| `browser-host` (Bridge) | JSON | `{"Config": { ... }}` |

## Save/Update Prompt (notification bar)

When the user submits a login form, the extension offers to save the credentials
as a new Login entry, or to update an existing entry when the domain+username
matches but the password changed (Bitwarden-style notification bar).

### Message flow

```
content-submit.js ──LOGIN_SUBMITTED{username,password,url}──▶ background (per-tab pending map)
   tabs.onUpdated 'complete' (or a ~2 s fallback timer for AJAX/SPA logins)
      ▼
GetConfig → locked or needs_login? → silently drop the pending credential
CheckLoginMatch{domain,username,password} → save | update | silent
      ▼
tabs.sendMessage SHOW_SAVE_BAR{mode,domain,username,entryName}   ← never contains the password
      ▼
content-bar.js renders the bar → SAVE_BAR_ACTION{save|update|dismiss}
      ▼
background: AddEntry{..., uris:[origin]}  or  UpdateLoginPassword{id,password}
then badge refresh + HIDE_SAVE_BAR on success, SAVE_BAR_ERROR on failure
```

### Agent actions

- **`AddEntry`** now accepts optional `totp` and `uris` fields (both
  `#[serde(default)]`, so older JSON payloads without the keys keep parsing).
  The save bar sends `name = domain` and `uris = [{ "uri": <origin>, "match_type": null }]`
  so the entry is domain-matchable afterwards.
- **`CheckLoginMatch { domain, username, password }`** → `LoginMatch { entry_id, name, password_matches }`.
  The agent compares the *submitted* password against its decrypted stored copy;
  the stored password never leaves the agent — the response is a single equality
  bit about a value the client already possesses. Domain matching uses the
  shared rules in `cosmic_bwarden_core::domain` (exact / boundary-subdomain in
  both directions / PSL eTLD+1 — see `docs/public_suffix_list.md`); URIs with
  match type `Never` are skipped, all other match types are treated as domain
  matching in v1. Legacy entries without URIs match via a hostname-shaped
  entry name (free-text names never domain-match); username compares
  case-insensitively.
- **`UpdateLoginPassword { id, password }`** sets a new password on an existing
  Login. The agent decrypts the stored entry itself and reuses the normal update
  path, so no other secret (TOTP, notes) ever transits to the extension and the
  redaction/merge pitfalls of echoing a `GetEntryMeta` result through
  `UpdateEntry` (which would wipe notes) are avoided.

### Security notes

- Plaintext submitted credentials exist transiently in: content-script locals
  (released after `sendMessage`), and `browser.storage.session` keyed by tab
  ID — in-memory only (never touches disk, cleared when the browser session
  ends), never logged, cleared on bar action, TTL alarm, or tab close.
  `storage.session` (not a plain module-level `Map`) and `browser.alarms`
  (not `setTimeout`) are used specifically because a Chrome MV3 service
  worker is killed after ~30 s of inactivity — routinely less than the time
  a user takes to read the bar and click Save/Update. A plain in-memory
  `Map`/`setTimeout` pair lost the pending credential across that restart:
  the bar displayed correctly, but confirming silently did nothing because
  the click landed on a fresh, empty `Map`. See `background-save.js`'s
  `pendingKey`/`getPendingSave`/`setPendingSave` and the
  `pendingSaveExpire:<tabId>` alarm.
- Messages from background to the page (`SHOW_SAVE_BAR`) carry display labels
  only, never the password.
- This preserves the existing invariant: credentials are captured *actively at
  user-initiated submit*, not from passive browsing, and the extension never
  fetches a stored secret to compare — comparison happens inside the agent.
- The bar is rendered in an open shadow root with all text set via
  `textContent` (page-derived strings are never injected as HTML).

## Generate Password (context menu + inline field icon)

The password generator (settings, algorithm, and 7-day local history) lives
entirely in the agent — see `crates/cosmic-bwarden-agent/src/handler/generator/`
and `docs/password_generator_plan.md` for the full design. The extension has
two entry points into it, both fully local/offline (no server round trip).

### Message flow

```
Context menu click (any editable field)
      ▼
background: sendToAgent(GeneratePassword{settings:null})   ← reuses last-saved settings
      ▼
tabs.sendMessage GENERATE_COPY_TO_CLIPBOARD{password}   → content-generate.js writes to clipboard
                                                            (Chrome MV3 service workers have no
                                                             clipboard/DOM access; Firefox's
                                                             persistent background page could
                                                             write directly, but the relay keeps
                                                             one code path for both browsers)

Inline field icon (registration / change-password forms only)
      ▼
content-generate.js: runtime.sendMessage(GeneratePassword{settings:null})
      ▼
background: sendToAgent(...) → agent generates + records history
      ▼
content-generate.js fills the clicked field's group directly (no clipboard)
```

### Agent actions

- **`GeneratePassword { settings: Option<GeneratorSettings> }`** → `GeneratedPassword { password }`.
  `Some(settings)` persists them as the new device-wide "last used" settings
  (the desktop pane's Generate button, and any CLI/browser caller fully
  specifying options); `None` reuses whatever is currently persisted — this is
  what both extension entry points send, so they always follow whatever the
  user last configured on any surface. Every call appends to the local 7-day
  history regardless of `Some`/`None`.
- Works with no vault unlock and no account configured — generation is local
  RNG, not a vault operation, so both entry points work on a fresh install.

### Inline icon placement heuristic

The icon (`content-generate.js`) never appears on a plain login form (a lone
password field with no registration signal) — offering to overwrite a login
password would be actively harmful. It appears when:
- The scope has 2+ password fields (registration confirm-password pattern,
  or a change-password form's new+confirm pair) — a field explicitly marked
  `autocomplete="current-password"` is always excluded from the group, even
  in an otherwise-qualifying scope.
- A lone password field has `autocomplete="new-password"`, or the enclosing
  form's id/name/class/action text matches a registration/signup pattern.

Clicking the icon fills every field in its group (e.g. both "new" and
"confirm" boxes) with one freshly generated password; it never touches a
`current-password` field.

### Security notes

- The generated password transits the native-messaging pipe once per request
  (to be copied or filled) — this is the same trust boundary every other
  agent-returned secret (`GetPassword`, `GetTotp`) already crosses.
- The context-menu path never touches page DOM directly; the fill path writes
  only into the fields already identified as the "new password" group,
  through the same `setInputValue` dispatch (`input`+`change` events) used by
  the existing autofill content script.
- See `AGENTS.md`'s "Password Generator" section for the at-rest protection
  (and threat model) of the local 7-day history the agent maintains.

## Verification

The integration is verified by an E2E test suite in `crates/cosmic-bwarden-tests/src/browser_host.rs`. This test:
1. Spawns the main agent.
2. Spawns the agent in `browser-host` mode.
3. Simulates the browser by sending length-prefixed JSON to the bridge's `stdin`.
4. Verifies the JSON responses received on `stdout`.

To run the integration tests:
```bash
cargo test -p cosmic-bwarden-tests --lib browser_host
```
