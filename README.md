# cosmic-bwarden

A native **Bitwarden / Vaultwarden client for the COSMIC™ desktop** — panel applet,
background agent, CLI, SSH agent, and browser extension, written in Rust on
[libcosmic](https://github.com/pop-os/libcosmic).

Point it at bitwarden.com, bitwarden.eu, or your self-hosted Vaultwarden.

## Why it exists

The official Bitwarden desktop app is an Electron window. COSMIC deserves a client
that lives in the panel, starts instantly, speaks the desktop's design language, and
keeps secrets in memory-locked storage under your control. One agent process owns the
unlocked vault; everything else — applet, vault window, CLI, browser extension,
`ssh-agent` — is a thin client over a local socket.

## Features

- **Panel applet**: search-as-you-type over your vault, favourites first, one-click
  copy of usernames/passwords/notes, inline unlock, TOTP-aware entries.
- **Vault window**: full CRUD for logins, secure notes, cards, identities, and SSH
  keys, with real-time server sync and master-password reprompt for gated items.
- **Background agent**: holds vault keys in `mlock`ed, zeroize-on-drop memory; locks
  on suspend/shutdown/session-lock (logind) and on an inactivity timeout; core dumps
  disabled; hardened systemd user unit.
- **SSH agent**: keys stored in your vault are served over the standard ssh-agent
  protocol — `ssh` works while the vault is unlocked, and stops when it locks.
- **CLI**: scriptable `list / get / add / edit / delete` with `KEY=VALUE` syntax.
- **Browser extension** (Firefox/Chrome): domain-matched entries, form fill, on-demand
  secret reveal — secrets never sit in extension state from passive browsing.
- **TPM PIN unlock** (optional, `--features tpm`): vault keys sealed in TPM2 bound to
  firmware + Secure Boot state (PCR 0/7), guarded by a PIN under the TPM's own
  anti-hammering lockout.

## Install

Prerequisites: Rust toolchain, `just`, and libcosmic's system deps
(`libxkbcommon-dev libwayland-dev libegl1-mesa-dev pkg-config cmake`). Details in
[docs/build_and_run.md](docs/build_and_run.md).

```sh
git clone <repo-url> && cd cosmic-bwarden   # submodules NOT required
just user-install    # to ~/.local (no sudo); or: sudo just install
just enable-agent    # systemd --user enable + start the agent
just restart-panel   # let the COSMIC panel discover the applet
```

A `.deb` is produced by `cargo deb -p cosmic-bwarden-ui --no-build` after
`just build`; an AUR `PKGBUILD` lives in [`packaging/`](packaging/).

## First run

1. Add the applet: COSMIC Settings → Desktop → Panel → Configure panel applets →
   **CosmicBWarden**.
2. Click the panel icon → **Open Vault Window** → log in (email, master password, and
   optionally your self-hosted server URL under *Advanced*).
3. That's it — search from the applet, copy with one click. The agent keeps the vault
   available (locked) across reboots; you only re-enter the master password, never
   the full login.

Browser extension: `just pack-extension`, load `target/cosmic-bwarden-extension.zip`,
and register the native host with `just register-browser-host` (dev) or `just install`
(system).

## Security model, honestly

- The **agent** is the only process holding secrets: keys live in memory-locked
  (`mlock`) buffers, zeroized on lock/drop; `PR_SET_DUMPABLE=0`; sockets are `0600`
  with per-connection same-UID (`SO_PEERCRED`) checks.
- The **on-disk cache** stores only server-encrypted data (AES-256-CBC +
  HMAC-SHA256, encrypt-then-MAC, verified before decrypt). Session tokens go to the
  Secret Service keyring, never to the JSON cache.
- **Out of scope**: a hostile process running *as your user* can ultimately reach the
  agent (same-UID is the trust boundary on a single-user desktop — reprompts raise
  the cost, they are not a wall). Unencrypted swap can page session tokens; use
  encrypted swap if that's in your threat model.
- Full threat model and review trail: [docs/review/](docs/review/), starting with
  [`01_security.md`](docs/review/01_security.md).

## Development

Start with [`AGENTS.md`](AGENTS.md) (rules, invariants, validation commands) and
[`CONTEXT.md`](CONTEXT.md) (architecture map). Tests: `just test` (unit → agent →
CLI → UI; needs a podman/docker socket), `just test-extension-unit`, and see
[docs/testing.md](docs/testing.md). Current backlog: [docs/roadmap.md](docs/roadmap.md).

## Status

Pre-release. Works daily against Vaultwarden and bitwarden.com; not yet published to
package repositories or extension stores (tracked in
[docs/review/07_packaging.md](docs/review/07_packaging.md)).

## License

[GPL-3.0-only](LICENSE), matching the COSMIC ecosystem. Not affiliated with Bitwarden
Inc. or System76.
