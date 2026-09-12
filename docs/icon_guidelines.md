# Icons: emoji inventory and CosmicDE construction

Roadmap item `[U4-4]` is done. This document records which emoji used to
stand in for icons, how official CosmicDE symbolics are made and look,
and the theme names now wired in
`crates/cosmic-bwarden-ui/src/view/symbolic.rs`.

Panel/brand artwork and install paths stay in
[`docs/cosmic_integration.md`](cosmic_integration.md).

## 1. Emoji that were used as icons (replaced)

These glyphs *were* the control's icon. They are gone from production UI;
the Cosmic name in the last column is what `view/symbolic.rs` returns
today. Login rows still have three action buttons; note and SSH rows omit
the URI button.

| Glyph | Was | Now |
|---|---|---|
| 📂 | `view/applet/search.rs` `login_row_view` / `secret_row_view` — `AppletOpenInVault` | `preferences-workspaces-symbolic` + tooltip `open-in-vault` |
| 🔗 | `login_row_view` — `AppletOpenLink` (`on_press_maybe` when `is_uri_like`) | `window-pop-out-symbolic` + tooltip `open-uri` |
| 🔑 | both row kinds — `AppletCopySecret` | `network-vpn-symbolic` + tooltip `copy-secret` |
| ✅ | `view/settings.rs` `tpm_settings_section` pass prefix | `object-select-symbolic` |
| ❌ | same, fail prefix | `process-stop-symbolic` |

The browser-extension popup, detail, and edit surfaces never used emoji
as button icons (`popup/popup.html`, `popup/icons.js`).

## 2. Glyphs that sat next to a real icon (also removed)

| Glyph | Was | Now |
|---|---|---|
| ⚠ | FTL `not-synced` tooltip prefix on the vault-sidebar sync button (`network-error-symbolic`) | `not-synced = Not synced` — the button is the icon |
| ★ | Extension caption `"★ Favourites"` in `popup/popup.js` | Caption is `Favourites`; the fav *button* is already an SVG star |

## 3. Classified out (not icons)

Hits from a Unicode pass over `crates/cosmic-bwarden-ui` and
`browser-extension/` (tests excluded from the "icon" list, not from this
classification).

**Comments in `view/symbolic.rs`** that name the old glyph next to the
helper that replaced it. Not UI.

**Exempt text glyphs** (AGENTS.md: not localized, not icons)

- `—` (U+2014), `…` (U+2026) in FTL and views.

**Comments / docs using `→` (U+2192)** as ASCII-arrow substitute in
Rust/JS comments. Not UI.

**Markdown checkmarks and section markers in `docs/` / `CONTEXT.md`**
(`✅` / `❌` in review notes, `docs/ui_test_coverage.md`,
`docs/ssh-agent.md` tables; 🚀 / 🛡️ / 🔄 heading markers in
`CONTEXT.md`). Not UI chrome.

**Vault / test *data*** (entry names and note bodies, never chrome)

- `tests/browser-extension/playwright/chrome-full.spec.js` (`🔐`, `💳`,
  `🌍`, `📝`, `🔑`, `🖊️` in created entry names)
- `crates/cosmic-bwarden-tests/src/notes_stdin_cli.rs` (`🔑`, `🚀` in a
  stdin note body)
- `crates/cosmic-bwarden-core/src/tests.rs` (`🦀`, `🚀`, `🔐` as test
  strings)
- `crates/cosmic-bwarden-tests/src/cli_secret_mask_test.rs` (numbered
  emoji in comments only)

## 4. CosmicDE symbolics: how they are made

There is no published construction spec in `pop-os/cosmic-icons` (license
text only). The rules below are taken from the SVGs themselves.

**Sources viewed**

