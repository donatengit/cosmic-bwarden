# Upstream applet review: `pop-os/cosmic-applets` at `epoch-1.10.0`

Reference material for the applet side of `cosmarden-ui`, refreshed for
COSMIC Epoch 1.10. This document records the revisions held locally, what
changed upstream since the 1.9 snapshot, and which upstream practices or
patches are worth copying. The code-level comparison of our applet against the
upstream examples stays in
[`applet_reference_comparison.md`](applet_reference_comparison.md); this file
covers revisions, change history, and which of the newer libcosmic/iced
patches matter to us.

The workspace moved to the COSMIC 1.10-era libcosmic revision `dbc62f2d8` on
2026-10-10. The 1.9 snapshot is
[`cosmic_applets_review_1.9.md`](cosmic_applets_review_1.9.md); every "our
pin" statement there describes `03d7dcb8` and the applets tag `epoch-1.9.0`,
not the current tree.

This is the Epoch 1.10 snapshot. On 2026-10-24 the workspace moved to the
COSMIC 1.11-era libcosmic revision `430f3d48a`, so every "our pin" statement
here (`dbc62f2d8`, applets tag `epoch-1.10.0`) describes the 1.10 tree. The
1.11 snapshot is
[`cosmic_applets_review_1.11.md`](cosmic_applets_review_1.11.md).

All paths below are relative to the repository root unless stated otherwise.
The reference clones live under `tmp_code_examples/`, which is gitignored
(`.gitignore:77`) — nothing here is a build input.

## 1. Local reference checkouts

| Path | Revision | Date | Notes |
|---|---|---|---|
| `tmp_code_examples/cosmic_examples/cosmic-applets` | `406d1b7d627138fdf4078f654268d3d8774e50f1` | 2026-10-06 | Branch `epoch-1.10.0`, the tag of the same name. `master` is the same commit. |
| `tmp_code_examples/libcosmic` | `dbc62f2d899cdbfbe43d170689ddc0290db663f4` | 2026-10-09 | Detached HEAD, matching the revision `Cargo.lock` builds against. |
| `tmp_code_examples/iced` | `a31e6c6f837e4c4d0cdb790df50883bee01ea558` | 2026-09-30 | `pop-os/iced`, the fork libcosmic bundles at `dbc62f2d8`. Cloned with `--filter=blob:none --no-checkout`; read it with `git show <rev>:<path>`. |
| `tmp_code_examples/cosmic_examples/cosmic-panel` | `3ce6ce1cec97669f91e809962d9446aaefe8a06e` | 2026-10-06 | Detached at `epoch-1.10.0`, also current `master`. The same commit `Cargo.lock` pins for `cosmic-panel-config` (crate version 1.10.0). |
| `tmp_code_examples/cosmic_examples/cosmic-settings` | `58aeb65244b8830880e8b6ced89dcd25663f20b1` | 2026-10-07 | Detached at `epoch-1.10.0`, also current `master`. |
| `tmp_code_examples/cosmic_examples/cosmic-notifications` | `1e4c1f11fdcae273d6954fd0bfc54749a6145267` | 2026-10-06 | Detached at `epoch-1.10.0`, also current `master`. |
| `tmp_code_examples/cosmic_examples/cosmic-ext-applet-external-monitor-brightness` | `b068d5c4c2e604f79d48060cb2a53c926165bf4e` | 2026-09-12 | Current `master`; unchanged since the 1.9 review. The clone the popup-reopen debounce is modelled on. |
| `tmp_code_examples/cosmic_examples/cosmic-applet-template` | `fc6b688eae769be2821d23b264ccd711f2abf0a6` | 2026-01-22 | Unchanged since before the 1.8 review. |
| `tmp_code_examples/cosmic_examples/toot` | `1c20d33dae5cac6bd543fe782eec745eb762980e` | 2026-07-14 | Unchanged since before the 1.8 review. |

Refresh procedure:

```sh
cd tmp_code_examples/cosmic_examples/cosmic-applets
git fetch --tags --prune origin
git checkout -B epoch-1.10.0 epoch-1.10.0      # or: git checkout -B master origin/master
```

## 2. libcosmic revision map

