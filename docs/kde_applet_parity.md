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

The COSMIC applet and the vault window stay libcosmic; the Plasma tray is
**`cosmarden-plasma-tray`**, a separate crate whose only surface is one menu and
which links no libcosmic at all ([Thin Plasma
tray](#thin-plasma-tray-cosmarden-plasma-tray)). An earlier libcosmic KDE tray
(`cosmarden-tray`, StatusNotifierItem + compact popup reusing `view/applet/`)
was **retired** in its favour; nothing in this document describes it any more
except where a fact about it explains a leftover decision.

Two things are gated on **`--features kde`** (UI crate, not default, not
auto-detected): the vault window's log-noise demotion (`src/kde.rs`) and — as a
non-default workspace member — the tray crate. `just build` / `just test-unit`
stay COSMIC; `just build-kde` produces the applet with the filter compiled in
plus `cosmarden-plasma-tray`. `just test-unit-kde` runs the gated tests.

`Open Vault` in the tray spawns **`cosmarden-applet`** in application mode, the
same binary COSMIC opens, resolved as a sibling of the running tray binary — a
tray must never re-spawn itself.

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
`RunMode::{Applet, Application}` — `COSMIC_PANEL_NAME` set means applet,
anything else is the vault window. One binary, `cosmarden-applet`, covers
both. Popup content is `applet_popup_content`; the panel button is
`applet_view`.

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
| Open Vault Window | `Message::OpenVaultRequested`. Destructive styling when `sync_failed && unlocked`. The Plasma tray's own `Open Vault` row is the same hand-off, without the popup. | Closes the popup, then: with a token channel (COSMIC panel) requests an activation token with `exec: "open-vault"` (`update/activation.rs` `classify_activation` → `SpawnVault`); without one (a directly launched applet) resolves the vault binary itself. Both spawn `cosmarden-applet` with `COSMARDEN_MODE=application` and `COSMIC_PANEL_NAME` removed, through the builder the tray shares (`cosmarden_core::vault_binary`). |
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
| 📂 | `AppletOpenInVault` | `Action::SetPendingEntry { id }` then the Open Vault Window path, so the spawned window selects that entry. Agent delivers `Event::OpenEntry` to a Subscribe client (the vault window). Available in the Plasma tray's popup too, which spawns the app the same way. |
| 🔗 | `AppletOpenLink` when the row carries a link, i.e. `domain::open_uri_for_name(name)` is `Some` (URI-shaped name, `http`/`https` scheme only) | none. Detached `xdg-open` (`activation::open_link`) |
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

## Tray menus cannot host input fields

A tray **menu** is not a widget canvas: the host draws it. The application
publishes a menu tree over `com.canonical.dbusmenu`, the tray host reads it and
renders the items itself, and clicks come back as `activateMenuEntry`.

- The protocol defines two item types, `standard` and `separator`, plus
  convention for vendor types (`x-<vendor>-`). Item properties are `label`,
  `enabled`, `visible`, `icon-name`/`icon-data`, `shortcut`, `toggle-type`,
  `toggle-state`, `children-display`, `disposition`
  ([libdbusmenu `dbus-menu.xml`](https://cgit.arctica-project.org/libdbusmenu/plain/libdbusmenu-glib/dbus-menu.xml)).
  No property is a text entry; `label` is display text with access-key
  handling, not an input.
- This crate's tray library agrees: `ksni` 0.3.6's menu model is
  `MenuItem::{Standard, Separator, Checkmark, SubMenu, RadioGroup}` and its
  wire `ItemType` serializes `"standard"` or `"separator"` only
  (`~/.cargo/registry/src/*/ksni-0.3.6/src/menu.rs`).
- KDE states the consequence for widget toolkits outright: in
  `KStatusNotifierItem`, "the actual representation is done by the systemtray
  (or the app behaving like it) itself, not by this app. Since 4.5 this also
  includes the menu, which means **you cannot use embed widgets in the menu**"
  ([kstatusnotifieritem.h](https://invent.kde.org/frameworks/kstatusnotifieritem/-/blame/80331812322720190141ee686bad805b757d78c4/src/kstatusnotifieritem.h)).
  A Qt `QWidgetAction` in a tray `QMenu` renders as an empty row for the same
  reason: the widget cannot cross the bus
  ([SO 34370904](https://stackoverflow.com/questions/34370904/qwidgetaction-doesnt-work-inside-qmenu-as-a-context-menu-for-tray)).
- Ubuntu's indicator stack prototyped a DBusMenu text item and still had to
  request masked entry — "hidden characters for things like passwords" — as a
  missing feature (IDO bug
  [#781924](https://bugs.launchpad.net/ido/+bug/781924)); it never became part
  of the protocol.

So the answer for this project is: **input lives in a real window, not in the
menu.** The retired libcosmic tray opened a compact popup that reused
`view/applet/` and its `secure_input` widgets; the shipped
`cosmarden-plasma-tray` has no popup at all and hands the whole job to
`cosmarden-applet` (`Open Vault`), which is a libcosmic window with every form
the app needs. A Plasma-native plasmoid could instead put
`PlasmaComponents.TextField` (`echoMode: TextInput.Password`) in its
`fullRepresentation`, because that is a QML widget in an ordinary popup window —
but a plasmoid does not share the applet *process* or its `AgentClient` (see
[COSMIC-only surfaces](#cosmic-only-surfaces-do-not-reimplement) and the plasmoid
classification above). The same limit is why the thin tray holds no secret at
all: menu rows are labels and URIs, and anything that needs typing is one
`Open Vault` away.

## Thin Plasma tray (`cosmarden-plasma-tray`)

Crate `crates/cosmarden-plasma-tray`, binary `cosmarden-plasma-tray`. Built by
`just build-kde`, tested by `just test-unit-kde`. It links **no libcosmic**: the
dependency list is `cosmarden-core` (protocol, `AgentClient`, `EventStream`,
`domain`, `vault_binary`), `ksni`, `tokio`, `postcard`, `png`, `anyhow`,
`tracing`, `arboard`, `zbus`, `zeroize`. Size is the point: measured on this
machine's release builds, `cosmarden-plasma-tray` 3.5 MB against
`cosmarden-applet` ~16 MB, with the (retired) libcosmic tray at 16.9 MB and the
CLI (core only) at 1.1 MB. Copying a secret cost the tray 0.1 MB: most of what
remains is `cosmarden-core`'s API client and crypto, which it links but never
calls.

### Why this shape

**Why a separate crate, not a feature of `cosmarden-ui`.** The requirement is
"no dependency on cosmic at all". A Cargo feature inside the UI crate cannot
deliver that: the binary would still link libcosmic, because the crate *is* the
libcosmic app (window, iced runtime, wgpu backend). A crate that never names
libcosmic is the only way the property can be checked — and it is checkable by
reading one `Cargo.toml`.

**Why a menu is the whole surface.** A tray menu is drawn by the host from a
`com.canonical.dbusmenu` tree, and that protocol has no text-input item, so no
menu can hold a password field ([Tray menus cannot host input
fields](#tray-menus-cannot-host-input-fields)). A tray that must not open a
window therefore cannot unlock anything: every design that does unlock is a
window with input, which is either libcosmic (excluded) or a plasmoid (a
different process model, and rejected above). So the thin tray *hands off*:
`Open Vault` starts `cosmarden-applet`, and that is the only unlock path.

**Why left click opens the menu.** With no popup there is nothing else a click
could do, so the item sets `ItemIsMenu` (`ksni`'s `MENU_ON_ACTIVATE`), which
makes Plasma ≥ 6.4 skip `Activate` entirely. Hosts that still call `Activate`
(GNOME's appindicator extension, Plasma < 6.4) get ksni's `UnknownMethod("
ItemIsMenu")` answer and show the menu themselves. One menu, both buttons, no
window.

**What it keeps.** `Open Vault` (hand-off); per pinned login a submenu with
`Copy password` first, then `Copy login` when a username is stored, then — when
the name is URI-like — `Open link` (the same `domain::open_uri_for_name`
decision the applet's 🔗 uses, `http`/`https` only); `Lock`,
`Logout` (two agent actions); `Exit`. Rows come from `GetSidebarEntries` with
`only_pinned: true` — favourites, capped at 10, matching the applet popup's
default view: a tray menu is a shortcut list, not a vault browser.

**What it drops, deliberately.** Generate (a generated password is useless
without a clipboard *and* a place to store it — the app owns both), the
generator pane, search, vault CRUD and settings. Notes and SSH keys are not
listed: their only menu action would be copying a secret blob whose type means
nothing in a menu.

**Copying, and the discipline around it.** `Copy login` reads the username out
of the sidebar snapshot (no round-trip); `Copy password` is one
`GetPassword { id, password: None }` whose `Response::Password` goes straight to
the clipboard. That makes `clipboard.rs` the crate's only plaintext boundary:
`Secret::expose()` is called there and nowhere else, the value is never logged
(the log subject is the row's *label*), it is zeroized when the write returns,
and the selection is cleared after `cosmarden_core::CLIPBOARD_CLEAR_SECS` if it
still holds our value — the same constant and the same
[P1-9] policy the UI uses, so the two clients clear at the same moment. arboard
gives one API for Wayland and X11 (`default-features = false`: text only, no
image support), and the handle lives for the whole process because a Wayland
selection is served by its owner.

A reprompt-gated entry cannot be copied here: `GetPassword` without a master
password answers `reprompt_required`, and a menu has no field to ask in. That
path logs a warning naming the entry and points at `Open Vault` — the app is the
only surface that can serve it.

**State, without polling the world.** `GetConfig` gives `has_account`,
`is_locked`, `needs_login`; `EventStream` (a dedicated `Subscribe` socket) gives
`Locked`/`Unlocked`/`VaultChanged`; a 30 s re-probe covers an agent that starts
after the tray. A protocol-mismatch `Version` collapses the menu to `Exit`
alone. Transitions are logged at `info`/`warn` (agent reachable/not, locked/not,
protocol mismatch, row count); per-poll failures stay at `debug` so a stopped
agent cannot fill the journal.

### Wiring

| Piece | Code | Notes |
|---|---|---|
| SNI item | `tray.rs` `PlasmaTray` (ksni), `MENU_ON_ACTIVATE = true` | id `cosmarden-plasma-tray` (the retired libcosmic tray used the app id, so both could not have run at once without one host grouping hiding the other). |
| Menu | `menu.rs` `build_menu(&TrayState)`, pure | `MenuNode::{Item, SubMenu, Separator}`: one submenu per login (`Copy password`, `Copy login`, `Open link`), a flat `"label — action"` item when a row has only one action. `agent_action()` is the pure command→`Action` map (built outside the async dispatcher, per `docs/testing.md`). |
| Clipboard | `clipboard.rs` (arboard, text only) | `Secret::expose()` at the write, nothing logged, clear-after-`CLIPBOARD_CLEAR_SECS`-if-still-ours. Not unit-tested against a real selection — a test must never touch the developer's clipboard; the policy is. |
| State | `state.rs` `probe(&AgentClient)`, `login_rows(&[SidebarEntry])` | Agent reads: `Version`, `GetConfig`, `GetSidebarEntries` for the menu, plus `GetPassword` on an explicit `Copy password` click. A row carries the username the sidebar already returned (so `Copy login` is free) and never a password. |
| Events | `cosmarden_core::agent_client::EventStream` | Core owns the Subscribe framing (`Ack` then events, capped frame length); the tray reconnects on EOF with a 5 s backoff. |
| Icon | `icon.rs` + `theme.rs` | The four embedded PNGs (`icons/`), ARGB32 like ksni wants. Panel darkness comes from the `org.freedesktop.appearance` portal when Plasma answers, else `kdeglobals`, and `theme::watch_panel_dark` follows the portal's `color-scheme` signal so a live theme switch swaps the glyph. |
| Hand-offs | `launch.rs` | `vault_binary::vault_command` (shared with the applet path) + a reaper thread per child, since the double fork `cosmic::process::spawn` provides is not available here. |
| Autostart | `resources/com.enikeev.cosmarden-tray.desktop` | `just install` writes it to `~/.config/autostart/` with `Exec=` rewritten to the absolute `~/.local/bin` path, only from a `build-kde` build. The packaged path is `/etc/xdg/autostart/`: the release workflow builds the tray and the deb ships both the binary and that entry. `packaging/PKGBUILD` still carries only agent/applet/CLI, so an AUR install has no tray. Plasma 6's `systemd-xdg-autostart-generator` turns it into `app-<name>@autostart.service` (`After=`/`PartOf=graphical-session.target`), and `OnlyShowIn=KDE;` becomes that unit's `ExecCondition` — checked against `XDG_CURRENT_DESKTOP`. Do **not** copy KDE's own `X-systemd-skip=true`: that key means "generate no unit", because plasma-* units start those components. Nothing else starts the tray at login, and nothing needs to: the item registers with `org.kde.StatusNotifierWatcher` (kded6) whenever the process runs. |
| Localisation | none | Labels are English literals (`menu.rs`): the Fluent bundle lives in `cosmarden-ui`, and `en` is the only shipped locale. |

### What retiring the libcosmic tray removed

The two trays overlapped for one release. The retired one was `cosmarden-tray`,
and removing it deleted `kde/{tray,menu,icon,theme}.rs`, `src/bin/cosmarden-tray.rs`,
the `kde` feature's tray plumbing (`kde::subscription`, `publish_snapshot`,
`handle_tray_command`), `Message::KdeTray`, `RunMode::Tray` with the
`COSMARDEN_MODE=tray` branch and its popup-window paths in `runtime.rs`,
`app/update/applet.rs` (the `QuitPath` split) and `app/update/lifecycle.rs`, plus
the `ksni`/`png`/`futures-util` dependencies of `cosmarden-ui`. `--features kde`
now only compiles `src/kde.rs` (log-noise demotion). It did **not** touch
`view/applet/`: the COSMIC applet still uses it, and the shared pieces the tray
needed moved into `cosmarden-core` (`domain` URI/label helpers, `EventStream`,
`vault_binary`) instead of being duplicated.

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
| 16 | Open URI via xdg-open | **Shipped** for the tray | Menu row `Open link` on every login row whose name is URI-like (`http`/`https` only) → `Message::AppletOpenLink` → `activation::open_link` (detached double fork; `Command::spawn` with a dropped `Child` is the zombie COSMIC already hit). A plasmoid would use `QDesktopServices::openUrl` with the same no-dropped-`Child` rule. |
| 17 | Protocol mismatch surface | **Possible** | `Action::Version` vs `PROTOCOL_VERSION`. |
| 18 | Not-configured caption (no login form) | **Possible** | Same as COSMIC applet: tell the user; login is not a popup job. |
| 19 | Open Vault Window / 📂 open-in-vault | **Shipped** for the tray (libcosmic window on Plasma) | The tray spawns the same `cosmarden-applet` binary COSMIC spawns, in application mode; the vault UI then runs on Plasma with libcosmic's theme-connection `ERROR`s demoted under `--features kde`. It resolves the binary as its own sibling (or `COSMARDEN_APPLET_BIN`/`PATH`), because a tray that re-spawned itself would start a second tray. `SetPendingEntry` / `Event::OpenEntry` still work: the spawned window subscribes and opens the targeted entry. A plasmoid could do the same, but not share the applet *process*. `OnlyShowIn=COSMIC;` still hides the applet's `.desktop` from KDE app launchers — a KDE `.desktop` belongs behind `--features kde`, not in the default install. |
| 20 | Activation-token `exec` (`open-vault`, `activate:lock`, `activate:generate`) | **Impossible** (COSMIC panel API) | `cosmic::applet::token::subscription`. Plasma has XDG activation for launching windows, but not this exec-string router, and there is no same-binary vault to spawn. |
| 21 | Quit tearing down the **UI process** | **Partial** | The plasmoid lives **inside plasmashell**. It cannot exit a panel process the way `Message::Exit` closes `cosmarden-applet` (or `cosmarden-plasma-tray`, which is its own process and does exit). Map Lock/Logout to IPC; map "just quit" to collapsing the popup (`plasmoid.expanded = false`). Do not send `Action::Quit` (that kills the agent). |
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

The shipped `cosmarden-plasma-tray` is not that: it links
`cosmarden-core` and speaks postcard over the socket in-process
(`AgentClient` for RPC, `EventStream` for live lock state), with no
libcosmic and no CLI in the loop. It renders no vault UI: it holds no
plaintext at all, and anything that needs input is the app it spawns.

## Environment note

This analysis machine has `/usr/bin/plasmawindowed`, `/usr/bin/plasmashell`,
a running `plasmashell` / `startplasma-way`, and `XDG_CURRENT_DESKTOP=KDE`.
No plasmoid was launched: implementing or scaffolding one is a non-goal.
Claims above are from shipped COSMIC source plus the Plasma 6 widget
docs cited, not from a live widget screenshot.