- Official applet symbolics in
  `tmp_code_examples/cosmic_examples/cosmic-applets/` (63 SVGs; 54 named
  `*-symbolic.svg`). Named examples:
  `cosmic-applet-network/.../com.system76.CosmicAppletNetwork-symbolic.svg`,
  `cosmic-applet-battery/.../com.system76.CosmicAppletBattery-symbolic.svg`,
  `cosmic-applet-audio/.../com.system76.CosmicAppletAudio-symbolic.svg`,
  `cosmic-applet-notifications/.../com.system76.CosmicAppletNotifications-symbolic.svg`,
  plus bluetooth / power / battery-level status variants.
- Installed theme: `/usr/share/icons/Cosmic/scalable/{actions,apps,status,places,emblems}/`
  (426 `*-symbolic.svg` files). Named examples: `edit-copy-symbolic`,
  `password-manager-symbolic`, `folder-symbolic`, `insert-link-symbolic`,
  `dialog-password-symbolic`, `checkbox-checked-symbolic`,
  `dialog-error-symbolic`, `object-select-symbolic`,
  `system-lock-screen-symbolic`, `network-error-symbolic`.
- Theme index: `/usr/share/icons/Cosmic/index.theme` (`Name=COSMIC`,
  `Inherits=Pop,hicolor`, `SmallDefault=16`, `scalable/*` is
  `Type=Scalable` with `MinSize=8` `MaxSize=512`).

**Canvas and naming**

- Action / status / applet symbolics are a **16×16** canvas
  (`width="16" height="16" viewBox="0 0 16 16"`). 396 of 426 installed
  symbolics are exactly that. A few applet/network icons are 17×16.
- File name ends in `-symbolic` (Freedesktop symbolic convention).
  Cosmic applet panel marks are
  `com.system76.CosmicApplet<Name>-symbolic.svg`.
- Root element is `fill="none"`; drawable geometry is child `<path>`s.

**Paint: fill, not stroke**

- Geometry is **filled paths**, not outlined strokes. No
  `stroke` / `stroke-width` on the action/status set viewed.
- Recolorable ink is **`fill="#232323"`** (416 of 426 installed
  symbolics; 48 of 63 applet SVGs). That dark grey is the symbolic
  "ink": GTK and libcosmic replace it with the current foreground.
- Secondary geometry of the *same* object uses the same `#232323` at
  **`opacity="0.35"`** (38 applet SVGs; 38 theme symbolics). Examples:
  wifi outer fan in the network applet, left-half battery fill, outer
  speaker wave, charging-battery remaining-charge slab.
- A near-invisible **hit rect** is common: a 16×16 path
  `fill="#808080"` with `fill-opacity="0.00"` or `"0.01"` (184 theme
  symbolics; 12 applet SVGs). It keeps hit-testing and layout on the
  full canvas when the drawable silhouette is smaller.
- Many files wrap paths in a **`clipPath`** that is itself a 16×16
  rect, with Figma-style ids (`clip0_4614_124622`). White `fill` on the
  clip rect is the clip mask, not visible paint.

**Recolor**

- libcosmic: `icon::from_svg_bytes(...).symbolic(true)` (and
  `icon::from_name` on a `-symbolic` theme name) discards the SVG's own
  paint and tints the shape to the panel/window foreground. This repo
  already documents that for the panel button in
  `view/applet/mod.rs`.
- `opacity="0.35"` is preserved, so the secondary layer stays at 35% of
  whatever the recolored ink is. Light and dark themes both work from
  one file.
- Status exceptions keep a **fixed hue** and are *not* meant to go
  monochrome: `dialog-error-symbolic` / `network-error-symbolic` use
  `#F44336`; `dialog-warning-symbolic` uses `#FF9800`. Do not treat
  those fills as the action-icon language.

**Not this language**

Seven applet SVGs are 256×256 full-color app artwork (gradients, extra
hues). Those are launcher/app identities, not 16px action symbolics.

## 5. CosmicDE symbolics: how they look

Viewed at 256px so construction is visible; they are designed to read at
16px.

