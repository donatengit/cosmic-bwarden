# Browser Extension Integration

Cosmarden includes a native browser extension (currently targeting Firefox) that provides secure, high-performance vault access directly within the browser.

## Architecture: The Native Messaging Bridge

The extension follows a **"Thin Client" Invariant**: it contains no cryptographic logic and stores no secrets. Instead, it acts as a UI proxy for the `cosmarden-agent`.

Communication is handled via the [WebExtensions Native Messaging API](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging).

### 1. The `browser-host` Subcommand

The `cosmarden-agent` includes a specialized mode triggered by the `browser-host` subcommand. This mode is responsible for:
- Reading JSON messages from `stdin` (sent by the browser).
- Translating JSON to the internal `Action` protocol (using Postcard).
- Forwarding the action to the main agent process via the local Unix socket.
- Translating the agent's `Response` back to JSON.
- Writing the JSON response to `stdout` (read by the browser).

### 2. Configuration & Compilation

The browser integration is guarded by a Cargo feature flag in `cosmarden-agent`:

- **Feature Name**: `browser-host`
- **Default**: Enabled
- **Crate**: `crates/cosmarden-agent`

To compile the agent without browser support:
```bash
cargo build -p cosmarden-agent --no-default-features
```

The `browser-host` feature requires the `io-std` feature of `tokio` for asynchronous access to standard input and output streams.

### 3. Extension source layout

Source lives in `browser-extension/`. Plain vanilla JS — no bundler, no framework.

| File | Responsibility |
|---|---|
| `background.js` | Native messaging queue, grey badge with the domain match count, theme-aware + lock-state icon switching (`<theme>_locked*.png` when locked *or* logged out) |
| `background-save.js` | Save-prompt state machine: per tab+frame pending credentials, save/update decision via `CheckLoginMatch` |
| `theme.js` | Design tokens (light+dark palette, radii, shadows) for the shadow-DOM surfaces — must stay in sync with `popup/popup.css`; loaded first in content scripts |
| `host-match.js` | `hostMatches`/`hostMatchesAny`: the one host-comparison used by the popup (tab vs entry) and the content script (frame vs entry) |
| `content-dom.js` | DOM primitives: ancestor-aware `isVisible`, `collectElements` (open-shadow-root traversal), `formOf`/`treeRootOf`, `setInputValue` (native prototype setter) |
| `content.js` | Form fill injected into pages |
| `content-heuristics.js` | Shared pure heuristics (username-field detection, password-field roles, submitted-credential capture); loaded after `content-dom.js` |
| `content-submit.js` | Login-form submit detection → `LOGIN_SUBMITTED` to background |
| `content-bar.js` | In-page "Save/Update password?" floating card (open shadow root; never receives the password) |
| `content-generate.js` | Clipboard relay for the context-menu generate action (Chrome MV3 service workers have no clipboard access); no page scan |
| `clipboard-clear.js` | Shared 30 s clipboard auto-clear for the popup and content-script copy paths |
| `popup/popup.js` | View management, list queries, domain helper, header wiring |
| `popup/popup-list.js` | Vault-list row rendering and per-row action buttons |
| `popup/popup-fill.js` | Fill the active tab's form from a Login entry |
| `popup/popup-list-actions.js` | Row actions: open the site, copy-username/password dropdown |
| `popup/popup-menu.js` | Shared dropdown plumbing: positioning, one-open-menu state, ARIA menu keyboard model (used by the copy dropdown and the generator menu) |
| `popup/popup-lock.js` | Locked-vault view: TPM PIN unlock, DA lockout feedback; loaded *before* `popup.js` — see "Script load order gotcha" below |
| `popup/popup-detail.js` | Detail view, secret reveal/copy (on-demand via `GetPassword`) |
| `popup/popup-edit.js` | Edit/add form, field rendering |
| `popup/popup-generator.js` | Header generate menu (Generate password → agent + clipboard, Show previous → 7-day history view with per-row reveal/copy) |
| `popup/popup-state.js` | View/search/draft persistence across popup close+reopen, in `storage.session`; scoped to the tab's domain |
| `popup/icons.js` | Inline SVG icons for dynamically built buttons (monochrome, `currentColor`), parsed once at load and cloned per button — no dynamic `innerHTML` assignment, which AMO's linter flags. Static header icons live in `popup.html` |
| `popup/popup.css` | Popup design system: tokens, `.btn` component classes, views; dark mode via `prefers-color-scheme` |

