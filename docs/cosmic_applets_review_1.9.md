# Upstream applet review: `pop-os/cosmic-applets` at `epoch-1.9.0`

Reference material for the applet side of `cosmarden-ui`, refreshed for
COSMIC Epoch 1.9. This document records the revisions held locally, what
changed upstream since the 1.8 snapshot, and which upstream practices are
worth copying. The code-level comparison of our applet against the upstream
examples stays in [`applet_reference_comparison.md`](applet_reference_comparison.md);
this file covers revisions, change history, and which of the newer
libcosmic/iced patches matter to us.

The workspace moved to the COSMIC 1.9-era libcosmic revision `03d7dcb8` on
2026-09-26 (commit `576635a`). The 1.8 snapshot is
[`cosmic_applets_review_1.8.md`](cosmic_applets_review_1.8.md); every "our
pin" statement there describes `53314201b` and the applets tag
`epoch-1.8.0`, not the current tree.

All paths below are relative to the repository root unless stated otherwise.
The reference clones live under `tmp_code_examples/`, which is gitignored
(`.gitignore:77`) — nothing here is a build input.

## 1. Local reference checkouts

| Path | Revision | Date | Notes |
|---|---|---|---|
| `tmp_code_examples/cosmic_examples/cosmic-applets` | `82e7cd814addb0641959b634227fe550374e195b` | 2026-09-20 | Branch `epoch-1.9.0`, the tag of the same name. `master` is the same commit. |
| `tmp_code_examples/libcosmic` | `03d7dcb832bc6d15bc1bb07327a1aa426b2d5bc0` | 2026-09-24 | Detached HEAD, matching the revision `Cargo.lock` builds against. |
| `tmp_code_examples/iced` | `b2419520fea14dbaf2759823f2c159acb07de89d` | 2026-09-22 | `pop-os/iced`, the fork libcosmic bundles at `03d7dcb8`. New in this refresh (section 9). Cloned with `--filter=blob:none --no-checkout`; read it with `git show <rev>:<path>`. |
| `tmp_code_examples/cosmic_examples/cosmic-panel` | `b5cc19e351b82eed999097ced1ff267d27bf8e7a` | 2026-09-17 | Detached at `epoch-1.9.0`, also current `master`. The same commit `Cargo.lock` pins for `cosmic-panel-config`. |
| `tmp_code_examples/cosmic_examples/cosmic-settings` | `7421ff0ab482c942eef23fbe9280edacfbeb35cc` | 2026-09-22 | Detached at `epoch-1.9.0`; `master` is two commits further. |
| `tmp_code_examples/cosmic_examples/cosmic-notifications` | `aa4dac2702506395ab76da1f6755e03b2ccb0db8` | 2026-09-15 | Detached at `epoch-1.9.0`; `master` is the same commit. |
| `tmp_code_examples/cosmic_examples/cosmic-ext-applet-external-monitor-brightness` | `b068d5c4c2e604f79d48060cb2a53c926165bf4e` | 2026-09-12 | Current `master`; the clone the popup-reopen debounce is modelled on. |
| `tmp_code_examples/cosmic_examples/cosmic-applet-template` | `fc6b688eae769be2821d23b264ccd711f2abf0a6` | 2026-01-22 | Unchanged since before the 1.8 review. |
| `tmp_code_examples/cosmic_examples/toot` | `1c20d33dae5cac6bd543fe782eec745eb762980e` | 2026-07-14 | Unchanged since before the 1.8 review. |

Refresh procedure:

```sh
cd tmp_code_examples/cosmic_examples/cosmic-applets
git fetch --tags --prune origin
git checkout -B epoch-1.9.0 epoch-1.9.0      # or: git checkout -B master origin/master
```

## 2. What the repository contains

The applet set grew by one: **`cosmic-applet-osk`** (On-screen Keyboard
Button) was added in `f2ef09d4`. Fifteen applets, three panel buttons, and the
shared crates, one workspace. The `.desktop` `Name=` field is what the panel
menu shows, and it is the fastest way to tell which applet is which:

