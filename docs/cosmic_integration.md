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

The `.desktop`/`.ron` `Icon=`/`icon:` fields still reference the theme name
`password-manager-symbolic` (used for the launcher/app-switcher context, not
the panel button); installing a dedicated icon into the hicolor theme for
those is separate, tracked polish (`docs/roadmap.md` UX backlog).

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
