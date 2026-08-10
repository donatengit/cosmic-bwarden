# Applet implementation comparison: libcosmic reference vs cosmic-bwarden

Status: research notes, 2026. Purpose: compare how COSMIC panel applets are
implemented in the reference material under `tmp_code_examples/` with the
applet in this repo, and collect spots for a later security review.

Reference trees used. These are **local read-only checkouts**, deliberately not part of
this repo (Phase 0 removed the reference submodules — see
[review/00_ground_truth.md](review/00_ground_truth.md)); clone them yourself if you want
to follow the line references:

- `tmp_code_examples/libcosmic` — the libcosmic framework itself, including the
  canonical example applet (`examples/applet/`).
- `tmp_code_examples/cosmic-examples/cosmic-panel` (referenced by the survey) —
  how the panel spawns/registers applets.
- `tmp_code_examples/bitwarden-official-clients` — not an applet source; kept
  out of scope except where its patterns (e.g. secrets-handling discipline)
  inform review spots.

This repo's applet lives in `crates/cosmic-bwarden-ui`; the agent it talks to
is `crates/cosmic-bwarden-agent` / `cosmic-bwarden-core`.

## 1. How a reference applet is implemented (libcosmic)

Minimal shape (`tmp_code_examples/libcosmic/examples/applet/src/`):

- Entry is `cosmic::applet::run::<Window>(())` (`main.rs:11`).
- `window.rs` implements `cosmic::Application` with `type Executor =
  SingleThreadExecutor`, `const APP_ID`, `core()/core_mut()`, `init()`,
  `update()`, `view()`, `view_window(id)`, and `style() ->
  Some(cosmic::applet::style())` (`window.rs:42-164`).
- `cosmic::applet::run` (`src/applet/mod.rs:536-593`) forces the iced daemon
  into applet shape: `AppType::Applet`, no decorations, non-resizable,
  `exit_on_close_request = true`, `sharp_corners = true`.
- Popup: the icon button uses `on_press_with_rectangle`, and the closure
  builds `Message::Surface(app_popup::<Window>(...))` directly — the settings
  closure computes `anchor_rect` from `bounds - offset` and stores the new
  window `Id` in state; the view closure renders
  `core.applet.popup_container(content).map(cosmic::Action::App)`
  (`window.rs:99-147`). `on_close_requested` maps to `PopupClosed(id)`
  (`window.rs:64-66`).
- Framework behavior the applet gets for free:
  - `get_popup_settings` (`src/applet/mod.rs:406-457`): 4 px pixel offset,
    anchor/gravity from `COSMIC_PANEL_ANCHOR`, `reactive: true`, width capped
    360 px, **`grab: true`** (Wayland closes the popup on outside click).
  - `popup_container` (`mod.rs:359-402`): autosize + themed card, 360×1000 cap.
  - `applet_tooltip` (`mod.rs:293-356`): separate tooltip surface whose
    `input_zone` is a zero-size rect at (−1000,−1000) with `grab: false` — the
    tooltip can never steal pointer input.
  - `icon_button` / `icon_button_from_handle` (`mod.rs:280`, `mod.rs:209`).

Registration is panel-driven, not token-based:

- The panel reads the applet's `.desktop` file (`X-CosmicApplet=true`,
  `Exec`, `X-CosmicHoverPopup`, `X-OverflowPriority`, …), shlex-parses `Exec`,
  and spawns the applet with `COSMIC_PANEL_NAME/OUTPUT/SPACING/ANCHOR/
  BACKGROUND/PADDING_OVERLAP` env, a dedicated Wayland socket fd, and
  optionally `X_PRIVILEGED_WAYLAND_SOCKET` from a compositor security context
  (`cosmic-panel wrapper_space.rs`).
- The applet reads `COSMIC_PANEL_*` in `Context::default()`
  (`src/applet/mod.rs:89-119`); there is no `COSMIC_PANEL_APPLET` token —
  placement is configured in COSMIC Settings, and the applet window is just a
  window on the socket the panel gave it.