**Icons**: `browser-extension/icons` is a symlink to the repo-root `icons/`
folder (`black*.png` for light theme, `white*.png` for dark). `zip -r`
dereferences symlinks, so `pack-extension` embeds them correctly.

**Manifest, one file for both engines**: `background` carries both
`service_worker` (Chrome MV3) and `scripts` (Firefox MV3 event page). Firefox
ignores `service_worker` and Chrome ignores `scripts`, so `web-ext lint`
reports `BACKGROUND_SERVICE_WORKER_IGNORED`; that warning is expected and is
why `background.js` pulls `background-save.js` in through `importScripts()`
on Chrome only. Removing it would mean a build step that rewrites the manifest
per browser, so the extension keeps one manifest that both engines load.

**Minimum version**: `browser_specific_settings.gecko.strict_min_version` is
`142.0`. `data_collection_permissions` is understood from Firefox 140 on
desktop and 142 on Android, and AMO requires that key for every new
submission, so a lower floor makes `web-ext lint` emit
`KEY_FIREFOX_*_UNSUPPORTED_BY_MIN_VERSION` again. One number covers both
because `gecko.strict_min_version` also sets the Android floor; no
`gecko_android` key is declared since native messaging does not exist on
Firefox for Android, which keeps the extension desktop-only on AMO.

**Unit-test loading gotcha**: `background-save.test.js` injects `sendToAgent` as
a `new Function` parameter, but `background.js` *declares its own* `sendToAgent`
(the native-messaging queue), which shadows any injected parameter — a unit test
that injects it will hang forever. Load `background.js` with a fake
`connectNative()` port whose `postMessage()` resolves the agent call and
delivers the response through the real `onMessage` listener (see
`background.test.js`).

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
3. Generates the host manifest JSON in `~/.mozilla/native-messaging-hosts/com.enikeev.cosmarden.json`.

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

## Autofill (fill)

`popup/popup-fill.js` fetches the full entry, checks the active tab against the
entry's URIs, and sends `FILL_FORM` carrying the entry's hosts:

```
popup-fill.js: GetEntry → entryMatchesTab(entry, tab.url)
      ▼
tabs.sendMessage(FILL_FORM{username, password, entryHosts})
      ▼
content.js in every frame: hostMatchesAny(location.hostname, entryHosts) → fillForm
```

Properties that matter, each pinned by a unit or E2E test:

- **Every frame runs the content script** (`content_scripts[0].all_frames`), so a
  login form inside an iframe is reachable. The message is broadcast, and each
  frame matches its *own* host against the entry's hosts. The guard is
  deliberately not the tab's host: a cross-origin auth iframe
  (`login.example.com` inside `example.com`) must be fillable, while an ad
  iframe on the same page must not be. A frame whose host cannot be named
  (`file://`, `about:blank`, a sandboxed frame) matches only an entry that has
  no host either. The broadcast reaches every frame's isolated world — the
  payload is not readable by page scripts — but only a matching frame writes
  it into the DOM.
- **`popup-fill.js` awaits the send.** No frame answering (a tab opened before
  the extension was installed or reloaded) now shows "Failed to fill form."
  instead of closing the popup as if the fill had worked.
- **Open shadow roots are traversed** (`content-dom.js::collectElements`) by
  fill and submit capture. A login widget inside a web
  component has no form in the document tree, so `content.js` scopes the
  username search to the shadow root and `content-submit.js` uses the root as
  the submit scope. Submissions there are captured through the click and Enter
  paths: a `submit` event is not composed and never reaches the window
  listener, and a composed click/keydown arrives retargeted to the component's
  host, so both handlers resolve the real node with `composedPath()`.