| Consumer | libcosmic revision | Date | iced revision | Commits behind `dbc62f2d8` |
|---|---|---|---|---|
| cosmic-applets `epoch-1.10.0` (this clone) | `5a8bd9482` | 2026-10-02 | `a31e6c6f` | 8 |
| cosmic-panel `epoch-1.10.0` | `5a8bd9482` | 2026-10-02 | `a31e6c6f` | 8 |
| cosmic-settings `epoch-1.10.0` | `5a8bd9482` | 2026-10-02 | `a31e6c6f` | 8 |
| cosmic-notifications `epoch-1.10.0` | `5a8bd9482` | 2026-10-02 | `a31e6c6f` | 8 |
| cosmic-ext-applet-external-monitor-brightness `master` | `dc1cf9f0` | 2026-07-29 | `7346cffd` | 91 |
| cosmarden (`Cargo.lock`) | `dbc62f2d8` | 2026-10-09 | `a31e6c6f` | 0 |

All four epoch-1.10.0 consumer tags resolve libcosmic to the same revision,
`5a8bd9482`, so the 1.10 release shipped one libcosmic for every COSMIC
surface. Our pin is the master tip eight commits ahead of it: those eight
carry widget fixes plus GPU-selection, i18n, and docs commits, and section 6
classifies them. The applet structure
(`src/applet/`) is unchanged across those eight commits, so the tag's applet
code still compiles against what this project builds.

Our lock resolves both git families to a single source each
(`git+https://github.com/pop-os/libcosmic#dbc62f2d`,
`git+https://github.com/pop-os/cosmic-protocols?rev=c0cff4d#c0cff4db`), which
is the invariant the bare-URL rule in `Cargo.toml` exists to preserve.

## 3. What changed in cosmic-applets since the 1.9 snapshot

`82e7cd81..406d1b7d` is 3 non-merge commits plus merges, 50 files,
+361/−198. Every non-i18n change is a version bump or packaging:

| Commit | Date | Change | Verdict for cosmarden |
|---|---|---|---|
| `57ace747` | 2026-09-30 | `COSMIC policy update` — the shared `.github/PULL_REQUEST_TEMPLATE.md` edit and a new `require-checklist.yml` workflow (`mheap/require-checklist-action@v2`). Applied identically in libcosmic, iced, panel, settings, and notifications. | Process, not code. Our repo has no PR template and no required-checklist gate; `CONTRIBUTING.md` already lists the gates. Nothing to port for the applet. |
| `0d8c604b` | 2026-10-06 | `i18n: translation updates from weblate` — 18 locale files under `i18n/`, including the new `desktop_entries.ftl` strings. | No logic; our `.desktop` is a single static English file (section 7). |
| `406d1b7d` | 2026-10-06 | `Epoch 1.10.0 version update` — generated version bumps in the 17 workspace `Cargo.toml`s, `Cargo.lock`, changelog, debian. | No. |

There is no commit after the 1.10 tag: `master` == `epoch-1.10.0` ==
`406d1b7d`. Unlike 1.9, no applet source file changed in this range, so
nothing applet-side is even a candidate.

## 4. What changed in the other reference clones

### 4.1 cosmic-panel `b5cc19e3..3ce6ce1c` (3 commits)

The only source change is `f4ba4ef` (`refactor: check for cosmic prefix`): the
panel now accepts `X-CosmicOverflowMinSize` / `X-CosmicOverflowPriority`
desktop-entry keys as fallbacks for the unprefixed `X-OverflowMinSize` /
`X-OverflowPriority` in `cosmic-panel-bin/src/space/wrapper_space.rs`. The rest
is `COSMIC policy update`, `Epoch 1.10.0 version update`, a new shared
`cargo.just` recipe file, GitHub workflow config, and debian packaging. Our
applet's `.desktop` declares neither key, so the fallback does not change its
panel sizing, and we have our own `justfile`. No.

### 4.2 cosmic-settings `7421ff0a..58aeb65` (7 commits)

