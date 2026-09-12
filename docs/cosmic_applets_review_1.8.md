# Upstream applet review: `pop-os/cosmic-applets` at `epoch-1.8.0`

Reference material for the applet side of `cosmic-bwarden-ui`, refreshed for
COSMIC Epoch 1.8. This document records the revisions held locally, what
changed upstream since the previous snapshot, and which upstream practices are
worth copying. The code-level comparison of our applet against the upstream
examples stays in [`applet_reference_comparison.md`](applet_reference_comparison.md);
this file covers revisions, change history, and build/process practices.

All paths below are relative to the repository root unless stated otherwise.
The reference clones live under `tmp_code_examples/`, which is gitignored
(`.gitignore:77`) — nothing here is a build input.

## 1. Local reference checkouts

| Path | Revision | Date | Notes |
|---|---|---|---|
| `tmp_code_examples/cosmic_examples/cosmic-applets` | `b933e538c043fbece3f6ecc432538608b3382819` | 2026-09-04 | Branch `epoch-1.8.0`, the tag of the same name. Contains every applet in the repository. |
| `tmp_code_examples/libcosmic` | `53314201b08b7ff3758b5d1222c4d214abfb5dd8` | 2026-09-11 | Detached HEAD, matching the revision `Cargo.lock` builds against. |
| `tmp_code_examples/cosmic_examples/cosmic-panel`, `cosmic-settings`, `cosmic-notifications`, `cosmic-applet-template`, `toot`, `cosmic-ext-applet-external-monitor-brightness` | unchanged | — | Not part of this refresh; see section 8 for how to bring them forward. |

Previous state of the applets clone was `c7b1cb35` (2026-08-21, `master`).
Upstream `master` at fetch time was `e01b16f4` (2026-09-10), one commit past the
tag — the reason the tag rather than `master` is checked out is explained in
section 5.

Refresh procedure:

```sh
cd tmp_code_examples/cosmic_examples/cosmic-applets
git fetch --tags --prune origin
git checkout -B epoch-1.8.0 epoch-1.8.0      # or: git checkout -B master origin/master
```

## 2. What the repository contains

Fourteen applets, three panel buttons, and shared crates in one workspace. The
`.desktop` `Name=` field is what the panel menu shows, and it is the fastest way
to tell which applet is which:

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
| `cosmic-applet-power` | User Session |
| `cosmic-applet-status-area` | Notifications Tray |
| `cosmic-applet-tiling` | Tiling |
| `cosmic-applet-time` | Date, Time & Calendar |
| `cosmic-applet-workspaces` | Numbered Workspaces |

Plus `cosmic-panel-app-button`, `cosmic-panel-launcher-button`,
`cosmic-panel-workspaces-button` (panel buttons with their own `.desktop`
files), and three crates that are not applets themselves: `cosmic-applets`
(`src/main.rs`, the binary that hosts applets), `cosmic-panel-button`
(`src/lib.rs` plus `config.rs` and a demo `main.rs`), and
`cosmic-applets-config` (`src/lib.rs` plus `battery.rs`, `time.rs`).

The directory set at the tag and at `master` is identical — no applet was added
or removed between our previous snapshot and today.

## 3. libcosmic revision map

| Consumer | libcosmic revision | Date |
|---|---|---|
| cosmic-applets `epoch-1.8.0` (this clone) | `d4d71fd5` | 2026-09-04 |
| cosmic-applets `master` | `a401af8b` | 2026-09-10 |
| cosmic-bwarden (`Cargo.lock`) | `53314201b` | 2026-09-11 |

`d4d71fd5` is an ancestor of our `53314201b`, so the 1.8-era applets build
against a libcosmic that is **17 commits older** than ours, inside the same API
era. Those 17 commits are not cosmetic: they carry the popup and menu lifecycle
fixes this project depends on —

- `a8fe59f38` refactor(surface): type surface actions on the message they carry
- `9f634b0ec` fix(menu): give each popup instance its own id
- `6081a48cd` fix(app): count a popup closed once now that `Done` also reaches its parent
- `a401af8b` fix(wayland): deliver popup `Done` to the parent window's widgets too
- `0304f1d92` fix: detect SVG icons by content

The consequence for reference work: the tag is the right baseline for *applet
structure*, but its surface/popup code is pre-refactor and does not compile
against our libcosmic. For anything involving `Message::Surface`, read
`master`, not the tag.