- **Filled silhouettes**, optically ~12–14px inside the 16px canvas
  (roughly 1–2px padding). Chunkier than a 1.5px outline icon.
- **Geometric**, slightly rounded corners (`r` ≈ 1–2 on battery,
  notification bubble, copy-rect). No drop shadow, no outline, no
  inner highlight.
- **Two-tone only when the object has a "background" part**: solid ink
  plus a 35% slab/arc of the same hue (battery remaining charge,
  wifi outer fan, far speaker wave). Single-object icons
  (`folder-symbolic`, `dialog-password-symbolic`,
  `checkbox-checked-symbolic`, `edit-copy-symbolic`) are one fill.
- **Metaphors in this theme** (the replacement names are in §8; these
  are what the viewed files actually draw):
  - VPN / key: filled key, round bow + hole + one bit
    (`preferences-vpn-symbolic` / `network-vpn-symbolic` — identical)
  - Workspaces: two overlapping rounded window cards
    (`preferences-workspaces-symbolic`)
  - "pop out" / leave this surface: filled arrow up-right
    (`window-pop-out-symbolic`). Not a square+arrow; Cosmic has none.
  - "link" *insert*: a connector/plug (`insert-link-symbolic`) — not
    the URI-open mark
  - secret/password *dialog*: filled keyhole (`dialog-password-symbolic`)
  - password-manager app mark: safe with a three-spoke lock
    (`password-manager-symbolic`)
  - confirm / ok: the same heavy check
    (`checkbox-checked-symbolic`, `object-select-symbolic`,
    `emblem-ok-symbolic`)
  - stop / fail: filled octagon, X cut out (`process-stop-symbolic`)
  - error: red filled circle with a cut-out bang
    (`dialog-error-symbolic`) — colored on purpose
  - warning: orange triangle (`dialog-warning-symbolic`, `#FF9800`)
- **State is silhouette, not color**, on recolorable symbolics. The
  charging-battery variant adds a lightning bolt in the same ink; it
  does not switch to green.

## 6. Contrast with this repo's brand symbolics

`crates/cosmic-bwarden-ui/resources/icons/`:

| File | Canvas | Paint | Role |
|---|---|---|---|
| `cosmic-bwarden-symbolic.svg` | 128×128 `viewBox="0 0 128 128"` | default black fill, no `#232323`, no 0.35 layer, no hit rect | Panel / `.desktop` mark (C-shape + three dots) |
| `cosmic-bwarden-locked-symbolic.svg` | same 128×128 outline, dots removed | same | Locked / logged-out panel mark |
| `cosmic-bwarden-full-symbolic.svg` | 500×500, design-tool export | detailed brand, not a 16px action | Full mark for contexts that do not recolor |

These are **identity** marks, not action icons. They sit on a 128px grid,
carry no Cosmic `#232323` / 0.35 secondary language, and are recolored
as a whole by `symbolic(true)`. Do not reuse them as 📂/🔗/🔑
replacements — at 16px the C-shape is a blob, and the three dots are the
unlocked-state signal, not a "copy secret" affordance.

## 7. Contrast with the extension's 16px stroke set

`browser-extension/popup/popup.html` (static header) and
`browser-extension/popup/icons.js` (dynamic buttons):

- `viewBox="0 0 16 16"`, CSS `.icon { width: 16px; height: 16px; }`
- `fill="none"` + `stroke="currentColor"` + `stroke-width="1.5"` +
  `stroke-linecap="round"` + `stroke-linejoin="round"`
- Color comes from the button's CSS, not from a `#232323` fill

That is an **outline** language. CosmicDE action symbolics are **fill**
language. A later U4-4 replacement in the applet should match Cosmic
(fill, `#232323`, optional 0.35 secondary, `-symbolic` name), not the
extension strokes. The extension set is already consistent with itself
and is out of scope for U4-4.

## 8. Equivalent Cosmic symbolics for each glyph

