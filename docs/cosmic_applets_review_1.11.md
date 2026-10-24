# Upstream applet review: `pop-os/cosmic-applets` at `epoch-1.11.0`

Reference material for the applet side of `cosmarden-ui`, refreshed for
COSMIC Epoch 1.11. This document records the revisions held locally, what
changed upstream since the 1.10 snapshot, and which upstream practices or
patches are worth copying. The code-level comparison of our applet against the
upstream examples stays in
[`applet_reference_comparison.md`](applet_reference_comparison.md); this file
covers revisions, change history, and which of the newer libcosmic/iced
patches matter to us.

The workspace moved to the COSMIC 1.11-era libcosmic revision `430f3d48a` on
2026-10-24. The 1.10 snapshot is
[`cosmic_applets_review_1.10.md`](cosmic_applets_review_1.10.md); every "our
pin" statement there describes `dbc62f2d8` and the applets tag `epoch-1.10.0`,
not the current tree.

All paths below are relative to the repository root unless stated otherwise.
The reference clones live under `tmp_code_examples/`, which is gitignored
(`.gitignore:77`) — nothing here is a build input.

## 1. Local reference checkouts

| Path | Revision | Date | Notes |
|---|---|---|---|
| `tmp_code_examples/cosmic_examples/cosmic-applets` | `328d142faac17c9cc3e00d434d05060411c007ea` | 2026-10-20 | Branch `epoch-1.11.0`, the tag of the same name. `master` is one audio-only commit ahead (`dc252e34`, 2026-10-21). |
| `tmp_code_examples/libcosmic` | `430f3d48a97d8779f9c1985473912e594ccca1ae` | 2026-10-19 | Detached HEAD, matching the revision `Cargo.lock` builds against. Also `origin/master`. |
| `tmp_code_examples/iced` | `229186b5180b6217d93866c912e6373ac378b231` | 2026-10-10 | `pop-os/iced`, the fork libcosmic bundles at `430f3d48a`. Cloned with `--filter=blob:none`; read it with `git show <rev>:<path>`. |
| `tmp_code_examples/cosmic_examples/cosmic-panel` | `ca20ce3185b3b4404f429d59d429c40b58a2ddef` | 2026-10-19 | Detached at `epoch-1.11.0`, also current `master`. The same commit `Cargo.lock` pins for `cosmic-panel-config` (crate version 1.11.0). |
| `tmp_code_examples/cosmic_examples/cosmic-settings` | `f1d00fc06e18203e880771617657067c10c43321` | 2026-10-20 | Detached at `epoch-1.11.0`. |
| `tmp_code_examples/cosmic_examples/cosmic-notifications` | `382927dc48964f3b098429ad6b836cc4b7e7ecf2` | 2026-10-20 | Detached at `epoch-1.11.0`. |
| `tmp_code_examples/cosmic_examples/cosmic-ext-applet-external-monitor-brightness` | `b068d5c4c2e604f79d48060cb2a53c926165bf4e` | 2026-09-12 | Current `master`; unchanged since the 1.9 review. The clone the popup-reopen debounce is modelled on. |
| `tmp_code_examples/cosmic_examples/cosmic-applet-template` | `fc6b688eae769be2821d23b264ccd711f2abf0a6` | 2026-01-22 | Unchanged since before the 1.8 review. |
| `tmp_code_examples/cosmic_examples/toot` | `1c20d33dae5cac6bd543fe782eec745eb762980e` | 2026-07-14 | Unchanged since before the 1.8 review. |
| `tmp_code_examples/vaultwarden` | `b71cb7a832080d9fdfdd97bcccf065307cf73236` | 2026-10-22 | `main`, 19 commits past the newest release tag (`1.37.4`). |
| `tmp_code_examples/bitwarden-official-clients` | `c0ca72a0c83cc8b7f990138ac04961cf63d985bc` | 2026-10-23 | Shallow clone of `bitwarden/clients` `main`; read for protocol and autofill behavior, never copied verbatim. |
| `tmp_code_examples/rbw_reference` | `ecc1654a6c15f03cbdfc4dcb23ed0ddc351b9b76` | 2026-05-07 | A fork of `rbw`; the upstream `dozens/rbw` repository no longer resolves. |

Refresh procedure:

```sh
cd tmp_code_examples/cosmic_examples/cosmic-applets
git fetch --tags --prune origin
git checkout -B epoch-1.11.0 epoch-1.11.0      # or: git checkout -B master origin/master
```