`fd68317` is the version bump, `c972ef2` i18n, `8249241` the policy update.
The product changes are `8940ec1` (a first pass at a drawing-tablet settings
page, 411 new lines under `pages/input/drawing_tablet.rs`), `0e307f3` (VPN
config on Enter in the wireguard name field), and `ee5b432` (delegate GTK font
preferences to cosmic-settings-daemon), plus `58aeb65` (debian gettext/nm
packaging). None maps onto a cosmarden surface.

### 4.3 cosmic-notifications `aa4dac27..1e4c1f1` (2 non-merge commits)

`41d722c` policy update, `1e4c1f1` version bump. No product change.

### 4.4 template and toot

Unchanged since before the 1.8 review; the external-brightness applet is
unchanged since the 1.9 review.

## 5. Does anything apply to cosmarden?

**No source change is required by the 1.10 applet commits.** The applet range
contains no Rust change at all, and the other consumers' product changes are
settings pages and packaging that map onto nothing here. The libcosmic/iced
changes that do reach us arrive through the bump and are classified in
section 6.

### 5.1 The applet-side patterns are unchanged

The 1.10 tag still opens popups with `.on_press_down(Message::TogglePopup)`
plus `core.applet.get_popup_settings(...)`, still carries the ext-brightness
applet's 200 ms close-then-reopen gate that our `APPLET_POPUP_REOPEN_DEBOUNCE_MS`
mirrors, and adds no new applet whose pattern we lack. `cosmic-applet-osk`
(added in 1.9) is unchanged.

### 5.2 The libcosmic/iced changes break no API our code calls

The workspace compiles clean at `dbc62f2d8` with no source change:
`cargo check --workspace --all-targets`, `-p cosmarden-agent --features tpm`,
`-p cosmarden-ui --features kde`, and `-p cosmarden-plasma-tray` all pass.
None of the ten libcosmic commits reshapes a trait or builder our code uses;
section 6 records the one new API and why it is left unused.

## 6. Which of the newer libcosmic/iced patches matter to us

`03d7dcb83..dbc62f2d8` is 10 non-merge commits (eight of which are not in the
1.10 release, since the tags sit at `5a8bd9482`), and it moves the bundled
iced from `b2419520` to `a31e6c6f` (2 commits). Reproduce any of them with
`git show <hash>` inside `tmp_code_examples/libcosmic` or
`tmp_code_examples/iced`.

### 6.1 Compatibility

Nothing we call changed shape. The diff touches `desktop/gpu.rs`,
`theme/style/{button,iced}.rs`, and `widget/{list,spin_button,text_input,toggler}`;
our call sites compile unchanged, and no trait was renamed.

### 6.2 User-visible through the bump

The "In 1.10?" column marks whether the commit is an ancestor of the revision
the epoch-1.10.0 tags pin (`5a8bd9482`); the "no" rows are the extra fixes our
pin carries past the release.

| Commit | In 1.10? | What a user of cosmarden sees |
|---|---|---|
| `2157f9d1` (iced) | yes | A slider drag ends when the pointer leaves the window instead of capturing the cursor. Our two sliders — password-generator length and settings lock timeout — never set `on_release`, so the change is purely the fixed release behavior, with no call-site edit. |
| `5a8bd9482` | yes | A menu item no longer double-paints its background when its hover color equals its base color. Our applet's `menu_button` rows (Open Vault, Quit footer, search results, reprompt) use the affected `Button::MenuItem`/`ListMenuItem` arms. |
| `60ad2cc4c` | no | An unselected `Button::ListItem` now draws its text and icon in the component `on` color instead of the list-button palette's. `cosmic_settings::item` builds `list::button` rows, so this reaches our settings pane, generator options, auth toggler rows, and the PIN-unlock section. |
| `f6be7b5f7` | no | A disabled toggler keeps its toggled track color and no longer shows a pointer cursor. Every toggler we render (auth "Remember email" / "Enable PIN unlock", settings TPM) passes `on_toggle`, so none is disabled and there is no visible change today — the fix matters only if a toggle is ever shown read-only. |
| `d880804b6`, `60ad2cc4c` | no | `Container::Transparent` becomes `iced_container::Style::default()`. We style containers only as `Primary`, `Background`, `WindowBackground`, or `Dialog(true)`, never `Transparent`, and the two internal widgets that used it (`ListColumn`, `settings::item`) end up unchanged here, so no call site is affected. |