These names are wired in `crates/cosmic-bwarden-ui/src/view/symbolic.rs`
(the view must call those helpers). They were viewed as SVGs and matched
to how Cosmic Settings / applets already use them.

### Applet search buttons

| Glyph | Use | Cosmic name | What it looks like | Why this name |
|---|---|---|---|---|
| 📂 | Open in vault window | `preferences-workspaces-symbolic` | Two overlapping rounded window cards (front card with a baseline) | Cosmic Settings Workspaces page (`Icon=preferences-workspaces` in `com.system76.CosmicSettings.Workspaces.desktop`). Reads as "open another surface", which is what `AppletOpenInVault` does. The 256×256 applet artwork `com.system76.CosmicAppletWorkspaces.svg` is full-color "12" branding, not a 16px action. `focus-windows-symbolic` is a similar pair of *outlined* frames if a lighter mark is wanted. |
| 🔗 | Open login URI | `window-pop-out-symbolic` | Filled arrow pointing up-right | Cosmic Settings uses this as the trailing icon on actions that leave the current surface (Wi-Fi, Wired, and VPN "Add network"). Cosmic does **not** ship a "square + up-right arrow" (the Adwaita/Material external-link). Closest square is `window-new-symbolic` (window + plus in the corner) — wrong mark. `insert-arrow-symbolic` is the same diagonal without the Cosmic "pop out" name. `insert-link-symbolic` is a connector/plug, not a link-out. |
| 🔑 | Copy secret | `network-vpn-symbolic` (same drawing as `preferences-vpn-symbolic`) | Filled key: round bow with a hole, shaft, one bit | The network applet draws every VPN row with `icon::from_name("network-vpn-symbolic")` (`cosmic-applet-network/src/app.rs`). Cosmic Settings' VPN page icon is `preferences-vpn-symbolic` (`pages/networking/mod.rs`). Both files are the same 16×16 key; the Cosmic theme ships `preferences-vpn-symbolic`, and `network-vpn-symbolic` resolves via `Inherits=Pop`. Prefer `network-vpn-symbolic` to match the applet row size/usage. Do **not** use `dialog-password-symbolic` (keyhole) — that is already this app's session-expired mark. |

### TPM diagnostics

| Glyph | Use | Cosmic name | What it looks like | Why this name |
|---|---|---|---|---|
| ✅ | Diagnostic passed | `object-select-symbolic` | Heavy filled check | Same silhouette as `checkbox-checked-symbolic` and `emblem-ok-symbolic`. This applet already uses `object-select-symbolic` for reprompt confirm (`view/applet/search.rs`). Keep one name. |
| ❌ | Diagnostic failed | `process-stop-symbolic` | Filled octagon with a cut-out X | Cosmic's "this did not succeed / stop" status, monochrome so it recolors. `window-close-symbolic` is the same X without the octagon and already means *dismiss* on the reprompt row — do not reuse it for fail. `dialog-error-symbolic` is a red circle-bang (`#F44336`); use that only if the fail mark should stay colored. |

### Glyphs that sat next to a real icon

| Glyph | Use | What landed |
|---|---|---|
| ⚠ | FTL `not-synced` tooltip prefix | Dropped. Button remains `network-error-symbolic`. Cosmic warning icon, if ever needed: `dialog-warning-symbolic` (`#FF9800`). |
| ★ | Extension caption `"★ Favourites"` | Dropped from the caption. Desktop fav toggle stays `starred-symbolic` / `non-starred-symbolic`. |

### Related text triangles

| Glyph | Use | What landed |
|---|---|---|
| ▾ / ▸ | Quit menu expand/collapse in `quit_footer` | `pan-down-symbolic` / `pan-end-symbolic` via `quit_disclosure_icon`. |

Generator's `insert-drawing-symbolic` placeholder is a separate TODO in `view/applet/menu.rs`, not in this emoji inventory.