- The activation-token subscription (`src/applet/token/`) is **not** a layout
  handshake: it is used to launch other apps with a compositor-issued
  `xdg-activation` token (e.g. the power applet launching `cosmic-settings`).
  The token thread connects over the privileged socket the panel granted, so
  an applet can't grab arbitrary compositor sockets.

There is no search UI in the reference applet framework; search lives in
cosmic-settings' panel-applets page.

## 2. How cosmic-bwarden's applet is implemented

- **One binary, two modes.** `cosmic-applet-bwarden` (`Cargo.toml:10`, the `[[bin]]` name);
  `detect_run_mode()` keys on `COSMIC_PANEL_NAME` (`main.rs:41-49`), then
  `run_applet` → `cosmic::applet::run::<CosmicBWardenApp>` vs
  `run_application` → `cosmic::app::run_single_instance` (`main.rs:328-356`).
  Same `Application` impl: `view()` renders only `applet_view()` in applet
  mode (`main.rs:191-238`), `style()` is `cosmic::applet::style()`
  (`main.rs:306-313`), and the activation-token subscription is armed only in
  applet mode (`main.rs:297-301`).
- **Registration:** desktop file `X-CosmicApplet=true`, `X-CosmicHoverPopup=Auto`
  (`resources/com.enikeev.cosmic_bwarden.desktop`); applet metadata `.ron`
  written by the justfile (`justfile:50-52`) and shipped via deb/PKGBUILD
  (`packaging/`). No row/column config — left to COSMIC Settings
  (`docs/cosmic_integration.md:99-100`).
- **Icon:** embedded symbolic SVG (`view/applet/mod.rs:24-29`) instead of a
  theme lookup — deliberate, so dev builds render without an install step.
- **Popup:** `applet_view` (`view/applet/mod.rs:28-45`) uses
  `icon_button_from_handle(...).on_press_with_rectangle(|offset, bounds|
  Message::AppletIconClicked(offset, bounds))`. The `Message` carries the raw
  coordinates; `app/update/applet.rs:35-50` toggles/destroys the existing
  popup, else `open_applet_popup_task` (`applet.rs:523-588`) opens
  `app_popup` with the same `anchor_rect = bounds - offset` math
  (`applet.rs:574-580`), plus a protocol-version check, `GetConfig` refresh,
  and input focus. Popup content is routed via `WindowState::Popup`
  (`view/mod.rs:103-124`).
- **Popup content** (`view/applet/mod.rs:47-89`): header row (menu.rs), then
  per-`View`: Vault/Settings → search, Setup → "not configured", anything else
  → inline unlock; error line; quit footer; all inside `popup_container` +
  toaster.
- **Search:** applet-only feature (no reference equivalent). Query goes to the
  agent (`GetSidebarEntries`), which substring-matches over a **decrypted
  in-memory sidebar cache** (`agent/src/handler/vault/query.rs:153-165`,
  `agent/src/state.rs:26-28`). Empty query ⇒ favourites only
  (`app/applet_search.rs:24-26`). Card/Identity rows dropped, capped at 10
  (`applet_search.rs:64-96`). Reprompt-protected entries swap the row for an
  inline `secure_input` (`view/applet/search.rs:158-187`).
- **Unlock/PIN:** inline `secure_input`s in the popup
  (`view/applet/unlock.rs`); actions built exclusively in the shared
  `auth_actions` builders; PIN length gated by `MIN_PIN_LEN` client-side
  (`applet.rs:140-143`), authoritatively in the agent. Master-password unlock
  can also (re-)seal/clear the TPM PIN (`apply_unlock_pin_task`).
- **Generator:** unconditional quick-gen button in the header
  (`view/applet/menu.rs:59-67`) → `GeneratePassword { settings: None }`
  (reuses last-saved settings), works locked/no-account; result goes to the
  30 s auto-clear clipboard.