### 6.3 Available but not adopted: `capture_escape`

`4ee2bdb6f` adds `TextInput::capture_escape(bool)`, default `true`. By default
a focused text field consumes Escape (unfocusing and marking itself read-only);
setting `false` lets the key fall through, so it can drive an outer surface —
for example, closing the applet popup from the search or PIN field.

We do not adopt it. Both `search_input` and `secure_input` return
`TextInput`, so the setter is chainable and the change would compile, but our
applet handles no Escape action today and the popup's dismissal is driven by
the panel's focus tracking, not by our update loop. Whether `capture_escape(false)`
improves or harms the open/close/reopen sequence can only be settled on a live
session (the same untested sequence section 9.5 of the 1.9 review already
carries). Leaving the default keeps current behavior; revisit only alongside
that live test.

### 6.4 Does not reach us

- `fd182fdbb` `feat(desktop): add GPU selection policy` adds `desktop::gpu`
  (`Preference`, `Descriptor`). We launch no external process with a GPU
  override.
- `41947abba` fixes the vertical `spin_button` orientation. The UI uses no
  spin button.
- `7f715bc5d` (i18n), `6af8b7051` (policy/CI), and `dbc62f2d8` (flatpak docs)
  are translations, workflow, and documentation.

## 7. Practices: status of the 1.9 verdicts

| Item | Status at 1.10 |
|---|---|
| 1.9/1.8 `[patch]` trailing-`//` redirect | **Moot.** The lock still resolves one cosmic-protocols source (`c0cff4d`) with no patch. The trick stays relevant only if a future tip splits the pins; `Cargo.toml` and `CONTEXT.md` keep the same-URL-patch caveat. |
| `desktop-file-validate` in CI | **Still open.** Nothing in `justfile`, `.github/`, or `packaging/` validates `crates/cosmarden-ui/resources/com.enikeev.cosmarden.desktop`. Unchanged value. |
| Upstream `require-checklist.yml` + PR checklist | **Not adopted.** New in 1.10, applied to every COSMIC repo. Our gates live in `CONTRIBUTING.md`; a PR checklist is a process choice, not a 1.10 requirement. |
| Localized `.desktop` entries | **Still open.** Upstream's `build.rs`/`desktop_entries.ftl` keep growing; our one static `.desktop` remains English-only. |
| fmt/clippy gates, `[workspace.dependencies]`, release profile | Unchanged from 1.9. |
| `rust-toolchain.toml` pin | Skip; we build with the distribution toolchain. |
| Do not imitate upstream's test culture | Keep ours. Our suite (417 unit + 89 E2E) still dwarfs the applet repository's handful of `#[test]`s. |

## 8. Cautions

- Our pin is **eight commits ahead of the revision the 1.10 tags ship**
  (`5a8bd9482`). Those eight are the GPU-selection, i18n, disabled-toggler,
  container-style, `capture_escape`, spin-button, `ListItem`-style, and docs
  commits. Two of them are user-visible and absent from the 1.10 release:
  `60ad2cc4c` (list-item text/icon colors) and `d880804b6` (container
  transparent style). This is deliberate (the project tracks the master tip;
  the same policy produced the 1.9 pin) but means our UI is not byte-identical
  to a 1.10 stock install.
- The iced clone is read-only and blob-filtered: `git show a31e6c6f:<path>`
  works, but the libcosmic clone's `iced/` submodule directory is empty, so
  iced diffs are not visible there.
- The ext-brightness clone pins a July libcosmic (91 commits behind); its popup
  code is a pattern reference, not a compile reference.
- Upstream pins `panic = "abort"` and a nightly rustfmt; both remain choices
  this project deliberately does not copy.

## 9. Refreshing these clones next time

```sh
# Applets: tag (release baseline) or master (newest API usage)
cd tmp_code_examples/cosmic_examples/cosmic-applets
git fetch --tags --prune origin
git checkout -B epoch-1.10.0 epoch-1.10.0

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
  git -C ../cosmic_examples/$d checkout --detach "epoch-1.10.0"
done
```

The template, toot, and external-brightness clones need no refresh until their
`PanelConfig`, settings, or subscription patterns come into play.