## 4. What changed since the previous snapshot

`c7b1cb35..epoch-1.8.0` is five commits, 21 files, +57/−43:

| Commit | Date | Change |
|---|---|---|
| `b933e538` | 2026-09-04 | `chore: update libcosmic to fix downsampled raster icon rendering` — moves the lock to `d4d71fd5`. |
| `b939b809` | 2026-09-05 | `fix(network): bump nmrs to 3.5.2` — dependency and 8 lines in `cosmic-applet-network/src/app.rs`. |
| `1aa03111` | 2026-09-01 | Translation update from Hosted Weblate. |
| `4dd06487` | 2026-08-30 | Translation update from Weblate. |
| `ab9d0699` | 2026-08-28 | `feat(bluetooth): show bluetooth alias name (#1534)` — 16 lines in `cosmic-applet-bluetooth/src/bluetooth.rs`. |

The two translation commits register a `fil` (Filipino) locale for translation
tooling: 12 new applet `i18n/fil/*.ftl` files plus `i18n/fil/desktop_entries.ftl`,
all 13 of them empty (0 bytes), so no Filipino strings exist yet. Real string
changes land in `ru`, `nl`, `pt-BR`, and `el`. Nothing in this range changes
applet architecture; it is dependency and translation churn plus one small
feature.

## 5. What changed after the 1.8 tag

`master` is one commit ahead of the tag, and that commit is the migration this
project just made:

`e01b16f4` (2026-09-10) `chore: adapt to changes in libcosmic` — 16 applet
source files, +51/−80, mechanically the same two edits in every applet:

```rust
// before
Surface(surface::Action),
return cosmic::task::message(cosmic::Action::Cosmic(cosmic::app::Action::Surface(a)));
// after
Surface(surface::Action<Message>),
return cosmic::task::message(cosmic::Action::Surface(a));
```

`cosmic-bwarden` is already on the after-state
(`crates/cosmic-bwarden-ui/src/message/mod.rs`,
`crates/cosmic-bwarden-ui/src/app/update/applet.rs`), which is why the tag is
the reference baseline here and `master` is not needed for it. Two facts make
the tag preferable as a checkout: it pairs with a libcosmic revision whose whole
graph resolves (section 3), and `master`'s revision `a401af8b` predates ours, so
reading it would show older popup handling than we build against.

## 6. Practices worth adopting

### 6.1 A `[patch]` redirect that unifies the cosmic-protocols pair

Upstream declares its protocols dependencies with one revision and then forces
every source from that URL to a single revision with a `[patch]` section whose
replacement URL ends in `//` (`Cargo.toml`, `[patch."https://github.com/pop-os/cosmic-protocols"]`):

```toml
[patch."https://github.com/pop-os/cosmic-protocols"]
cosmic-protocols = { git = "https://github.com/pop-os/cosmic-protocols//", rev = "32283d7" }
cosmic-client-toolkit = { git = "https://github.com/pop-os/cosmic-protocols//", rev = "32283d7" }
```

Their `Cargo.lock` (at `master`) then contains exactly two entries from one
source, `git+https://github.com/pop-os/cosmic-protocols//?rev=32283d7#32283d76`,
even though the workspace dependencies ask for a different revision and
libcosmic's own manifest pins a third one.