- **Open vault window:** requests an activation token
  (`TokenRequest { app_id, exec: "open-vault" }`), then spawns the same
  binary as a second process with `COSMIC_BWARDEN_MODE=application`,
  `COSMIC_PANEL_NAME` removed, and `XDG_ACTIVATION_TOKEN`/
  `DESKTOP_STARTUP_ID` set (`applet.rs:107-132`) — the reference power-applet
  pattern. Deep-link handoff via `SetPendingEntry` → agent → `Event::OpenEntry`
  → next subscriber (`applet.rs:248-254`, `agent/src/lib.rs:314-321`).
- **Protocol mismatch:** checked at startup and on every popup open; the popup
  then shows only the localized error + Quit (`view/applet/mod.rs:49-64`).

## 3. Comparison — alignment and divergences

| Aspect | Reference (libcosmic example) | cosmic-bwarden | Verdict |
|---|---|---|---|
| Entry | `cosmic::applet::run` | same | aligned |
| Application shape | minimal `Application` impl + `applet::style()` | same, mode-aware | aligned |
| Popup anchor math | inline in `on_press_with_rectangle` closure | coordinates travel through `Message::AppletIconClicked`; same math in `open_applet_popup_task` | deliberate divergence — testability, matches repo's "pure builder" rule; watch that raw screen coords in a `Message` stay a UI-internal concern |
| Popup id tracking | state field + `PopupClosed` | state field + `WindowState::Popup` map + stale-popup guard | cosmic-bwarden is more defensive (guards double-click/race) |
| Tooltip / popup surfaces | framework-provided (input-zone quarantine, grab) | same framework calls | aligned — no custom surface code |
| Registration | panel-driven via `.desktop` + `COSMIC_PANEL_*` env | same; plus `.ron` metadata written by justfile/deb/PKGBUILD | aligned; no token for placement anywhere |
| Activation token | used to launch other apps | used to launch the vault window as a separate process | aligned |
| Icon | theme lookup (`icon_button`) | embedded symbolic SVG | justified divergence (dev-build rendering); `symbolic(true)` keeps theming |
| Search | none in framework | agent-side substring search, favourites-only default | feature beyond reference; secrets correctly excluded from results |
| Unlock/PIN in popup | n/a (no auth concept) | inline secure_inputs + zeroize discipline | feature beyond reference |
| Hover popup | `X-CosmicHoverPopup=Start` (battery) | `Auto` | config choice, both valid |

Non-issues confirmed while comparing:

- `View::PasswordGenerator` is only set in `app/update/pwgen.rs` (main-window
  pane); the applet popup's `match` (`view/applet/mod.rs:69-73`) treats
  non-Vault/Settings/Setup views as "locked" — correct today, but the arm is
  implicit (`_`); a comment or explicit `View::Locked` arm would make the
  intent clear if the applet ever grows a view that isn't Vault/Settings.
- Clipboard copies (secret, generated password) all go through the shared
  30 s auto-clear path (`app/update/mod.rs:282-297`).

## 4. Spots for later security review

Severity is a review-priority guess, not a confirmed vulnerability. All paths
repo-relative.

1. **`xdg-open` with a vault-derived URI** — `crates/cosmic-bwarden-ui/src/app/update/applet.rs:255-258`
   spawns `xdg-open <uri>` where `uri` comes from a login entry's name field
   (vault data → external program). The gate `is_uri_like`
   (`app/applet_search.rs:52-59`) is conservative: it strips only
   `https://`/`http://`, then requires the part before the first `/` to
   contain a `.` and no space/`@`, which effectively rejects non-http(s)
   schemes (`ftp://…`, `file://…`, `javascript://…` all fail the dot check on
   the `scheme:` token). `Command::arg` avoids shell injection. Review
   points: the heuristic is the only guard between an attacker-controlled
   name and the user's default URI handler; the `let _ = spawn` swallows
   spawn failures; if link handling ever widens (custom schemes, `ssh://`),
   re-audit this gate.
