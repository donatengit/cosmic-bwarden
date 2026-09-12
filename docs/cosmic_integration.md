# Integration with COSMIC DE

How cosmarden registers with the COSMIC desktop. This documents what
`just install` actually does (the justfile is the source of truth). Install
is user-local: `~/.local/bin`, `~/.local/share`, `~/.config/systemd/user`.
Deb/AUR packages remain the system-wide path.

## Application ID

Everything keys off the ID **`com.enikeev.cosmarden`** (desktop entry,
applet metadata, `StartupWMClass`, `CONFIG_ID` in `core/src/config.rs`).

> This is a **temporary** ID pending publish. `config_dir()`/`data_dir()` key
> off a separate `profile()` string (`core/src/dirs.rs`), not `CONFIG_ID`, so
> `config.json` survives an ID change untouched; only keyring entries
> (`agent/src/keyring.rs` `APP_ID`) are scoped to the ID and would need
> re-login after a future rename.

## What gets installed

| Artifact | Source | Installed to (`just install`) |
|---|---|---|
| Applet/app binary `cosmarden-applet` | built from the `cosmarden-ui` crate | `~/.local/bin` |
| Desktop entry | `crates/cosmarden-ui/resources/com.enikeev.cosmarden.desktop` | `~/.local/share/applications` |
| Applet metadata (`.ron`) | generated inline by the justfile | `~/.local/share/cosmic/applets/com.enikeev.cosmarden.ron` |
| Agent systemd user unit | `crates/cosmarden-agent/res/cosmarden-agent.service` (hardened; `@BINDIR@` substituted) | `~/.config/systemd/user` |
| Firefox native-messaging host | `tests/browser-extension/register_host.py` | `~/.mozilla/native-messaging-hosts/` |

The desktop entry carries the applet markers COSMIC's panel looks for:
`X-CosmicApplet=true`, `X-CosmicHoverPopup=Auto`, `OnlyShowIn=COSMIC;`, plus
`NoDisplay` is *not* set so the app is also launchable as a normal window.

COSMIC Settings lists applets by scanning XDG desktop files
(`~/.local/share/applications` is on that list), which is why a user-local
install still appears in the picker. The panel then spawns `Exec=` with
`Process::with_executable` using **cosmic-panel's** PATH (systemd user
default: `/usr/local/bin:/usr/bin` — not `~/.local/bin`). `just install`
therefore rewrites `Exec=` to the absolute `~/.local/bin/cosmarden-applet`
path. Distro packages leave the unqualified name, because `/usr/bin` is on
that PATH.

## One binary, two modes

`cosmarden-applet` runs as a **panel applet** when the panel launches it
(`COSMIC_PANEL_NAME` is set in the environment) and as a **full application
window** otherwise. "Open Vault Window" from the applet spawns a second
instance with `COSMARDEN_MODE=application` and `COSMIC_PANEL_NAME`
removed. Both talk to the same `cosmarden-agent` over the Unix socket.

## Panel icon

Applet action icons (search-row open/link/copy, TPM pass/fail, quit
disclosure) are Cosmic `-symbolic` names listed in
[`docs/icon_guidelines.md`](icon_guidelines.md). This section covers only
the panel/app identity mark.

The applet uses the repo's brand mark as a symbolic icon
(`resources/icons/cosmarden-symbolic.svg`, the drawable content of the
repo-root `icons/black.svg` with design-tool export metadata stripped),
embedded at compile time via
`icon::from_svg_bytes(...).symbolic(true)` and rendered through
`applet.icon_button_from_handle()` (`view/applet/mod.rs`). Embedding avoids any
install-time dependency on the system icon theme — the panel button renders
correctly in dev builds too — and `symbolic(true)` makes libcosmic recolor it
to match the panel's light/dark foreground automatically, no separate
black/white variants needed.

The `.ron` `icon:` field references our own `com.enikeev.cosmarden-symbolic`
(used for the panel-applet listing in COSMIC Settings) — the same symbolic mark
the panel button uses, rendered small and recolored like the panel button.

The `.desktop` `Icon=` field is different: it's what the window manager/dock
resolves for the "big" window/taskbar icon when the app runs standalone, so it
needs a real installed icon, not a generic theme name. `Icon=` in the
`.desktop` file points at `com.enikeev.cosmarden-symbolic`, the
simplified monochrome mark installed into hicolor `scalable/apps/` by
`just install` / `clean-install` / `user-install`, so COSMIC Launcher, Dock,
and app switchers recolor it per theme. The full detailed brand mark
(`icons/black.svg`, plus `black{16,32,64,128}.png` — the same source as
`FULL_ICON_SVG` in `view/style.rs`, not the simplified panel glyph) is
additionally installed into hicolor as `com.enikeev.cosmarden` for
contexts that don't recolor symbolic icons. `cargo deb`'s asset list
(`crates/cosmarden-ui/Cargo.toml`) mirrors this same hicolor layout.

### No light/dark variant for the standalone icon (and why)

`icons/white.svg`/`white{16,32,64,128}.png` exist in the repo (used by the
**browser extension**'s toolbar icon, which does its own theme detection in
JS — see `docs/browser_integration.md`), but they are **not** installed as an
alternate `com.enikeev.cosmarden` for dark theme, and that's
intentional, not an oversight: unlike the panel button's `symbolic(true)`
runtime recolor (a libcosmic/iced feature that only applies to icons *we*
render ourselves), there is no OS-level mechanism that swaps a plain hicolor
app icon by system light/dark preference — confirmed against
`tmp_code_examples/cosmic_examples/cosmic-settings` and `toot`, both of which
ship exactly one icon for their standalone app identity, no pair. Trying to
install `white.svg` under the same `com.enikeev.cosmarden` name would
just silently never be selected by anything.

What *is* real: the freedesktop `-symbolic` suffix convention, which some
consumers (app-icon pickers, launchers/search results, anything that renders
it the way we render our own panel button) will look up and recolor
correctly. `just install`/`clean-install`/`user-install`/`cargo deb` all
additionally install the existing embedded panel glyph
(`resources/icons/cosmarden-symbolic.svg` — no new artwork needed, it's
the same monochrome mark already used for the panel button) as
`com.enikeev.cosmarden-symbolic.svg` in hicolor's `scalable/apps/`. The primary `Icon=` in the desktop file points at `com.enikeev.cosmarden-symbolic` so COSMIC Launcher, Dock, and app switchers dynamically recolor the icon for light/dark system themes (matching COSMIC applets like `cosmic-applet-network` and `cosmic-applet-audio`).

## After installing

```bash
just enable-agent     # systemd --user enable + start cosmarden-agent
just restart-panel    # restart the COSMIC panel so it discovers the applet
```

Then add the applet through COSMIC Settings → Desktop → Panel → Configure
panel applets.

## Custom servers & "Remember Me"

The login screen supports a custom server URL (official Bitwarden, Vaultwarden,
or enterprise installs). "Remember Email" persists the email in `config.json`
so later launches only prompt for the master password.