- **Values are written through the native prototype setter** plus `input` and
  `change` events, both `composed`. A plain `el.value = x` is swallowed by
  React's per-element value tracker: the field looks filled, `onChange` never
  fires, and the next render or submit sends the old value. Without the
  composed flag a framework listening on a container outside the field's shadow
  root never sees the event at all.
- **`readonly` is not invisible.** readonly blocks typing, not script
  assignment; sites that set it until focus would otherwise be unfillable.
- **Visibility walks the ancestor chain**, crossing shadow hosts. An element
  reports its own `display`, so a field inside a `display:none` wrapper used to
  count as visible and could outrank the real one.
- **A text field marked `autocomplete="current-password"` or `"new-password"`
  is a password field**, for both fill and submit capture: custom masked
  widgets, and "show password" toggles that swap `type`.
- **Fields associated through the `form` attribute are included** via
  `form.elements`. They are not descendants of the form, so a subtree query
  missed them and the password was filled without the username.

## Save/Update Prompt (notification bar)

When the user submits a login form, the extension offers to save the credentials
as a new Login entry, or to update an existing entry when the domain+username
matches but the password changed (Bitwarden-style notification bar).

### Message flow

```
content-submit.js ──LOGIN_SUBMITTED{username,password,url}──▶ background (pending map keyed by tab+frame)
   tabs.onUpdated 'complete' (or a ~2 s fallback timer for AJAX/SPA logins)
      ▼
GetConfig → locked or needs_login? → defer (see "Locked vault" below)
CheckLoginMatch{domain,username,password} → save | update | silent
      ▼
tabs.sendMessage SHOW_SAVE_BAR{mode,domain,username,entryName}   ← never contains the password
      ▼
content-bar.js renders the bar → SAVE_BAR_ACTION{save|update|unlock|dismiss}
      ▼
background: AddEntry{..., uris:[origin]}  or  UpdateLoginPassword{id,password}
then badge refresh + HIDE_SAVE_BAR on success, SAVE_BAR_ERROR on failure
```

### Pending state is keyed by tab *and* frame

The content scripts run in every frame, so a page can submit two logins at
once (its own form plus an embedded widget). `background-save.js` keys the
pending credential as `pendingSave:<tabId>:<frameId>` and targets every
`SHOW_SAVE_BAR` / `HIDE_SAVE_BAR` / `SAVE_BAR_ERROR` at `{ frameId }`, so each
bar appears in the frame that submitted and a click saves that frame's
credential. `onTabComplete` and `onTabRemoved` are tab-level events with no
frame argument, so they enumerate the keys for that tab
(`pendingFramesForTab`). Keying by tab alone made the second submission
overwrite the first, and the visible bar then saved the wrong password.

The frame id is stable across navigations within a frame (checked on a
same-origin and a cross-origin subframe navigation), but it does not survive
the frame being destroyed: a widget that removes itself on success, or a form
with `target="_top"`, leaves the pending keyed to a frame `tabs.sendMessage`
can no longer reach. `showBar` therefore falls back to the top frame once
instead of dropping the credential, and every bar message carries the
`pendingKey` back on click so a bar in the top frame still saves the credential
of the frame that submitted. The key is accepted only when it names a pending
in the clicking tab. While the submitting frame exists, the bar renders inside
it, so a widget in a small iframe shows a correspondingly small bar — the
alternative is putting a prompt about that frame's credential on an unrelated
page.

### Locked vault: defer, don't drop

A login submitted while the vault is locked is held, not discarded — the user
unlocks moments later and still wants the prompt. The pending entry is marked
`awaitingUnlock` (and left re-evaluable, since `evaluated` otherwise blocks a
second pass), and a `mode: 'locked'` bar offers **Unlock**, which asks the
background to `action.openPopup()`. Once unlocked, the popup sends
`VAULT_UNLOCKED`; the background sweeps `storage.session` for every
`pendingSave:<tabId>` key and re-evaluates each into a real Save/Update bar.