## 2. libcosmic revision map

| Consumer | libcosmic revision | Date | iced revision | Commits behind `430f3d48a` |
|---|---|---|---|---|
| cosmic-applets `epoch-1.11.0` | `430f3d48a` | 2026-10-19 | `229186b5` | 0 |
| cosmic-panel `epoch-1.11.0` | `430f3d48a` | 2026-10-19 | `229186b5` | 0 |
| cosmic-settings `epoch-1.11.0` | `430f3d48a` | 2026-10-19 | `229186b5` | 0 |
| cosmic-notifications `epoch-1.11.0` | `430f3d48a` | 2026-10-19 | `229186b5` | 0 |
| cosmic-ext-applet-external-monitor-brightness `master` | `dc1cf9f0` | 2026-07-29 | `7346cffd` | 99 |
| cosmarden (`Cargo.lock`) | `430f3d48a` | 2026-10-19 | `229186b5` | 0 |

Unlike 1.10, every epoch-1.11.0 consumer tag pins the libcosmic master tip, so
our pin is the release revision with no extra commits on top. The bundled iced
moves from `a31e6c6f` to `229186b5` (2 commits); cosmic-protocols stays at
`c0cff4d` in every consumer, so the lock still resolves one source per git
family (`git+https://github.com/pop-os/libcosmic#430f3d48a`,
`git+https://github.com/pop-os/cosmic-protocols?rev=c0cff4d#c0cff4db`).

## 3. What changed in cosmic-applets since the 1.10 snapshot

`406d1b7d..328d142f` is 8 non-merge commits, 49 files, +308/−458. Two are
translations and one is the version bump; the five product commits are one
shared-widget refactor and four audio-applet fixes.

| Commit | Change | Verdict for cosmarden |
|---|---|---|
| `22a140b5` | `fix: simplify hover logic` — `cosmic-app-list`'s toplevel previews replace a `hovered_toplevel` state field plus `on_enter`/`on_exit` messages with iced's `hover(base, top)` helper. | **Adopted** for the applet search rows (section 5). |
| `ecabe712` | `fix(audio): mouse wheel sets volume to 0 if popup was not opened` — initialise `max_sink_volume`/breakpoints in `init` instead of relying on `Default`, because a scroll before the first popup used the zeroed values. | Audio-specific. No scroll gesture in our popup. |
| `0bdb1c1e` | `fix(audio): throttle mouse wheel adjustments and set volume step to 5` — 192 ms gate, relative `SetSinkVolumeScroll` message, mute cleared on an absolute set. | Audio-specific. |
| `93c87162` | `chore: remove unused code from widget used to capture mouse wheel events` — strips 207 of 217 lines from the audio applet's local `MouseArea`, leaving only `on_mouse_wheel`. | Audio-specific; we have no local wheel widget. |
| `328d142f` | `fix(audio): do nothing when scrolling down at zero volume` — also skip when the requested volume equals the current one. | Audio-specific. |
| `2060ef7d`, `3d643f81` | i18n updates. | Nothing. |
| `ace9c1a9` | `Epoch 1.11.0 version update`. | Nothing. |

There is one commit after the tag: `dc252e34` (`Fix audio applet stretched
thumbnails & short audio names`, 2026-10-21), so `master` is not a compile
reference for 1.11.

## 4. What changed in the other reference clones

### 4.1 cosmic-panel `3ce6ce1c..ca20ce31` (1 commit)

`ca20ce3` is the version bump only; `cosmic-panel-config` and
`xdg-shell-wrapper-config` become 1.11.0. The panel's own `Cargo.lock` still
carries a different cosmic-protocols revision for the *binary* crates
(`?rev=32283d7`), but the two crates we consume depend on neither
cosmic-protocols nor its client toolkit, so that pin never enters our graph —
`Cargo.lock` still holds exactly one `cosmic-protocols` source.

### 4.2 cosmic-settings `58aeb652..f1d00fc` (4 commits)

`c4ff548` replaces broken autostart symlinks instead of failing silently,
`f3b1b60` adds tap-to-drag touchpad controls, plus i18n and the version bump.
Nothing maps onto a cosmarden surface.

### 4.3 cosmic-notifications `1e4c1f11..382927d` (3 non-merge commits)

`4ee24cc` raises its panel-connection timeout from 1 s to 10 s: the daemon is
handed a socket FD by the panel and gave up too early on a loaded boot.
`c1a0695` adds a debian `Provides:` field, `2c7d50f` is the version bump. The
timeout fix is specific to that FD-handshake daemon; libcosmic's applet runtime
has no equivalent fixed timeout to raise.