| Directory | `Name=` in its `.desktop` |
|---|---|
| `cosmic-app-list` | App Tray |
| `cosmic-applet-a11y` | Accessibility |
| `cosmic-applet-audio` | Sound |
| `cosmic-applet-battery` | Power & Battery |
| `cosmic-applet-bluetooth` | Bluetooth |
| `cosmic-applet-input-sources` | Input Sources |
| `cosmic-applet-minimize` | Minimized Windows |
| `cosmic-applet-network` | Network |
| `cosmic-applet-notifications` | Notifications Center |
| `cosmic-applet-osk` | On-screen Keyboard Button (new in 1.9) |
| `cosmic-applet-power` | User Session |
| `cosmic-applet-status-area` | Notifications Tray |
| `cosmic-applet-tiling` | Tiling |
| `cosmic-applet-time` | Date, Time & Calendar |
| `cosmic-applet-workspaces` | Numbered Workspaces |

Plus `cosmic-panel-app-button`, `cosmic-panel-launcher-button`,
`cosmic-panel-workspaces-button`, and three crates that are not applets:
`cosmic-applets` (the binary hosting every applet), `cosmic-panel-button`
(shared button widget), and `cosmic-applets-config` (battery/time applet
config schemas). The OSK applet's config lives in the external
`cosmic-osk-config` crate, not in this workspace.

## 3. libcosmic revision map

| Consumer | libcosmic revision | Date | iced revision | Commits behind `03d7dcb8` |
|---|---|---|---|---|
| cosmic-applets `epoch-1.9.0` (this clone) | `03c8f93b2` | 2026-09-21 | `6f1b3a71` | 4 |
| cosmic-panel `epoch-1.9.0` | `23f5d63fe` | 2026-09-14 | `87e1787f` | 18 |
| cosmic-settings `epoch-1.9.0` | `d77e99fb1` | 2026-09-15 | `87e1787f` | 16 |
| cosmic-notifications `epoch-1.9.0` | `d921602cd` | 2026-09-15 | `87e1787f` | 14 |
| cosmic-ext-applet-external-monitor-brightness `master` | `dc1cf9f0` | 2026-07-29 | `7346cffd` | 81 |
| cosmarden (`Cargo.lock`) | `03d7dcb8` | 2026-09-24 | `b2419520` | 0 |

Every revision is an ancestor of our pin, and the applets tag is only **four
commits behind** it — against **seventeen** at 1.8. Those four commits
(`f0eed6e0c`, `3d3e4391f`, `08a09e3df`, `03d7dcb83`) touch only the menu
widgets and the bundled iced revision; `git diff --stat 03c8f93b2..03d7dcb83
-- src/applet/` is empty. The tag's applet structure therefore compiles
against the revision this project builds, unlike the 1.8 snapshot, where the
popup refactor sat between the tag and our pin. The external brightness applet
is still 81 commits behind (it pins a July libcosmic), so treat its popup code
as a pattern source only, not as API reference.

Our lock resolves both git families to a single source each
(`git+https://github.com/pop-os/libcosmic#03d7dcb8`,
`git+https://github.com/pop-os/cosmic-protocols?rev=c0cff4d#c0cff4db`).

## 4. What changed in cosmic-applets since the 1.8 snapshot