Two details keep this from losing the credential as silently as the drop it
replaced — both are regression-tested in `background-save.test.js`:

- **The locked bar's 30 s auto-dismiss must not notify the background.** Routing
  it through `SAVE_BAR_ACTION{dismiss}` clears the pending state, and opening
  the popup and typing a PIN routinely takes longer than 30 s. The bar removes
  itself silently; only an explicit **Dismiss** click clears the credential.
- **The 90 s TTL is restarted once, at the first deferral.** The original alarm
  runs from the form submit, so whatever is left of it is no basis for how long
  unlocking takes. It is *not* re-armed on later evaluations — that would keep a
  credential alive for as long as the tab keeps navigating.

`action.openPopup()` needs a user gesture the background does not always have,
and its failure is silent, so the locked bar's buttons stay enabled: the user
can retry, or open the popup from the toolbar and dismiss the bar by hand.

### Agent actions

- **`AddEntry`** now accepts optional `totp` and `uris` fields (both
  `#[serde(default)]`, so older JSON payloads without the keys keep parsing).
  The save bar sends `name = domain` and `uris = [{ "uri": <origin>, "match_type": null }]`
  so the entry is domain-matchable afterwards.
- **`CheckLoginMatch { domain, username, password }`** → `LoginMatch { entry_id, name, password_matches }`.
  The agent compares the *submitted* password against its decrypted stored copy;
  the stored password never leaves the agent — the response is a single equality
  bit about a value the client already possesses. Domain matching uses the
  shared rules in `cosmarden_core::domain` (exact / boundary-subdomain in
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

## Generate Password (popup menu, context menu)

The password generator (settings, algorithm, and 7-day local history) lives
entirely in the agent — see `crates/cosmarden-agent/src/handler/generator/`
and `docs/password_generator_plan.md` for the full design. The extension has
two entry points into it, both fully local/offline (no server round trip).

### Message flow

```
Popup header menu → Generate password
      ▼
popup-generator.js: runtime.sendMessage(GeneratePassword{settings:null})
      ▼
agent generates + records history → GeneratedPassword{password}
      ▼
popup: navigator.clipboard.writeText + scheduleClipboardClear (30 s)
       (the popup is a normal extension page, so it writes the clipboard
        directly — no service-worker relay needed here)

Popup header menu → Show previous
      ▼
popup-generator.js: runtime.sendMessage(GetPasswordHistory)
      ▼
agent replies PasswordHistory{entries} (7 days, newest first, decrypted)
      ▼
popup renders the history view: masked rows with reveal/copy per row

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
```

### Agent actions

- **`GeneratePassword { settings: Option<GeneratorSettings> }`** → `GeneratedPassword { password }`.
  `Some(settings)` persists them as the new device-wide "last used" settings
  (the desktop pane's Generate button, and any CLI/browser caller fully
  specifying options); `None` reuses whatever is currently persisted — this is
  what both extension entry points send, so they always follow whatever
  the user last configured on any surface. Every call appends to the local
  7-day history regardless of `Some`/`None`.
- **`GetPasswordHistory`** → `PasswordHistory { entries }`, pruned to the last
  7 days, newest first. The popup is the only extension surface that asks for
  it; `background.js` rejects every agent action from a content script, so a
  page script cannot enumerate generated passwords.
- Works with no vault unlock and no account configured — generation is local
  RNG, not a vault operation, so both entry points work on a fresh
  install. The popup therefore keeps the menu button on the lock screen too.

### Popup history view

- Rows are masked (`••••••••`) when the view opens; reveal and copy are
  explicit clicks, the same rule the detail view applies to vault secrets.
- The history is **never persisted** by the popup. `popup-state.js` maps the
  history view to `list` before writing `storage.session`, and leaving the
  view clears its rows — the decrypted passwords live only in the open
  document, exactly like `GetPassword` results.
- Copying a row uses the shared `scheduleClipboardClear`, so generated
  passwords clear on the same 30 s timer as vault copies.

### No passive field scan

The extension does not search the page for password fields on load. The only
DOM discovery is `content.js::fillForm`, which runs when the popup sends
`FILL_FORM` after an explicit Fill click. It collects password inputs, filters
to visible ones, and scopes the username search to each field's form (or its
shadow root).