### 4.4 template, toot, and external-brightness

Unchanged; see section 1.

### 4.5 The non-COSMIC reference clones

**vaultwarden.** `1.37.4..main` is 19 commits (`b71cb7a8`, 2026-10-22), none of
which changes the wire our client uses: `src/api/core/ciphers.rs` and
`src/api/core/sync.rs` are untouched. The changes are 2FA refactors, event
logging, org-policy revision dates, auth-request cleanup, and a new
`biometrics-sdk-ipc` flag. `1.37.4` is still the newest tag, so the version
notes in `CONTEXT.md` remain current.

**bitwarden-official-clients.** The clone moved from `45eb0013` (2026-09-25) to
`c0ca72a0` (2026-10-23), 329 commits. Three candidates were examined; none is
adopted:

- `124f1b1d` (PM-42722) bounds the KDF parameters read from a
  *password-protected export file*. It hardens the importer against a crafted
  export; cosmarden has no import path, so there is nothing to bound.
- `9d29a0dc` pins `rsa` to `0.9.6` because Bitwarden's ssh-agent signs through
  `ssh-key` 0.6, whose RSA tests break on `rsa` 0.9.10. Our agent does not use
  that path: `crates/cosmarden-agent/src/ssh_agent/sign.rs` builds an
  `rsa::RsaPrivateKey` and signs with `pkcs1v15` directly, and the E2E
  `test_ssh_agent_signs_real_ssh_connection_rsa` covers RSA signing end to end,
  so the pin does not apply.
- `abccf1e3` and `6f2ee59d` refine browser autofill. The first splits username
  and TOTP field names so a TOTP token outranks a bare `login` token; our
  heuristics already exclude OTP tokens outright (`EXCLUDED_PREFIXES` in
  `content-heuristics.js`) and never autofill TOTP codes, so there is no tie to
  break. The second moves Bitwarden's insecure-page confirmation into the
  background: it asks before filling an `http:` page when the cipher holds an
  `https:` URI for the same host. **That gap exists here**: `content.js` checks
  the host but not the scheme, so a user-initiated fill on the http twin of a
  saved https site happens without a warning. Closing it needs a confirmation
  surface in the fill path (the existing content bar) plus unit and E2E tests;
  it is left out of this dependency bump.

Everything else in the range is UI work, SDK migration, passkeys (which
cosmarden does not implement), enterprise/PAM, and billing.

**rbw_reference.** The fork is at `ecc1654` (2026-05-07) with no commits behind
`origin`, and `dozens/rbw` returns 404, so there is no newer reference to read.

## 5. Adopted: iced's `hover` helper for search-row actions

Our applet revealed a search row's open/link/copy icons by tracking
`applet_hovered_row_id` in state: `mouse_area(...).on_enter(...).on_exit(...)`
sent `AppletSearchRowHoverChanged`, an update arm set or cleared the id, the
popup reset cleared it, and `applet_menu::row_actions_visible` decided whether
to rebuild the row with its icons. The 1.11 app-list replaces exactly this
shape with `iced::widget::hover(base, top)`, which draws `top` only while
`base` is hovered or holds focus.

Our rows now call `hover(label_row, action_overlay(icons))` and nothing else.
Removed: the state field and its reset, the message variant and its debug arm
and update arm, the `row_actions_visible` helper, and the three tests that
exercised the state machine; the "hovered row renders" test became a plain
"search row renders" test. The visibility decision now lives in the widget,
which is where upstream keeps it.

Two behavioral consequences, both acceptable:

- **Keyboard focus keeps the icons visible.** They stay shown while one of the
  action buttons has focus, which the state version did not do.
- **The tooltips still work.** `Hover::overlay` returns the base overlay while
  the label's tooltip is up, and the top layer's overlays when it is not, so
  the per-icon tooltips are not lost.

The `hover` helper is an iced 0.14 widget available before this bump; it is
adopted now because the 1.11 applet release uses it.

## 6. Which of the newer libcosmic/iced patches matter to us

`dbc62f2d8..430f3d48a` is 8 non-merge commits and moves the bundled iced from
`a31e6c6f` to `229186b5` (2 commits). All eight are in the 1.11 release, since
the tags pin this tip. Reproduce any of them with `git show <hash>` inside
`tmp_code_examples/libcosmic` or `tmp_code_examples/iced`.

