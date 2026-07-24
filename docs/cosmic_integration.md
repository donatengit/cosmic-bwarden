# Integration with COSMIC DE

How cosmic-bwarden registers with the COSMIC desktop. This documents what
`just install` / `just user-install` actually do (the justfile is the source of
truth).

## Application ID

Everything keys off the ID **`com.enikeev.cosmic_bwarden`** (desktop entry,
applet metadata, `StartupWMClass`, `CONFIG_ID` in `core/src/config.rs`).

> This is a **temporary** ID pending publish. `config_dir()`/`data_dir()` key
> off a separate `profile()` string (`core/src/dirs.rs`), not `CONFIG_ID`, so
> `config.json` survives an ID change untouched; only keyring entries
> (`agent/src/keyring.rs` `APP_ID`) are scoped to the ID and would need
> re-login after a future rename.

## What gets installed

| Artifact | Source | Installed to |
|---|---|---|
| Applet/app binary `cosmic-applet-bwarden` | built from the `cosmic-bwarden-ui` crate | `{bin_dir}` |
| Desktop entry | `crates/cosmic-bwarden-ui/resources/com.enikeev.cosmic_bwarden.desktop` | `{apps_dir}` |
| Applet metadata (`.ron`) | generated inline by the justfile | `{applets_dir}/com.enikeev.cosmic_bwarden.ron` |
| Agent systemd user unit | `crates/cosmic-bwarden-agent/res/cosmic-bwarden-agent.service` (hardened; `@BINDIR@` substituted) | `{systemd_user_dir}` |
| Firefox native-messaging host | `tests/browser-extension/register_host.py` | `~/.mozilla/native-messaging-hosts/` |

The desktop entry carries the applet markers COSMIC's panel looks for:
`X-CosmicApplet=true`, `X-CosmicHoverPopup=Auto`, `OnlyShowIn=COSMIC;`, plus
`NoDisplay` is *not* set so the app is also launchable as a normal window.

## One binary, two modes

`cosmic-applet-bwarden` runs as a **panel applet** when the panel launches it
(`COSMIC_PANEL_NAME` is set in the environment) and as a **full application
window** otherwise. "Open Vault Window" from the applet spawns a second
instance with `COSMIC_BWARDEN_MODE=application` and `COSMIC_PANEL_NAME`
removed. Both talk to the same `cosmic-bwarden-agent` over the Unix socket.

## Panel icon

The applet uses the repo's brand mark as a symbolic icon
(`resources/icons/cosmic-bwarden-symbolic.svg`, the drawable content of the
repo-root `icons/black.svg` with design-tool export metadata stripped),
embedded at compile time via
`icon::from_svg_bytes(...).symbolic(true)` and rendered through
`applet.icon_button_from_handle()` (`view/applet/mod.rs`). Embedding avoids any
install-time dependency on the system icon theme — the panel button renders
correctly in dev builds too — and `symbolic(true)` makes libcosmic recolor it
to match the panel's light/dark foreground automatically, no separate
black/white variants needed.

The `.ron` `icon:` field still references the generic theme name
`password-manager-symbolic` (used only for the panel-applet listing in COSMIC
Settings) — that one's fine as a placeholder since it's rendered small and
recolored like the panel button.

The `.desktop` `Icon=` field is different: it's what the window manager/dock
resolves for the "big" window/taskbar icon when the app runs standalone, so it
needs a real installed icon, not a generic theme name. `just install` /
`clean-install` / `user-install` install the repo's full detailed brand mark
(`icons/black.svg`, plus `black{16,32,64,128}.png` — the same source as
`FULL_ICON_SVG` in `view/style.rs`, not the simplified panel glyph) into the
hicolor icon theme as `com.enikeev.cosmic_bwarden`, and `Icon=` in the
`.desktop` file points at that name. `cargo deb`'s asset list
(`crates/cosmic-bwarden-ui/Cargo.toml`) mirrors this same hicolor layout.

### No light/dark variant for the standalone icon (and why)

`icons/white.svg`/`white{16,32,64,128}.png` exist in the repo (used by the
**browser extension**'s toolbar icon, which does its own theme detection in
JS — see `docs/browser_integration.md`), but they are **not** installed as an
alternate `com.enikeev.cosmic_bwarden` for dark theme, and that's
intentional, not an oversight: unlike the panel button's `symbolic(true)`
runtime recolor (a libcosmic/iced feature that only applies to icons *we*
render ourselves), there is no OS-level mechanism that swaps a plain hicolor
app icon by system light/dark preference — confirmed against
`tmp_code_examples/cosmic_examples/cosmic-settings` and `toot`, both of which
ship exactly one icon for their standalone app identity, no pair. Trying to
install `white.svg` under the same `com.enikeev.cosmic_bwarden` name would
just silently never be selected by anything.

What *is* real: the freedesktop `-symbolic` suffix convention, which some
consumers (app-icon pickers, launchers/search results, anything that renders
it the way we render our own panel button) will look up and recolor
correctly. `just install`/`clean-install`/`user-install`/`cargo deb` all
additionally install the existing embedded panel glyph
(`resources/icons/cosmic-bwarden-symbolic.svg` — no new artwork needed, it's
the same monochrome mark already used for the panel button) as
`com.enikeev.cosmic_bwarden-symbolic.svg` in hicolor's `scalable/apps/`, so
that recolor-capable path actually exists. The primary `Icon=` still points
at the single full-color `black.svg`-derived icon, matching every other
COSMIC app's convention for the non-recolored case.

## After installing

```bash
just enable-agent     # systemd --user enable + start cosmic-bwarden-agent
just restart-panel    # restart the COSMIC panel so it discovers the applet
```

Then add the applet through COSMIC Settings → Desktop → Panel → Configure
panel applets.

## Custom servers & "Remember Me"

The login screen supports a custom server URL (official Bitwarden, Vaultwarden,
or enterprise installs). "Remember Email" persists the email in `config.json`
so later launches only prompt for the master password.