2. **Decrypted usernames cached in the popup process** —
   `app/state.rs:127-130` `applet_search_results: Vec<SidebarEntry>` holds
   plaintext usernames (agent-decrypted, `query.rs:153-165`) in the applet
   process while unlocked, and `AppletCopyPrimary` copies straight from that
   cache (`applet.rs:239-246`) without a fresh agent round-trip. By design
   (the applet is the same trusted client as the main window; secrets are
   excluded from `SidebarEntry` via `redact_entry_secrets`), but note: the
   cache is not `zeroize`d on popup close — it is overwritten by the next
   fetch, leaving freed copies in RAM. Consistent with the vault sidebar's
   model; worth a deliberate decision.
3. **`SetPendingEntry` failure is silently discarded** —
   `applet.rs:248-254` (`let _ = agent.send(...)`). If it fails, the opened
   vault window just doesn't preselect the entry — benign today, but it is a
   user-visible silent failure; log at `warn` or handle the error.
4. **PIN/password handling in the popup** — `applet_pin`/`applet_unlock_password`/
   `applet_reprompt_password`/`unlock_pin` are plain `String`s, zeroized on
   submit, on success, and when the popup opens (`applet.rs:158, 266, 364,
   527-534`; `auth.rs:290-328` incl. the no-TPM "Nothing" arm). Consistent
   with the main window; the agent remains authoritative on `MIN_PIN_LEN`
   and TPM DA. Review only if the popup lifecycle changes (e.g. popup kept
   alive after unlock).
5. **Agent-side plaintext search cache (not applet-specific)** —
   `agent/src/state.rs:26-28` + `query.rs:153-165` keep decrypted
   names/usernames/hosts in unlocked memory and substring-match queries
   against them. RAM-hygiene consideration (mlock scope), not an applet bug.
6. **Already-handled hygiene to re-verify during review:**
   - Redacting `Debug` impls for `Action`/`Response`
     (`core/src/protocol/debug_impls.rs:13-38`); `Response::GeneratedPassword`
     never logged verbatim (`protocol.rs:450`).
   - Socket `0600` (`agent/src/lib.rs:151-153`), `SO_PEERCRED` UID rejection
     (`lib.rs:198-220`), 8 MiB request cap (`lib.rs:236-242`) — applet and
     window are equally trusted clients.
   - Reprompt gate: `GetPassword { id, password: None }` triggers the
     agent-side reprompt check; the user's master password is only sent as
     `Some` after re-entry (`tasks.rs:113-128`, `query.rs:39-51`).

## 5. Non-security findings

All three findings below were fixed in the same session (2026); the notes
record what changed.

- **i18n violations in the applet menu (was review-blocking per AGENTS.md):**
  `view/applet/menu.rs:35-47` hardcoded two user-facing tooltip strings —
  `"Session expired — click to log in again"` and `"Not synced — click to
  retry"` — as bare literals passed to `text::caption`. **Fixed:** both now go
  through `fl!` via new keys `sync-session-expired-tooltip` and
  `sync-not-synced-tooltip` (`i18n/en/cosmic_bwarden_ui.ftl`). Dedicated keys
  were chosen over reusing the existing `session-expired`/`not-synced` keys
  because the sidebar (view/vault/sidebar.rs:141,147) uses those for status
  labels while the applet tooltips describe the click action.
- `view/applet/menu.rs:102-105` composed `format!("▾ {}"/"▸ {}", fl!("quit"))`
  — glyph + whole-key composition; glyphs are exempt from localization, so it
  was acceptable, but it still built a display string with `format!`.
  **Fixed:** now `fl!("quit-menu-expanded"|"quit-menu-collapsed",
  label = fl!("quit"))`, with the glyph as a Fluent placeable so translators
  can position it.
- `Menu`'s `is_unlocked` predicate (`menu.rs:10-13`, `96-99`) included
  `View::PasswordGenerator`, which the applet process can never reach (set
  only in `update/pwgen.rs`); the popup `match` in `view/applet/mod.rs:69-73`
  did not — the two views of "unlocked" were maintained independently.
  **Fixed:** single source of truth `View::is_unlocked()`
  (`message.rs`, unit-tested), used by both the applet menu and the popup
  content routing. Behavior is unchanged today (`PasswordGenerator` is still
  unreachable from the applet); the divergence is gone by construction.