### 6.1 Compatibility

Nothing we call changed shape: `cargo check --workspace --all-features` and
the tpm/kde/plasma feature checks pass, and the bump itself forced no source
change. `TextInput::capture_escape` is gone, but no call site used it.

### 6.2 User-visible through the bump

| Commit | What a user of cosmarden sees |
|---|---|
| `430f3d48a` | `cosmic::widget::scrollable` no longer pads its content by 8 px; the padding moves to the scrollbar ends. Our four scrollables — settings pane, vault sidebar, generator pane, applet results — lose that inset and the scrollbar is inset instead. No call-site edit: settings and generator already sit in a 20 px container, and the applet rows now line up with the padded search field. |
| `e1f2cd2e5` | Text fields no longer consume Escape: the key still unfocuses the field, but it now reaches the parent. We handle no Escape action, so the visible behavior is unchanged; the 1.10 review declined the opt-in `capture_escape(false)`, which is now the only behavior. |
| `229186b5` (iced) | The software renderer (`tiny_skia`) computed text damage bounds with the previous item's transformation, which misplaced the damage between groups. If cosmarden falls back to that renderer (no usable GPU), stale or smeared text regions are fixed. |

### 6.3 Does not reach us

- `3e927a678` fixes `Table::deactivate`; the UI has no table widget.
- `fcb3865f5` changes menu bars to dynamic item height; we use `menu_button`
  rows, not `menu::bar` or `ResponsiveMenuBar`.
- `a6ac45c7c` and `9e739429d` make `dropdown`/`popup_dropdown` close on
  Escape; the UI uses no dropdown widget.
- `b1d52d563` is a docs typo; `3a37c8bff` is the iced bump itself.
- `e03ca50e` (iced) fixes BSD feature gates; we build Linux only.

## 7. Practices: status of the 1.10 verdicts

| Item | Status at 1.11 |
|---|---|
| 1.9/1.8 `[patch]` trailing-`//` redirect | **Moot.** The lock still resolves one cosmic-protocols source (`c0cff4d`) with no patch. |
| `desktop-file-validate` in CI | **Still open.** Nothing in `justfile`, `.github/`, or `packaging/` validates our `.desktop`. |
| Upstream `require-checklist.yml` + PR checklist | **Not adopted.** Our gates live in `CONTRIBUTING.md`. |
| Localized `.desktop` entries | **Still open.** Upstream i18n keeps landing; our `.desktop` is English-only. |
| fmt/clippy gates, `[workspace.dependencies]`, release profile | Unchanged. |
| `rust-toolchain.toml` pin | Skip; we build with the distribution toolchain. |
| Do not imitate upstream's test culture | Keep ours. |

## 8. Cautions

- Our pin is now **exactly the revision the 1.11 tags ship**, so there are no
  out-of-release widget fixes to track this cycle. Re-check on the next bump:
  if the next tag falls behind `master` again, section 6's "in release"
  distinction returns.
- `cosmic-panel`'s `Cargo.lock` naming a different cosmic-protocols revision
  is not a signal that our graph split. Check our own lock with
  `grep -o 'git+https://github.com/pop-os/cosmic-protocols[^"]*' Cargo.lock |
  sort -u`; it must print one line.
- The iced clone is blob-filtered: `git show <rev>:<path>` works, but the
  libcosmic clone's `iced/` submodule directory is empty.
- The ext-brightness clone pins a July libcosmic (99 commits behind); its popup
  code is a pattern reference, not a compile reference.
- Upstream pins `panic = "abort"` and a nightly rustfmt; both remain choices
  this project deliberately does not copy.

## 9. Refreshing these clones next time

```sh
# Applets: tag (release baseline) or master (newest API usage)
cd tmp_code_examples/cosmic_examples/cosmic-applets
git fetch --tags --prune origin
git checkout -B epoch-1.11.0 epoch-1.11.0

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
  git -C ../cosmic_examples/$d checkout --detach "epoch-1.11.0"
done
```

The template, toot, and external-brightness clones need no refresh until their
`PanelConfig`, settings, or subscription patterns come into play.

The non-COSMIC references move independently:

```sh
git -C ../vaultwarden pull --ff-only
git -C ../bitwarden-official-clients fetch --depth=1 origin main
git -C ../bitwarden-official-clients checkout --detach origin/main
```

`rbw_reference` tracks a fork whose upstream no longer resolves; leave it
unless a different `rbw` mirror appears.