`CONTEXT.md` and `Cargo.toml` currently state that no `[patch]` can repair a
split in this dependency family. That is too strong: what fails is a patch whose
replacement URL is *identical* to the patched URL (Cargo: "patches must point to
different sources"), which is the form this project tried. The trailing `//`
makes the replacement a different source string while pointing at the same
repository. Adopting it would allow tracking libcosmic `master`, including the
tip (`7e82198ab`, whose bundled iced pins `cosmic-client-toolkit` to `c0cff4d`)
without the duplicate that forced the current pin.

Status: **not verified in this repository.** The evidence is upstream's lock, not
our build. A scratch branch would settle it in one step: add the stanza, run
`cargo update -p libcosmic --precise 7e82198ab1aba712640b0167e58ad37bf03012ca`,
then check that `grep -c 'cosmic-protocols' Cargo.lock` shows a single source and
that `cargo check --workspace --all-targets` passes. If it works, the paragraphs
in `CONTEXT.md` and `Cargo.toml` need correcting, not just extending.

### 6.2 Validate `.desktop` files in CI

Upstream runs `desktop-file-validate` over every `*.desktop` in the repository
(`.github/workflows/validate-desktop-files.yml`, an `ubuntu:25.10` container with
`desktop-file-utils` and `findutils`). This project ships exactly one such file,
`crates/cosmic-bwarden-ui/resources/com.enikeev.cosmic_bwarden.desktop`, plus the
applet's `.ron` metadata, and nothing checks either. A malformed key or a missing
`StartupWMClass` breaks panel registration silently — the failure appears as "the
applet does not show up", not as a build error.

Verdict: **adopt.** One `just` recipe (`command -v desktop-file-validate`
guarded, so a missing tool is a skip rather than a failure) plus a CI job keeps
the same property upstream has.

### 6.3 Format and lint gates in CI — already in place

Upstream's `ci.yml` runs `cargo +nightly fmt --all -- --check` and clippy over
`--all --all-targets --all-features`. This project's `.github/workflows/ci.yml`
runs `cargo fmt --check`, `cargo clippy --workspace --all-targets`, and a second
clippy pass for `-p cosmic-bwarden-agent --features tpm` on stable. Nothing to
copy; the only difference is upstream's nightly rustfmt, which brings formatting
changes that stable rustfmt rejects — not worth the churn for one contributor
and no nightly CI.

### 6.4 `[workspace.dependencies]` — already in place

Upstream declares `libcosmic`, `i18n-embed`, `i18n-embed-fl`, `zbus`,
`rust-embed`, `cctk`, and the protocols crates once in
`[workspace.dependencies]`. This project has the same section in `Cargo.toml`.
No action.

### 6.5 Release profile: ours is already stricter

| Setting | Upstream | cosmic-bwarden |
|---|---|---|
| `opt-level` | `3` | `"z"` |
| `lto` | `"thin"` | `"fat"` |
| `codegen-units` | default | `1` |
| `strip` | not set | `true` |
| `panic` | `"abort"` | default (`"unwind"`) |

The one upstream choice worth a deliberate look is `panic = "abort"`, which
drops the landing pads and shrinks the binary. This project leaves it at
`unwind` on purpose (see the comment in `Cargo.toml`): the agent is a
long-running daemon whose panic path must still produce a usable log and let
systemd restart it, and unwinding keeps `catch_unwind` usable in the IPC server.
Verdict: **keep ours.**

### 6.6 `rust-toolchain.toml`

Upstream pins `channel = "1.93"` (`rust-toolchain.toml`) and CI builds with
`1.93.1`. This project builds with whatever the distribution provides (1.98.1 in
the development environment on this machine) and packages for Arch and deb,
where a pinned channel fights the system toolchain and adds a rustup dependency
to the build instructions. Verdict: **skip**; state a minimum supported Rust
version in `docs/build_and_run.md` instead if drift ever becomes a problem.

### 6.7 Translations: Weblate and localized `.desktop` entries

Upstream's translation updates arrive as automated commits from Hosted Weblate,
and they cover panel-visible strings too: `i18n/<locale>/desktop_entries.ftl`
holds localized `Name`/`Comment` output for every applet, on top of the
per-crate `i18n/<locale>/<crate>.ftl` files. This project has one locale
(`crates/cosmic-bwarden-ui/i18n/en/cosmic_bwarden_ui.ftl`) and no translation
pipeline; the panel menu therefore reads "COSMIC BWarden" in every locale.

Verdict: **worth doing, but it depends on a product decision, not on this
review.** Registering the crate on a translation platform costs nothing until
translators appear, and the Fluent plumbing is already in place. Two caveats:
the display name is pinned to `COSMIC BWarden` across every layer
(`AGENTS.md`, "Naming"), so a localized `Name=` needs an explicit exception to
that rule; and `.desktop` files are installed statically, so localized names
mean either `Name[xx]=` keys generated from the FTL files or upstream's
Fluent-at-runtime approach.

### 6.8 Vendoring recipes

Upstream has `just vendor` and `just vendor-extract`, which produce and unpack a
`vendor.tar` for offline and distribution builds. This project depends on git
sources (libcosmic, cosmic-protocols, winit, and others), so any packaging that
must build without network access — a source tarball for an AUR or deb build
service — needs exactly this. Verdict: **adopt only when packaging requires it**;
nothing in `packaging/` needs it today.

### 6.9 `cargo-machete` metadata

Upstream's workspace manifest carries
`[workspace.metadata.cargo-machete] ignored = ["libcosmic"]`, which records why a
dependency that looks unused is not. This project has no unused-dependency check;
if one is added, the same `ignored` list is needed for `libcosmic` and for the
feature-gated crates (`tss-esapi` under `--features tpm`). Verdict: **optional
developer tool**, no CI value today.

### 6.10 Install machinery keyed by app ID

Upstream's `justfile` installs each applet through a helper taking the app ID and
the binary name (`_install_applet 'com.system76.CosmicAppletTime' 'cosmic-applet-time'`),
and installs a *default config schema* alongside one applet
(`_install_default_schema cosmic-app-list`) — the panel then has a valid config
before the first run. This project's `just install` / `just user-install` do not
ship a default schema; the applet writes its own config on first use. Verdict:
**minor**; worth revisiting only if a user reports a panel entry that needs
manual setup.

### 6.11 Testing culture: do not imitate

The whole upstream repository contains four `#[test]` functions. The 16-line
Bluetooth alias feature in section 4 shipped without one. This project runs 375
unit tests and 86 E2E tests (`just test`). The asymmetry is deliberate and should
stay: upstream compensates with manual QA across a desktop session, which this
project cannot do from CI. When reading upstream code for ideas, read it for
structure, and write the tests ourselves.

## 7. Cautions

- The tag's surface and popup code predates `a8fe59f38` and does not compile
  against our libcosmic (section 3). Copying snippets from the tag will not
  compile.
- Upstream's `[patch]` block also redirects `smithay/client-toolkit` to the
  crates.io `0.20.0` release. That is unrelated to the cosmic-protocols fix and
  not needed here.
- Upstream pins `panic = "abort"`; forcing it onto this workspace would change
  the agent's failure behaviour (section 6.5).
- A locale directory appearing in upstream does not mean it is translated: the
  `fil` tree added in this range is empty in every applet (section 4).

## 8. Refreshing these clones next time

```sh
# Applets: tag (release baseline) or master (newest API usage)
cd tmp_code_examples/cosmic_examples/cosmic-applets
git fetch --tags --prune origin
git checkout -B epoch-1.8.0 epoch-1.8.0
# Applet set unchanged against master? (no output means identical)
diff <(git ls-tree --name-only -d HEAD | sort) \
     <(git ls-tree --name-only -d origin/master | sort)

# libcosmic: keep in step with what the project builds against
cd ../../libcosmic
git fetch --tags origin
git checkout --detach "$(grep -A2 '^name = "libcosmic"' ../../Cargo.lock | grep -o '[0-9a-f]\{40\}')"
```

The other reference clones (`cosmic-panel`, `cosmic-settings`,
`cosmic-notifications`, `cosmic-applet-template`, `toot`,
`cosmic-ext-applet-external-monitor-brightness`) were left untouched by this
refresh; bring them forward with the same `fetch` + `checkout` pair when their
`PanelConfig`, settings, or notification APIs come into play.

## 9. Which of the newer libcosmic patches matter to us

The 17 commits in `d4d71fd5..53314201b` (section 3) are not uniformly relevant.
Below, each one is classified by what it does for *this* project. Reproduce any
of them with `git show <hash>` inside `tmp_code_examples/libcosmic`.

### 9.1 Compatibility

- `a8fe59f38` `refactor(surface): type surface actions on the message they carry`
  — the breaking change our migration paid for: `surface::Action<Message>` in
  `crates/cosmic-bwarden-ui/src/message/mod.rs` and the root
  `cosmic::Action::Surface` in `crates/cosmic-bwarden-ui/src/app/update/applet.rs`.
  Nothing optional remains about it; the pin does not compile without the
  change. The typing is a small win on top: a surface action carrying another
  app's message type is no longer representable.
- `0304f1d92` `fix: detect SVG icons by content` — reaches our theme icon
  lookups, `icon::from_name("…-symbolic")` in `view/applet/menu.rs`,
  `view/applet/search.rs`, and `view/applet/unlock.rs`, which resolve through
  the system icon theme. Our own applet icon is explicitly declared as SVG
  (`icon::from_svg_bytes`, `view/applet/mod.rs`), so only the theme path
  benefits: an SVG shipped under a name without an `.svg` suffix now renders.

### 9.2 Maintainability: two heuristics in our applet may now be removable

`Message::AppletIconClicked` in
`crates/cosmic-bwarden-ui/src/app/update/applet.rs` carries two workarounds
whose own comments name the failure they defend against: it `take()`s the popup
id eagerly "instead of waiting for `WindowClosed`", because a stale `Some(id)`
"would swallow every subsequent click"; and it suppresses a re-open inside
`APPLET_POPUP_REOPEN_DEBOUNCE_MS` (200 ms, `app/update/applet/helpers.rs`)
because one physical press could arrive twice.

Both are symptoms of unreliable popup close accounting, which this commit range
fixes:

- `a401af8b` `fix(wayland): deliver popup Done to the parent window's widgets too`
  bumps the bundled iced submodule (`51118067` → `ffe1f1db`) so `Done` reaches the
  window that owns the popup, not only the popup surface.
- `6081a48cd` `fix(app): count a popup closed once now that Done also reaches its
  parent` makes `src/app/cosmic.rs` report `SurfaceClosed` only when the closed
  popup id equals the app's own surface id. Before it, *any* popup or layer
  `Done` was reported as `SurfaceClosed(our id)`, so closing an unrelated surface
  could make the applet believe its popup had gone.
- `9f634b0ec` `fix(menu): give each popup instance its own id` removes the other
  half: the popup and the toasts stacked inside it (`toaster`,
  `view/applet/mod.rs`) no longer share ids.

Action: re-test the popup on this pin — open, close, reopen, dismiss with
Escape, click away, with a toast visible — and if the state machine holds
without them, delete the debounce and the eager `take()`. That removes a
timestamp field, one helper, and its tests. The commit range alone is not proof
about compositor behaviour, so the heuristics stay until that manual pass
happens; they are harmless while the events underneath are correct.

### 9.3 User-visible

| Commit | What changes for a user of our applet |
|---|---|
| `53314201b` `fix(theme): draw selected label text in the on-accent color` | The selected label of our `segmented_button`s (`view/vault/generator.rs`, `view/vault/sidebar.rs`) was drawn in a colour that could match the accent fill. This is our pin's tip commit. |
| `7cc116803` → `eec2f7619` → `ff7c75330` (`text_input` selected text) | Selection colour in our PIN, master-password, and search fields: the first fix was reverted the same day and redone as `ff7c75330`, which uses the theme's `selected_text_color`. Pinning at either earlier revision would ship the version upstream rejected. |
| `56a210bb4` `feat(text_context_menu): always show items, but disable them` | Right-click inside our text fields shows Cut/Copy/Paste greyed out instead of dropping unavailable entries. We never construct this menu — `src/widget/text_input/input.rs` does — so the fix arrives through the widget. |
| `376f1b885` `fix(segmented_button): read the live menu state when drawing the context item` | The context item on our segmented buttons is drawn from live state instead of a stale snapshot. |
| `521f3c2ba`, `37c392a47`, `645ca8b90`, `2fb052d13`, `aac833d37`, `fd20f0972` | Context-menu robustness: no panic when no menu is set, overlays/cursor/drop-target pass-through, an icon slot and a width setter on `menu::Item`, open/close reporting. We build no context menus, so these reach us only through widget internals; the X11 placement fix does not apply to a Wayland session. |

### 9.4 From the applets repository itself

- `b933e538` (`chore: update libcosmic to fix downsampled raster icon rendering`)
  is the same class of benefit as `0304f1d92`, for the case that matters here:
  raster icons from the system theme scaled down to panel size — what
  `icon::from_name` does at 16–24 px — sample better. Their reason for the bump
  is ours: keep libcosmic current.
- `ab9d0699` (`feat(bluetooth): show bluetooth alias name`) is the only
  product-level change in the five commits: prefer a user-facing alias, fall
  back to the technical name. No label in our UI has that pair today, so it is a
  pattern to remember rather than something to port.
- The two translation commits, the `nmrs` bump, and the adapt commit
  (`e01b16f4`) hold nothing for us beyond sections 4 and 5.
