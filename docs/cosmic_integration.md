# Integration with COSMIC DE

How cosmic-bwarden registers with the COSMIC desktop. This documents what
`just install` / `just user-install` actually do (the justfile is the source of
truth).

## Application ID

Everything keys off the ID **`com.system76.CosmicBWarden`** (desktop entry,
applet metadata, `StartupWMClass`, `CONFIG_ID` in `core/src/config.rs`).

> ⚠️ Pre-publish task (see `docs/roadmap.md`): this squats System76's RDNN
> namespace. Before public release the ID must move to a namespace we control
> (e.g. `io.github.<owner>.CosmicBWarden`), with a config migration for
> `CONFIG_ID`.

## What gets installed

| Artifact | Source | Installed to |
|---|---|---|
| Applet/app binary `cosmic-applet-bwarden` | built from the `cosmic-bwarden-ui` crate | `{bin_dir}` |
| Desktop entry | `crates/cosmic-bwarden-ui/resources/com.system76.CosmicBWarden.desktop` | `{apps_dir}` |
| Applet metadata (`.ron`) | generated inline by the justfile | `{applets_dir}/com.system76.CosmicBWarden.ron` |
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

The applet currently uses the theme icon `password-manager-symbolic`
(`view/applet/mod.rs`). Branded symbolic icons (repo `icons/black.svg` /
`white.svg`) are a tracked polish task — panel icons should be recolorable
symbolic SVGs, not the PNG set (those serve the browser extension).

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