`b933e538..82e7cd81` is 8 commits (5 non-merge plus merges #1545, #1562,
#1564), 64 files, +1073/−831; the Rust source delta is 20 files, +126/−62.
Most of the file count is translation updates.

| Commit | Date | Change | Verdict for cosmarden |
|---|---|---|---|
| `e01b16f4` | 2026-09-10 | `chore: adapt to changes in libcosmic` — 16 files, +51/−80: `Surface(surface::Action)` becomes `Surface(surface::Action<Message>)`, and the action moves out of `Cosmic(app::Action::Surface(..))` to `cosmic::Action::Surface(..)`. | Already in our tree (`message/mod.rs`, `app/update/applet.rs`); this was the only breaking change in the 1.8-era libcosmic. |
| `f2ef09d4` (merge `ca5cc038`, #1564) | 2026-09-15 | `Add cosmic-applet-osk` — new applet: a 72-line `lib.rs`, a 13-line `main.rs`, a `.desktop`, one arm in `cosmic-applets/src/main.rs`, one row in `cosmic-applets/build.rs`, metainfo/debian/justfile entries. The applet's whole behavior is one button that toggles `always_shown` in `cosmic-osk-config` through `Config::handler()` + `Config::get_entry`. | Nothing to port. It is a config-toggle applet, not a popup applet; our applet has no device-global config and already uses `cosmic::applet::run` + `core.applet.icon_button`. |
| `409bae32` (merge `595e49c9`, #1562) | 2026-09-14 | `Epoch 1.9.0 version update` — generated version bumps in 20 `Cargo.toml`s, `Cargo.lock`, changelog. | No. |
| `da4c82bb` (merge `0bfc3da2`, #1545) | 2026-09-14 | Translation update from Hosted Weblate. New `en-GB`, `oc`, `th` strings; no logic. | No. |
| `82e7cd81` | 2026-09-20 | `fix(audio): adjust active device index on node removal` — `Nodes::remove` decremented nothing when an earlier index was removed, so the next volume event panicked with an out-of-bounds index. Four lines in `cosmic-applet-audio/src/model.rs`. | App-local bug; our applet has no device vector. No. |

There is no commit after the 1.9 tag: `master` == `epoch-1.9.0` == `82e7cd81`.

## 5. What changed in the other reference clones

### 5.1 cosmic-panel `f664a34c..b5cc19e3` (7 commits)

| Commit | Change | Verdict |
|---|---|---|
| `170a9c5`, `f22088a` | `cargo +nightly fmt`, remove unused imports. | No. |
| `d78104f` | Update `smithay`. | No. |
| `e6e6442` | `fix: Use InputTime::now() instead of start_time.elapsed()` — pointer event timestamps in the panel's nested compositor. | Panel-side; our applet never sees these timestamps. No. |
| `15ba731` | `feat: add config for border thickness around the panels` — new `border_width: f32` in `CosmicPanelConfig` and `CosmicPanelContainerConfig`, default `0.0`. | Our crate does not read panel config; `cosmic-panel-config` is a libcosmic dependency, not ours. No. |
| `b5cc19e` | `fix: layer shell surface corners` — corner-radius handling in the panel's xdg-shell wrapper. | Panel-side rendering. No. |
| `d638769` | Epoch 1.9.0 version update. | No. |

### 5.2 cosmic-settings `3cb538f7..7421ff0a` (11 commits)

`bf920be` is the same mechanical `Surface` action adaptation as `e01b16f4`
(already in our tree). `5eb5a2e` is translations, `874cf1b` the version bump,
`158c88a` a cosmic-protocols pin, `7aa7807` an `nmrs` bump, `7421ff0` a
display-mirroring test. The product changes are an OSK settings page
(`a366fc7`), showing the COSMIC version on the About page (`0f787de`), a date
and time context drawer (`9a6aff1`), and `e6e2acc`, which is only a libcosmic
bump to pick up the cosmic-config fixes in section 9.3. None of them map onto
a cosmarden surface.

### 5.3 cosmic-notifications `8486bd6b..aa4dac27` (4 commits)

`b8a3226` is the same `Surface` adaptation, `471e8c6` the version bump,
`cece682` a libcosmic bump. `aa4dac2` hides visible notification cards when
Do Not Disturb is turned on. Our toaster is in-process UI feedback; it never
goes through the notification daemon's DND state, so nothing to port.

### 5.4 cosmic-ext-applet-external-monitor-brightness `3e6486ff..b068d5c4` (2 commits)

`efa7122` splits the applet's subscription events into `RefreshBrightness`
(re-read cached display handles) and `RescanDisplays` (re-enumerate and
replace them), so a manual refresh finds newly connected displays; `b068d5c`
adds a Polish metainfo translation. Our applet has no device enumeration; the
useful part is already taken — our reopen debounce mirrors this applet's
`last_quit` gate, which is still present at `3e6486f`/`b068d5c`
(`src/app.rs:87-99`).

### 5.5 template and toot

Unchanged since before the 1.8 review.

## 6. Does anything apply to cosmarden?

**No source change is required by the 1.9 applet commits.** The adaptation
commit is already in our code, the OSK applet is a new leaf whose pattern we
already use, and the audio fix is local to that applet. What changed for us
arrived through the libcosmic bump itself and is classified in section 9. The
rest of this section records the checks behind that conclusion.

### 6.1 The applet-side patterns are the ones we already follow

- The 1.9 tag still opens popups with `.on_press_down(Message::TogglePopup)`
  plus `core.applet.get_popup_settings(parent, id, Some((1, 1)), None, None)`
  in the real applets (battery, time, network, power, notifications) — the
  same code as at `epoch-1.8.0`, so not a 1.9 change. The libcosmic example
  applet keeps `on_press_with_rectangle` + an explicit `anchor_rect`, which is
  what our `AppletIconClicked(offset, bounds)` path follows. Both are current;
  the divergence and its testability rationale stay as recorded in
  `applet_reference_comparison.md`.
- The ext-brightness applet still carries the 200 ms close-then-reopen gate
  that our `APPLET_POPUP_REOPEN_DEBOUNCE_MS` mirrors.
- `cosmic-applet-osk` confirms the minimal applet shape (`cosmic::applet::run::<App>(())`,
  `type Executor = SingleThreadExecutor`, `style() -> Some(cosmic::applet::style())`)
  and adds nothing our `Application` impl lacks.

### 6.2 The libcosmic changes do not change any API our code calls

`git diff --stat 53314201b..03d7dcb83` touches `app/cosmic.rs`, `surface/`,
the menu and context-menu widgets, `popover`, `progress_bar`, `scrollable`,
`cosmic-config`, `cosmic-theme`, and the iced submodule. Our code calls none
of the changed trait surfaces: no `menu::Tree`/`menu_bar`/`context_menu`, no
`popover`, no custom progress-bar style, and `CosmardenConfig` is loaded
through `load_legacy`, never through `CosmicConfigEntry::get_entry`. The
workspace compiles and 417 unit tests plus 89 E2E tests pass at `03d7dcb8`
(commit `576635a`).

## 7. Practices: status of the 1.8 verdicts

| 1.8 item | Verdict then | Status at 1.9 |
|---|---|---|
| 6.1 `[patch]` trailing-`//` redirect | Adopt if it verifies | **Moot.** At `03d7dcb8` (and at every applet tag in this review) the graph resolves to one cosmic-protocols source, `c0cff4d`, with no patch; our lock has exactly one libcosmic and one cosmic-protocols source. The trick stays untested in this repo and becomes relevant again only if a future tip splits the pins. `CONTEXT.md` and `Cargo.toml` still state that a same-URL patch cannot repair a split, which is the case they document. |
| 6.2 `desktop-file-validate` in CI | Adopt | **Still open.** Nothing in `justfile`, `.github/`, or `packaging/` validates `crates/cosmarden-ui/resources/com.enikeev.cosmarden.desktop`. The `.desktop` is unchanged in form since 1.8; value is unchanged. |
| 6.3 fmt/clippy gates | Already in place | Unchanged. |
| 6.4 `[workspace.dependencies]` | Already in place | Unchanged. |
| 6.5 release profile | Keep ours | Unchanged. |
| 6.6 `rust-toolchain.toml` | Skip | Unchanged. |
| 6.7 Weblate/localized desktop entries | Product decision | **Still open.** Upstream's `cosmic-applets/build.rs` still generates localized `Name`/`Comment`/`Keywords` from the Fluent trees; the OSK row is the only 1.9 change there. Our one static `.desktop` remains English-only. |
| 6.8 vendoring recipes | Only when packaging needs it | Unchanged. |
| 6.9 `cargo-machete` metadata | Optional developer tool | Unchanged. |
| 6.10 default config schema install | Minor | Unchanged. |
| 6.11 do not imitate upstream's test culture | Keep ours | Unchanged; our suite is now 417 unit + 89 E2E tests against four `#[test]`s in the applet repository. |

## 8. Cautions

- The four commits between the applets tag's libcosmic (`03c8f93b2`) and ours
  change only menu widgets and iced. If we ever build a `menu::Tree`,
  `menu_bar`, or `context_menu`, the 1.9 tag's snippets predate the Esc and
  keyboard-binding fixes (`f0eed6e0c`, `3d3e4391f`) and would be wrong.
- The iced clone is read-only and blob-filtered: `git show
  b2419520:<path>` works, but the libcosmic clone's `iced/` submodule
  directory is empty, so iced diffs are not visible there.
- `5fc4372d` (unfocused suppression) applies only to layer surfaces with
  `KeyboardInteractivity::Exclusive` and to popups parented to subsurfaces.
  COSMIC applets open a transparent xdg toplevel (`cosmic::applet::run` builds
  `Settings` with no layer-surface settings; `SctkLayerSurfaceSettings`'s
  default interactivity is `None`), so do not expect that fix to change our
  popup's dismissal behavior.
- The ext-brightness clone pins a July libcosmic; its popup code is a pattern
  reference, not a compile reference.
- Upstream pins `panic = "abort"` and a nightly rustfmt; both remain choices
  this project deliberately does not copy (1.8 sections 6.3 and 6.5).

## 9. Which of the newer libcosmic/iced patches matter to us

`53314201b..03d7dcb8` is 19 non-merge commits plus two merges (one
translation, one `Live context menu (#1447)`), and it moves the bundled iced
from `9ef36c96` to `b2419520` (12 commits). This section classifies each by
what it does for *this* project. Reproduce any of them with
`git show <hash>` inside `tmp_code_examples/libcosmic` or
`tmp_code_examples/iced`.

### 9.1 Compatibility

Nothing we call changed shape:

- `3664828bf` `chore: session lock action` adds `Action::Lock`/`DestroyLock`
  to `surface::Action<M>` and a `get_lock` path in `app/cosmic.rs`. We never
  match `surface::Action` exhaustively, so no code change was needed; the
  frame remains non-`#[non_exhaustive]`, so a future `match` on it would have
  to list the new variants.
- `d921602cd` `feat(progress_bar): custom style` renames the progress-bar
  traits (`StyleSheet` → `Catalog`, `Appearance` → `Style`) and adds
  `Class`. We call only `indeterminate_circular()`/`indeterminate_linear()`;
  the custom-style API is available if a spinner ever needs a colour
  override, and is unused today.
- `a29ab33d6`/`d77e99fb1` make cosmic-config treat a missing key file as
  `NotFound` instead of an error and stop `get_entry` from collecting those
  errors. Verified: no code in this workspace calls `get_entry` (`rg
  'CosmardenConfig::get_entry|write_entry'` finds nothing); `CosmardenConfig`
  only derives the trait and is read through `load_legacy`.
- `23f5d63fe` moves libcosmic's own `cosmic-client-toolkit` pin to `c0cff4d`,
  the revision our lock already resolves and the reason the 1.9 pin unifies
  the graph.

### 9.2 Applet popup and focus

| Commit | What it changes | Effect here |
|---|---|---|
| `0968a10f` (iced) | sctk reports keys without a text representation (arrows, Home) as `Some("")` where winit reports `None`; the translation now filters the empty string. Reported against a layer surface; the translation is shared by every Wayland surface in this backend. | Our text fields — applet search, applet PIN/master/reprompt, and the main window's inputs — get arrow-key editing under the contract the widgets are written against. User-visible, arrives through the bump. |
| `6f1b3a71` (iced) | A grabbing popup's modifier changes are also reported to its parent toplevel. | Our popup is `grab: true`; modifier state no longer diverges between popup and main window. Minor, internal. |
| `5fc4372d` (iced) | Does not report a keyboard leave when focus moves from a layer surface to its own grabbing popup; also resolves popup parents through subsurface chains and corrects their anchor offsets. | Does not apply to COSMIC applets: the applet window is an xdg toplevel and its popup's parent is that toplevel, and the leave-swallowing branch requires `KeyboardInteractivity::Exclusive` (see section 8). `PinRequested`'s "the panel drops unfocused popups" constraint is unchanged. |
| `87ab8179e` | Window, subsurface, and layer-surface creation now chain live-settings application before the surface command instead of batching them; the popup path already chained. | Internal ordering; no code action. |

### 9.3 User-visible through the bump

| Commit | What a user of cosmarden sees |
|---|---|
| `836bc1e9` (iced) | The main vault window stays responsive to mouse input while maximized: the drag-resize border handler no longer consumes motion events for a maximized or undecorated window. Our pin's tip, libcosmic `03d7dcb83`, is the bump that delivers this iced fix. |
| `b2419520` (iced) | The resize cursor is restored after drag-resize, instead of sticking to the border cursor. |
| `5716b9a6` (iced) | Windows' user interfaces are built in window-id order rather than in a drained map's order; relevant when the main window and the applet popup are both open. |
| `890a53fe`, `85d4e4d5` (iced) | Tablet input support for toplevels and touchscreen window moving. |
| `5e2b5807` (iced) | accesskit update; screen-reader plumbing only. |
| `021cb93f` (iced) + `eaa7a5a24` | Scrollable padding was re-expressed through iced's `Scrollbar` type; libcosmic's wrapper moved from `scrollbar_padding(8.0)` to the equivalent `padding(8.0)` in the same range. Our `cosmic::widget::scrollable` call sites (sidebar, generator, settings, applet search) keep the same 8 px scrollbar padding; no layout change. |

### 9.4 Does not reach us

- `34caca138` (live context-menu refresh), `f0eed6e0c` (Esc closes the
  innermost menu), `3d3e4391f` (context-menu keyboard bindings),
  `8c0e57835` (no leading dividers), and the `Live context menu (#1447)`
  merge all change `context_menu`/`menu_bar`/`menu_inner`. The UI builds no
  `menu::Tree`, `menu_bar`, or `context_menu`; `text_input`'s menu lives in
  `text_context_menu.rs`, which is untouched in this range.
- `e793c4552` reverts a popover modal change. The UI uses no `popover`.
- `3664828bf`/`87e1787f` add the session-lock layer protocol. The applet does
  not lock the session, only the vault.
- `d9431dc36`, `8aac70d89`, `2f99abe16`, `7e82198ab`, `52b3f0728` are
  formatting, translation, and iced-bump commits with no call-site change.

### 9.5 The popup workarounds stay

The 1.8 review (section 9.2) left one open action: re-test the applet popup
without the eager `take()` of `applet_popup` and without
`APPLET_POPUP_REOPEN_DEBOUNCE_MS`, on the theory that
`a401af8b`/`6081a48cd`/`9f634b0ec` made popup close accounting reliable.

The 1.9 range adds no evidence that settles it. The fixes that would matter
most — `5fc4372d` — do not apply to this applet's surface stack (section 9.2),
and nothing in the applet repository removed an equivalent workaround: the
ext-brightness applet still carries its 200 ms gate. Keep both workarounds
(`app/update/applet.rs:31-59`, `app/update/applet/helpers.rs:13-25`) until
someone can run the manual sequence on a live session: open, close, reopen,
Escape, click away, and a toast visible, with the panel on each COSMIC panel
edge. Only that run can justify deleting the timestamp field and its tests.

## 10. Refreshing these clones next time

```sh
# Applets: tag (release baseline) or master (newest API usage)
cd tmp_code_examples/cosmic_examples/cosmic-applets
git fetch --tags --prune origin
git checkout -B epoch-1.9.0 epoch-1.9.0
# Applet set changed? (no output means identical)
diff <(git ls-tree --name-only -d HEAD | sort) \
     <(git ls-tree --name-only -d origin/master | sort)

# libcosmic: keep in step with what the project builds against
cd ../../libcosmic
git fetch --tags origin
git checkout --detach "$(grep -A2 '^name = "libcosmic"' ../../Cargo.lock | grep -o '[0-9a-f]\{40\}')"

# iced: the libcosmic submodule revision, read-only
cd ../iced
git fetch --prune origin
git checkout --detach "$(git -C ../libcosmic ls-tree HEAD iced | awk '{print $3}')"

# The other consumers: bring their tags forward to the same epoch
for d in cosmic-panel cosmic-settings cosmic-notifications; do
  git -C ../cosmic_examples/$d fetch --tags --prune origin
  git -C ../cosmic_examples/$d checkout --detach "epoch-1.9.0"
done
```

The template, toot, and external-brightness clones were brought forward by
their own `fetch` in this refresh; bring them forward again only when their
`PanelConfig`, settings, or subscription patterns come into play. A locale
directory appearing upstream does not mean it is translated — check the file
sizes before treating a language as supported.
