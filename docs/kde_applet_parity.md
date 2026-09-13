# Thin KDE Plasma applet: parity with the COSMIC panel popup

Analysis of whether a **very thin** Plasma 6 panel plasmoid can match
the COSMIC panel applet (`view/applet/`) by talking to the existing
`cosmarden-agent`. Not a design for a full Kirigami vault app.

Verdict: **applet-popup parity is possible** if the plasmoid is a
`PlasmoidItem` with a C++ or Rust QML plugin that speaks the agent's
postcard Unix-socket protocol (including a dedicated `Subscribe`
connection). A QML-only widget that shells out to `cosmarden-cli`
via `Plasma5Support.DataSource` `engine: "executable"` is **unfit**.

The **libcosmic vault window already opens on Plasma**. Theme-connection
`ERROR` lines (`cosmic::theme` / `cosmic_config` talking to
`cosmic-settings-daemon`) are log noise, not a launch failure. Open
Vault from a future plasmoid can spawn `cosmarden-applet` in
application mode, the same binary COSMIC uses.

All KDE-specific code is gated on **`--features kde`** (UI crate, not
default, not auto-detected). `just build` / `just test-unit` stay
COSMIC. `just build-kde` produces **`cosmarden-tray`**, a separate
agent-only process (StatusNotifierItem + compact popup). It does not
open the vault window. Context menu: copy username/secret, generate,
lock, logout, quit — one Quit row, matching the popup footer
(`app::applet_menu::quit_actions`); the dropped Lock-and-Quit /
Logout-and-Quit pair are no longer in the popup either. Left-click /
Unlock opens a popup that reuses `view/applet/` (`secure_input` for
PIN/password). DBusMenu cannot host a password field. `cosmarden-applet`
stays the COSMIC applet / vault window and does not host the tray.
`just test-unit-kde` runs the gated tests.