The toolbar badge is the only passive site indicator: a grey count of vault
entries whose stored URIs match the tab's host, computed by the agent. It does
not read the page.

The inline per-field generate icon was removed. It required a full-page scan at
`document_idle` plus a `MutationObserver` per open shadow root, and it was the
only reason any content script inspected inputs before a user action.
Generation now happens only through the popup menu and the context menu; both
write to the clipboard, never to page inputs.

### Security notes

- The generated password transits the native-messaging pipe once per request
  (to be copied) — this is the same trust boundary every other agent-returned
  secret (`GetPassword`, `GetTotp`) already crosses.
- The context-menu path never touches page DOM; `content-generate.js` only
  writes the clipboard. Stored credentials are written only by `content.js`
  on an explicit `FILL_FORM`, through the same `setInputValue` dispatch
  (`input`+`change` events) used by the rest of autofill.
- See [`docs/password_generator_plan.md`](password_generator_plan.md) for the
  at-rest protection (and threat model) of the local 7-day history the agent
  maintains.

## PIN Unlock (TPM 2.0)

The popup offers a quick PIN unlock when the vault is locked, mirroring the
desktop UI's PIN flow (see [`docs/tpm.md`](tpm.md)). The extension only ever
collects a PIN here — master password unlock stays a desktop/CLI-only action;
the popup never collects the master password, so the "thin client, no secrets"
invariant holds for the locked state too.

### Message flow

```
popup opens, GetConfig → is_locked: true
      ▼
RequestUnlock (unchanged — lets the desktop applet also react)
      ▼
CheckTpm → TpmStatus{available, configured}
      ▼
available && configured?
   yes → show PIN input (view-locked / popup-lock.js)
   no  → show the existing "unlock via the COSMIC app or applet" message

PIN submitted → UnlockWithPin{pin}
   Ack                              → back to the entry list (re-fetches GetConfig/entries)
   Error{ERR_TPM_UNSEAL_FAILED}      → GetTpmDaStatus → show attempts-remaining/lockout line
   Error{other message}              → shown as-is (environmental failure, not a wrong PIN)
```

`CheckTpm` and `GetTpmDaStatus` are safe to call on any build: without the
agent's `tpm` feature they return `TpmStatus{available:false,...}` /
`TpmDaStatus::default()` rather than erroring, so the popup always falls back
to the plain locked message when TPM PIN unlock isn't compiled in or not
configured for the account.

### Script load order gotcha

`popup-lock.js` (which defines `showLockedView`) is loaded **before**
`popup.js` in `popup.html`, not after, even though it calls `popup.js`
globals (`showView`, `browser`) from its function bodies. `popup.js`'s init
IIFE `await`s `browser.tabs.query`, and browsers run a microtask checkpoint
— which can resume that `await` — right after a `<script>`'s synchronous
portion finishes, *before* the parser reaches the next `<script>` tag. If
`popup-lock.js` loaded after `popup.js`, that resumed continuation could call
`showLockedView()` before it was defined. Loading it first sidesteps the
race: nothing at `popup-lock.js`'s own top level calls into `popup.js`, so
order doesn't matter for it, but order very much matters for what `popup.js`
can already call by the time its `await` resolves.

## Verification

The integration is verified by an E2E test suite in `crates/cosmarden-tests/src/browser_host.rs`. This test:
1. Spawns the main agent.
2. Spawns the agent in `browser-host` mode.
3. Simulates the browser by sending length-prefixed JSON to the bridge's `stdin`.
4. Verifies the JSON responses received on `stdout`.

To run the integration tests:
```bash
cargo test -p cosmarden-tests --lib browser_host
```