This inventory is taken from the shipped UI source, not from
`CONTEXT.md` / `docs/cosmic_integration.md`. Where those docs disagree
with source, source wins (see [Doc vs source](#doc-vs-source)).

The unit test `applet_view_message_inventory_matches_parity_report`
(`crates/cosmarden-ui/src/app/tests/applet_inventory.rs`) pins the
`Message::` set actually wired in `view/applet/`. If a new popup control
is added, that test fails until this report and the test list are
updated together.

## Doc vs source

| Claim | Where | Source |
|---|---|---|
| Version string next to "Open Vault" | `CONTEXT.md` "Versioning & Protocol Compatibility" | **Absent.** `view/applet/menu.rs` `header_row` has no version label. Protocol mismatch is a dedicated error+Quit surface in `view/applet/mod.rs`, not a version readout. |
| Locked menu is only "Open Vault Window" and "Quit" | `CONTEXT.md` "COSMIC Panel Applet" | **False.** Generate is unconditional (`menu.rs` comment: local RNG, works locked / with no account). Logout is unconditional. Lock is unlocked-only. Quit footer now holds a **single Quit row** (`app/applet_menu.rs`
`quit_actions()`); the Lock-and-Quit / Logout-and-Quit pair was removed. |
| Result rows are two text buttons: primary label + "Password"/"Note"/"Private key" | `CONTEXT.md` "Quick Search" | **False.** `view/applet/search.rs` uses a truncated domain/name label plus icon buttons: 📂 open-in-vault, 🔗 open-link (logins whose name is URI-like), 🔑 copy-secret. Username is a tooltip on the login label, not a second text button. |

## COSMIC applet inventory (source)

Panel process split: `lib.rs` `detect_run_mode` returns
`RunMode::{Applet, Application, Tray}` — `COSMIC_PANEL_NAME` set means
applet, `COSMARDEN_MODE=tray` means the Plasma tray, anything else is
the vault window. `cosmarden-applet` covers applet and application
mode; `cosmarden-tray` is the second binary behind `--features kde`.
Both link the same `Application` impl in `runtime.rs`. Popup content is
`applet_popup_content`; the panel button is `applet_view`.

### Panel icon — `view/applet/mod.rs`

| Control | Wiring | Agent IPC |
|---|---|---|
| Symbolic brand mark | `APPLET_ICON_SVG` when `View::is_unlocked()` (`Vault` / `Settings` / `PasswordGenerator`); `APPLET_ICON_LOCKED_SVG` (dots removed) for Loading/Setup/Unlock. `icon::from_svg_bytes(...).symbolic(true)` so libcosmic recolors to the panel foreground. | none (local) |
| Click toggles popup | `Message::AppletIconClicked` → `update/applet.rs` open/close with 200 ms same-press debounce (`APPLET_POPUP_REOPEN_DEBOUNCE_MS`) | on open: `Action::Version`, `Action::GetConfig` |
| Tooltip | `fl!("app-title")` via `applet_tooltip`; `Message::Surface` | none |

### Popup chrome — `view/applet/mod.rs`, `menu.rs`

| Control | Wiring | Agent IPC |
|---|---|---|
| Protocol mismatch | If `protocol_mismatch`: body `fl!("protocol-version-mismatch")` + Quit (`Message::Exit`). Nothing else. | `Action::Version` compared to `PROTOCOL_VERSION` (`app/tasks.rs` `check_protocol_version`) |
| Open Vault Window | `Message::OpenVaultRequested`. Destructive styling when `sync_failed && unlocked`. | Closes popup; requests a COSMIC activation token with `exec: "open-vault"` (`update/activation.rs` `classify_activation` → `SpawnVault`). Spawns **this same binary** with `COSMARDEN_MODE=application` and `COSMIC_PANEL_NAME` removed. |
| Not-synced icon | Only `sync_failed && unlocked`. Session-expired (`error` contains `"session token"`) → `Message::LogoutClicked`; else `Message::SyncClicked`. | `Action::Logout` or `Action::Sync` |
| Generate | Always shown. `Message::AppletGeneratePasswordRequested` | `Action::GeneratePassword { settings: None }` via `generator_actions::generate_with_stored` — reuses last-used settings, does not persist new ones. Works locked and with no account. |
| Lock | Unlocked only. `Message::LockClicked` | `Action::Lock` via `auth_actions::lock` |
| Logout | Always shown. `Message::LogoutClicked` | `Action::Logout` via `auth_actions::logout` |
| Not configured | `View::Setup`: `fl!("not-configured")`. No login form in the applet. | none in the popup; login is the full window |
| Error line | `applet_error` body text | previous IPC error |
| Copied toast | `applet_toasts` / `fl!("copied-to-clipboard")` | none |
| Quit | One `menu_button` row, always (`menu::quit_footer` → `app::applet_menu::quit_actions`). `QuitAction::message()` is `Message::Exit`. | none. Leave the agent running for SSH/CLI |

`Action::Quit` is **not** sent by the applet. That variant stops the agent
(CLI `quit` / `subscription_handler`). Applet "Quit" only destroys the
libcosmic applet process.

### Unlock — `view/applet/unlock.rs`

Handlers: `update/applet.rs`. Builders: `update/auth_actions.rs`. TPM
status: `update/lifecycle.rs` `CheckTpm` / `GetTpmDaStatus`.

| Control | Wiring | Agent IPC |
|---|---|---|
| Wait spinner | Until `tpm_status_known` so a PIN user never types into a password field that then swaps | `Action::CheckTpm` |
| PIN field + reveal + submit | When `UnlockMode::Pin`. `AppletPinChanged` / `AppletTogglePinReveal` / `AppletPinSubmitted` | `Action::UnlockWithPin { pin }` |
| Attempts caption | After wrong PIN (`pin_incorrect`): `pin_feedback_line()` (DA remaining or `pin-incorrect`) | `Action::GetTpmDaStatus` on `ERR_TPM_UNSEAL_FAILED` |
| Use master password instead | `AppletUseMasterPasswordInstead` | none (local `UnlockMode::Password`) |
| Master-password field + reveal + submit | `UnlockMode::Password`. `AppletUnlockPasswordChanged` / `AppletToggleUnlockPasswordReveal` / `AppletUnlockSubmitted` | `Action::Unlock { password }` |
| Optional PIN on password unlock | If `tpm_available`: extra field `unlock_pin` (`UnlockPinChanged` / `UnlockPinRevealToggled`), empty = disable. Caption `pin-reenable-note-short` or `pin-optional-note-short` | After successful unlock, `apply_unlock_pin` → `SetupTpmPinFromUnlocked` or `DisableTpmPin` |
| TPM PCR / missing blob | `ERR_TPM_STATE_CHANGED` / `ERR_TPM_BLOB_MISSING` force password mode with recovery copy | those stable `Response::Error` strings |

`Event::Unlocked` (`main.rs` Subscribe stream → `lifecycle.rs`) is the
secondary path onto the search view; `AppletUnlockResult` / `AppletPinResult`
switch immediately.

### Search / copy — `view/applet/search.rs`, `app/applet_search.rs`

| Control | Wiring | Agent IPC |
|---|---|---|
| Search field | `AppletSearchChanged` | `Action::GetSidebarEntries { query, entry_type: None, only_pinned, domain: None }` via `vault_actions::applet_search`. Empty query ⇒ `query: None` and `only_pinned: true` regardless of the star (`effective_only_pinned`). Cap 10 after dropping Card/Identity/BankAccount/DriversLicense/Passport (`build_applet_rows`). |
| Favourites star | `AppletToggleFavouritesFilter` | same `GetSidebarEntries` with toggled `only_pinned` |
| Empty state | `no-pinned-entries` vs `no-results` | none |
| Login label | Copies username already on `SidebarEntry` (`AppletCopyPrimary`). No extra IPC. Disabled if no username. Tooltip is the username. Label is PSL eTLD+1 of the name (`extract_domain_label`), truncated to 28 chars. | clipboard only |
| 🔑 / note-or-ssh label | `AppletCopySecret` | `Action::GetPassword { id, password: None }` via `vault_actions::fetch_password`. Agent maps login password / note body / SSH private key into `Response::Password { password: Secret }`. |
| 📂 | `AppletOpenInVault` | `Action::SetPendingEntry { id }` then the Open Vault Window path. Agent delivers `Event::OpenEntry` to a Subscribe client (the vault window). Rendered disabled in `RunMode::Tray` (`search.rs` `open_in_vault` returns `None`), which has no vault window. |
| 🔗 | `AppletOpenLink` if `is_uri_like(name)` | none. Detached `xdg-open` (`activation::open_link`) |
| Reprompt row | Replaces that result row when GetPassword returns `"reprompt_required"`. Secure input + submit + cancel + reveal | `GetPassword { id, password: Some(master) }` |

Clipboard: `copy_to_clipboard_with_autoclear` (`update/mod.rs`). Writes via
iced clipboard, remembers the value, after `CLIPBOARD_CLEAR_SECS` (30)
reads back and clears only if still ours. Generate and secret copy both
unwrap `Secret` only at this clipboard call.

### Subscribe / notifications — `runtime.rs`, `lifecycle.rs`, `unlock_notify.rs`

Not popup widgets, but applet behavior:

| Behavior | Source | Agent IPC |
|---|---|---|
| Live lock/unlock/vault-change | `Application::subscription` opens its **own** `UnixStream` (not `AgentClient`), sends `Action::Subscribe`, loops on `Response::Event` | `Subscribe` then streamed `Event::{Locked, Unlocked, VaultChanged, UnlockRequested, PinRequested, OpenEntry}` |
| Unlock requested (SSH/browser) | Does **not** auto-open the popup (cosmic-panel drops unfocused popups). Sends `org.freedesktop.Notifications` `Notify` | events above |

### Registration (COSMIC-only install)

| Artifact | Source |
|---|---|
| `.desktop` `X-CosmicApplet=true`, `X-CosmicHoverPopup=Auto`, `OnlyShowIn=COSMIC;` | `crates/cosmarden-ui/resources/com.enikeev.cosmarden.desktop` |
| `.ron` applet metadata | justfile `applets_dir` |
| Mode split | `lib.rs` `RunMode`: `COSMIC_PANEL_NAME` → Applet, `COSMARDEN_MODE=tray` → Tray, else Application |

### Actions the applet actually sends

From the handlers above, not the whole `protocol::Action` enum:

`Version`, `GetConfig`, `GetSidebarEntries`, `GetPassword`,
`GeneratePassword { settings: None }`, `Unlock`, `UnlockWithPin`,
`Lock`, `Logout`, `Sync`, `SetPendingEntry`, `SetupTpmPinFromUnlocked`,
`DisableTpmPin`, `CheckTpm`, `GetTpmDaStatus`, `Subscribe`.

Not sent by the applet (full window / extension / CLI): `Login`,
`Register`, `GetEntry` / `GetEntryMeta` / `GetTotp`, all `Add*` /
`UpdateEntry` / `DeleteEntry`, `GetEntries`, generator settings/history
UI, `CheckLoginMatch`, `RequestUnlock`, `Action::Quit` (agent stop),
`CopyToClipboard` (agent stub: "handle in client").

### Out of thin-applet scope (full vault window)

`view/vault/`, Settings, login/register, cipher CRUD, TOTP reveal,
generator Settings/History tabs, TPM setup forms in Settings
(`TpmSetup*` messages live in `update/applet.rs` only because that
file also owns TPM result handling; they are **not** wired in
`view/applet/`). These are not KDE gaps.

## Shipped KDE tray (source)

`--features kde` compiles `src/kde.rs` plus `src/kde/{tray,menu,theme,icon}.rs`
and the `cosmarden-tray` binary (`src/bin/cosmarden-tray.rs`). That binary sets
`COSMARDEN_MODE=tray` and calls `runtime::run_tray`, which uses
`cosmic::app::run` rather than `run_single_instance`, so it coexists with
`cosmarden-applet`; its main window stays empty and only the popup is shown.

| Piece | Wiring | Notes |
|---|---|---|
| StatusNotifierItem | `kde/tray.rs` `CosmardenTray` (ksni). `icon_pixmap` builds an ARGB pixmap from the two symbolic SVGs (`kde/icon.rs`) because Plasma does not recolor a named symbolic icon. | A tray that fails to register logs `error!` and leaves the process running without an icon. |
| Panel contrast | `kde/theme.rs` reads the Plasma theme colour over D-Bus and watches it for changes. | Dark panel ⇒ light pixmap. |
| Left click | `activate()` → `TrayCommand::TogglePopup` → `Message::KdeTray` | Opens or closes the compact popup window (372×480, `AlwaysOnTop`) that reuses `view/applet/`. |
| Context menu | `kde/menu.rs` `build_menu()` — a pure `MenuNode` tree, unit-tested without a tray host. | Copy rows come from `applet_search::build_applet_rows`; chrome from `chrome_commands()`. |
| State push | `kde::publish_snapshot` after every applet-search update and after each tray command. | DBusMenu is read on demand, so the menu is rebuilt from the pushed snapshot. |
| Quit row | `chrome_commands()` ends in `Exit`; the lock/logout-and-quit pair was dropped along with the popup's. | See the Doc-vs-source table. |

`Message::KdeTray` is intercepted at the top of `update_app`, before the
lifecycle/auth/vault groups, and reassembles the snapshot after handling. Its
commands map onto the popup's existing messages (`AppletCopySecret`,
`LockClicked`, …) through `menu::command_to_message`, so the tray and the panel
share one handler per action.

## Plasma 6 classification

Grounded in KDE's Plasma 6 widget docs:

- Root must be `PlasmoidItem` (`import org.kde.plasma.plasmoid`).
  [Setup](https://develop.kde.org/docs/plasma/widget/setup/),
  [Porting to KF6](https://develop.kde.org/docs/plasma/widget/porting_kf6/).
- Panel icon = `compactRepresentation`; popup = `fullRepresentation`.
  Click sets `plasmoid.expanded` (custom compact needs a `MouseArea`).
- `metadata.json` with `"X-Plasma-API-Minimum-Version": "6.0"` and
  `"KPackageStructure": "Plasma/Applet"`. Without `6.0` Plasma 6 hides
  the widget. Entry is always `contents/ui/main.qml`.
- Native sockets / binary protocols need a **private C++ QML plugin**
  (or equivalent compiled plugin). QML-only cannot do File I/O or
  C++ libraries. [C++ API](https://develop.kde.org/docs/plasma/widget/c-api/).
- `Plasma5Support.DataSource` `engine: "executable"` runs a shell
  command and captures stdout. That is the QML-only escape hatch; it is
  classified **unfit** below, not recommended.
- Testing tools: `plasmawindowed`, `plasmoidviewer`
  ([Testing](https://develop.kde.org/docs/plasma/widget/testing/)).

| # | COSMIC applet behavior | Plasma 6 | Why |
|---|---|---|---|
| 1 | Panel icon, locked vs unlocked glyph, click-to-toggle popup | **Possible** | `compactRepresentation` + `plasmoid.expanded`; `PlasmaCore.IconItem` `isMask: true` recolors a monochrome SVG (Plasma counterpart of `symbolic(true)`). Ship the two existing SVGs in the plasmoid package. |
| 2 | Popup size ~360–372 × min 200 | **Possible** | `Layout.preferredWidth` / `minimumWidth` on `fullRepresentation`. System tray popups are a fixed size — **partial** if placed in the tray rather than the panel. |
| 3 | Tooltip with app title | **Possible** | `PlasmoidItem.toolTipMainText`. |
| 4 | Search-as-you-type, favourites star, 10-row cap, drop card/identity | **Possible** | PlasmaComponents text field + list. Query logic can reuse `applet_search` / `GetSidebarEntries` in the plugin (or a shared Rust lib). Must not hold passwords in the list model — `SidebarEntry` already does not. |
| 5 | Copy username from sidebar row | **Possible** | Username is not a `Secret`; still should use the password-manager clipboard path if treating it as credential-adjacent. |
| 6 | On-demand secret copy (`GetPassword` → clipboard, 30 s clear-if-still-ours) | **Partial** | Possible in a compiled plugin: `QClipboard` + timer. Plasma's Klipper records history unless the copy sets MIME `x-kde-passwordManagerHint` = `secret` (KeePassXC / plasma-pass / KWalletManager convention). iced's clipboard write on COSMIC has no Klipper equivalent, so this is extra KDE work, not a missing COSMIC feature. **Do not** put the `Secret` in a QML `property string` that outlives the copy. |
| 7 | Inline master-password reprompt | **Possible** | `TextField` `echoMode: TextInput.Password`; send `GetPassword { password: Some(...) }`. Compare the exact `"reprompt_required"` error string. |
| 8 | Generate with stored settings, copy result | **Possible** | `GeneratePassword { settings: None }`. Same clipboard rules as (6). No generator-settings UI in the thin applet (matches COSMIC applet). |
| 9 | Master-password unlock | **Possible** | Plugin sends `Unlock`; PIN/password stay in the plugin, not in a long-lived QML model. |
| 10 | TPM PIN unlock, DA caption, PCR/missing-blob recovery, optional reseal field | **Possible** | Same actions as the COSMIC applet (`UnlockWithPin`, `CheckTpm`, `GetTpmDaStatus`, `SetupTpmPinFromUnlocked`, `DisableTpmPin`). Hardware presence is agent-side. |
| 11 | Wait for CheckTpm before showing a form | **Possible** | Busy indicator until `TpmStatus` arrives. |
| 12 | Lock / Logout (agent stays up) | **Possible** | `Action::Lock` / `Action::Logout`. |
| 13 | Sync / session-expired → logout | **Possible** | `GetConfig.sync_failed` + `Action::Sync` / `Logout`. |
| 14 | Subscribe-driven view switch (lock/unlock/vault change) without polling | **Possible** only with a native socket plugin | Agent `Subscribe` **hijacks that connection** (`agent/src/lib.rs`: after `Ack`, the task only writes `Response::Event` and never reads another request). Requires a **second** Unix connection beside request-response. `AgentClient::send(Subscribe)` would poison the process-wide persistent socket (`core/src/agent_client.rs` `LazyLock` `CONN`). COSMIC UI already uses a separate `UnixStream` in `runtime.rs` `agent_subscription()`. |
| 15 | Desktop notification on UnlockRequested / PinRequested (no auto-open popup) | **Possible** | `org.freedesktop.Notifications` is DE-agnostic; Plasma implements it. Not auto-opening matches COSMIC (and avoids fighting `hideOnWindowDeactivate`, default `true` on `PlasmoidItem`). |
| 16 | Open URI via xdg-open | **Possible** | `QDesktopServices::openUrl` or detached `xdg-open`. Do not `Command::spawn` and drop `Child` (COSMIC already hit the zombie). |
| 17 | Protocol mismatch surface | **Possible** | `Action::Version` vs `PROTOCOL_VERSION`. |
| 18 | Not-configured caption (no login form) | **Possible** | Same as COSMIC applet: tell the user; login is not a popup job. |
| 19 | Open Vault Window / 📂 open-in-vault | **Possible** (libcosmic window on Plasma) | The same `cosmarden-applet` binary opens as a normal window when `COSMIC_PANEL_NAME` is unset. Confirmed on Plasma: the vault UI runs; libcosmic then logs theme-connection `ERROR`s without `cosmic-settings-daemon` (demoted under `--features kde`). A plasmoid cannot share the applet *process*, but it can spawn this binary like COSMIC's `SpawnVault`. `OnlyShowIn=COSMIC;` hides it from KDE app launchers — a KDE `.desktop` belongs behind `--features kde`, not in the default install. `SetPendingEntry` / `Event::OpenEntry` still work if that window is subscribed. |
| 20 | Activation-token `exec` (`open-vault`, `activate:lock`, `activate:generate`) | **Impossible** (COSMIC panel API) | `cosmic::applet::token::subscription`. Plasma has XDG activation for launching windows, but not this exec-string router, and there is no same-binary vault to spawn. |
| 21 | Quit tearing down the **UI process** | **Partial** | The plasmoid lives **inside plasmashell**. It cannot exit a panel process the way `Message::Exit` closes `cosmarden-applet` (or `cosmarden-tray`, which is its own process and does exit). Map Lock/Logout to IPC; map "just quit" to collapsing the popup (`plasmoid.expanded = false`). Do not send `Action::Quit` (that kills the agent). |
| 22 | 200 ms Wayland same-press popup debounce | **Partial** | Plasma's `expanded` toggle is a different event path. May still need a small debounce; do not copy libcosmic popup `anchor_rect` code. |
| 23 | Fluent `fl!` strings | **Out of scope** | Plasma uses `i18n()` / ki18n. Non-goal. |
| 24 | Pixel-identical COSMIC theme | **Out of scope** | Use PlasmaComponents / Kirigami. Non-goal. |
| 25 | Full vault window, Settings, CRUD, TOTP, generator pane | **Out of scope** | Not applet-popup parity. |

QML-only + `DataSource` `engine: "executable"` wrapping the CLI: **unfit**,
not a recommended path. Reasons: (a) `cosmarden-cli unlock --password`
puts the master password in argv (`/proc/<pid>/cmdline`); omit `--password`
and the CLI uses `rpassword` on a TTY the DataSource does not have;
(b) `generate` prints `password.expose()` to stdout, which the DataSource
buffers in QML; (c) each keystroke would spawn a process; (d) **no
Subscribe** — the CLI is one-shot `AgentClient::send`; (e) CLI `Unlock`
is master-password only (`commands/auth.rs`); `UnlockWithPin` is not a
CLI command. Search-as-you-type + live lock state cannot be built on
this transport without leaking secrets or polling.

## Transport comparison

Three thin-client transports already in this repo. None of them should
be changed for a plasmoid (non-goal: agent wire protocol, D-Bus agent
API, weakening `SO_PEERCRED` / `0600`).

### 1. Postcard Unix socket — `AgentClient` + raw Subscribe stream

- **Where:** `core/src/agent_client.rs`; UI Subscribe loop in
  `crates/cosmarden-ui/src/runtime.rs`; server in `crates/cosmarden-agent/src/lib.rs`.
- **Framing:** 4-byte LE length + postcard `Action` / `Response`.
  Same UID via `SO_PEERCRED`; socket mode `0600`.
- **Request-response:** `AgentClient::send` holds one persistent
  connection per process, reconnects once on I/O error. Suitable for
  search-as-you-type (`GetSidebarEntries` per keystroke) without
  respawning.
- **Subscribe:** after `Ack`, that socket is event-only. Must be a
  **second** connection. The UI already does this; a plasmoid plugin
  must too. Putting `Subscribe` through `AgentClient::send` would leave
  `CONN` stuck in the subscribe loop.
- **Secrets:** `Response::Password` / `GeneratedPassword` carry
  `db::Secret` (`ZeroizeOnDrop`, `#[serde(transparent)]`). Unwrap only
  at the clipboard (and never store in a QML property).
- **Plasmoid fit:** **recommended.** QML cannot speak postcard or Unix
  sockets; a compiled QML plugin (C++ `QLocalSocket` calling a small
  Rust FFI around `AgentClient`, or a Rust `cdylib` QML module) is the
  KDE-documented extension point for native I/O.

### 2. Browser-host JSON stdin/stdout

- **Where:** `agent/src/browser_host.rs`; native-messaging host; extension
  in `browser-extension/`.
- **Framing:** 4-byte LE length + JSON `Action` / `Response`. Internally
  forwards each message through `AgentClient::send` (postcard).
- **Subscribe:** **cannot.** The host is a request/response loop. A
  JSON `Subscribe` would `send()`, get `Ack`, and leave `AgentClient`'s
  persistent socket in the agent's subscribe loop — the next extension
  message would write a request the agent no longer reads. The
  extension never sends `Subscribe`; it polls.
- **Secrets:** JSON strings in the host and in JS. Acceptable for the
  browser (short-lived, user-gesture GetPassword). A plasmoid that
  spawned the host would still need a long-lived helper and would still
  lack Subscribe.
- **Plasmoid fit:** usable as a stopgap for one-shot calls **if** the
  helper is kept as a daemon and secrets stay out of argv. Unfit as the
  primary path for a Subscribe-driven search popup.

### 3. CLI subprocess

- **Where:** `cosmarden-cli`; clap subcommands; same `AgentClient`.
- **Subscribe:** **cannot.** No `Subscribe` subcommand; process exits
  after one response.
- **Secrets:** `--password` is argv. Interactive prompt needs a TTY.
  `generate` and `get --show-secrets` write plaintext to stdout.
  `list` / `list --pinned` are metadata-only and would be "safe" as
  DataSource fodder, but they cannot unlock, copy a password, or PIN
  unlock.
- **Plasmoid fit:** **unfit** for the popup. Rejected even as a
  QML-only prototype if that prototype handles unlock or copy.

| Need | Postcard `AgentClient` + extra Subscribe socket | Browser-host JSON | CLI spawn |
|---|---|---|---|
| Search-as-you-type | yes (persistent conn) | yes if helper kept alive | no (spawn per key) |
| Subscribe / live lock | yes, second socket | no (would poison `CONN`) | no |
| Unlock / PIN without argv leak | yes | yes (JSON on stdin) | no (`--password` or TTY) |
| `Secret` zeroize on drop | yes, if plugin stays in Rust | JSON `String` in host | stdout `String` |
| QML-only | no | no (still a helper process) | yes, and that is the problem |
| Recommended for plasmoid | **yes** | no | no |

## COSMIC-only surfaces (do not reimplement)

A thin KDE applet is a **new DE surface over the existing agent**, not
a libcosmic port and not a second vault app.

Do not reimplement:

1. The full libcosmic vault window (`view/vault/`, Settings, login /
   register, cipher CRUD, TOTP, generator Settings/History).
2. COSMIC panel registration: `.desktop` `X-CosmicApplet` /
   `OnlyShowIn=COSMIC`, `{share}/cosmic/applets/*.ron`,
   `COSMIC_PANEL_NAME` mode split of `cosmarden-applet`.
3. Activation-token spawn of the same binary (`open-vault`,
   `activate:lock`, `activate:generate` in `update/activation.rs`).
4. libcosmic popup positioning (`clamped_anchor_rect`, cosmic-panel
   hover-popup).
5. A D-Bus API on the agent, a JSON mode on the main socket, or any
   relaxation of `SO_PEERCRED` / `0600`.

DE-agnostic pieces that already work on Plasma and must stay the
backend layer:

- `cosmarden-agent` systemd user unit
- logind lock-on-sleep
- SSH agent socket
- browser extension + native-messaging host
- CLI
- generator settings/history on disk (device-global)

## Recommended thin shape

**Agent keeps** vault keys, IPC, Subscribe fan-out, generator,
TPM seal/unseal, SSH, browser-host, autolock, logind.

**KDE plasmoid owns** (compiled plugin + QML):

- `metadata.json` (`X-Plasma-API-Minimum-Version: 6.0`,
  `KPackageStructure: Plasma/Applet`)
- `PlasmoidItem` compact icon (two symbolic SVGs) + fullRepresentation
  popup
- Two Unix connections to the existing socket: `AgentClient` (or FFI
  equivalent) for RPC, a dedicated stream for `Subscribe`
- Unlock (password + PIN), search, favourites, copy, generate, lock,
  logout, sync-failed affordance, protocol-mismatch, not-configured
- Clipboard write with `x-kde-passwordManagerHint` + 30 s clear-if-ours
- Freedesktop notify on unlock-request (optional; agent events already
  fire)

**KDE plasmoid does not own:** login/register, vault CRUD, settings,
quitting plasmashell, Fluent, COSMIC panel metadata. Opening the vault
window is spawning the existing libcosmic binary, not a Qt port.

Rejected thinner alternative: a StatusNotifierItem-only tray **that
shells the CLI**. Same secret and Subscribe failures, plus no Plasma
panel popup.

The shipped `cosmarden-tray` is not that: it links the UI crate and
speaks postcard over the socket in-process (`AgentClient` for RPC, a
dedicated `Subscribe` stream), so it keeps `Secret` zeroization and live
lock state, and its left-click popup reuses `view/applet/` widgets
including `secure_input`.

## Environment note

This analysis machine has `/usr/bin/plasmawindowed`, `/usr/bin/plasmashell`,
a running `plasmashell` / `startplasma-way`, and `XDG_CURRENT_DESKTOP=KDE`.
No plasmoid was launched: implementing or scaffolding one is a non-goal.
Claims above are from shipped COSMIC source plus the Plasma 6 widget
docs cited, not from a live widget screenshot.
