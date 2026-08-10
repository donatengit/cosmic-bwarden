# cosmic-bwarden

A native **Bitwarden / Vaultwarden client for the COSMIC™ desktop** — panel applet,
background agent, vault window, CLI, SSH agent, and browser extension, written in Rust
on [libcosmic](https://github.com/pop-os/libcosmic).

Point it at bitwarden.com, bitwarden.eu, or your self-hosted Vaultwarden.

> **Status: pre-release.** Used daily against Vaultwarden and bitwarden.com, but not
> yet in any package repository or extension store, and **not independently audited**.
> Treat it accordingly — see [Security model, honestly](#security-model-honestly).

## Why it exists

I ran Bitwarden's official desktop client for years and was continually annoyed by one
thing: its CPU usage when I wasn't using it. Not during unlock or bulk decryption — in
ordinary background idle. An Electron shell permanently idling at a few percent of a
core convinced me that the *resource floor* of a password manager matters as much as its
speed. A thing that runs 24/7 on your session should cost approximately nothing when you
aren't touching it.

So this is a client built to be as close to nothing as possible at rest, plus the four
gaps I actually felt daily:

- **Tiny idle footprint** — the whole point. The agent sits at **5.7–5.9 MB RSS** with
  *zero* voluntary context switches while idle; the release binary is 4.8 MB. Unlock
  costs 88 ms (PBKDF2 600k) to 131 ms (Argon2id), and decrypting a 5,000-entry vault
  takes 7.7 ms. Numbers and method: [docs/review/06_performance.md](docs/review/06_performance.md).
- **An SSH agent** that serves keys straight from the vault — `ssh` works while the
  vault is unlocked and stops working when it locks. No second key store to manage.
- **A device-bound PIN via TPM2** — not because a master password is weak, but because
  retyping a genuinely strong one all day is genuinely annoying. Sealing the vault key
  to the TPM, bound to firmware + Secure Boot state, behind a short PIN under the TPM's
  own anti-hammering lockout, is the trade-off I actually want on my own laptop.
- **One lock state for the whole system.** Applet, vault window, CLI, SSH agent, and
  browser extension are thin clients over one agent. Unlock once; lock once; everything
  follows.
- **Favourites one click from the panel**, usable without ever opening a browser — the
  applet shows favourites first and searches as you type.

And the reason it's COSMIC: the desktop's workspace model is excellent, and it deserved
a password manager that lives in the panel and speaks its design language rather than
shipping its own browser.

Full postmortem of building it — including the security review that found three
S0-class bugs in my own code, and the shipped bug that green tests missed:
[**Eight Weeks, Seventy Commits, One Password Manager**](https://don.enikeev.com/posts/cosmic-bwarden/).

### Prior art

This project stands on [`rbw`](https://github.com/doy/rbw) (a Rust CLI Bitwarden client)
and the [official Bitwarden clients](https://github.com/bitwarden/clients), both read
closely to get the crypto and API semantics right rather than reinventing them. The
security review repeatedly diffed this repo's cipherstring handling against theirs.

## Features

- **Panel applet**: search-as-you-type over your vault, favourites first, one-click copy
  of usernames/passwords/notes, inline unlock, TOTP-aware entries, password generator.
- **Vault window**: full CRUD for logins, secure notes, cards, identities, and SSH keys,
  with real-time server sync and master-password reprompt for gated items.
- **Background agent**: holds vault keys in `mlock`ed, zeroize-on-drop memory; locks on
  suspend/shutdown/session-lock (logind) and on an inactivity timeout; core dumps
  disabled; hardened systemd user unit.
- **SSH agent**: vault-stored keys served over the standard ssh-agent protocol.
- **CLI**: scriptable `list / get / add / edit / delete` with `KEY=VALUE` syntax, and
  notes piped through stdin so whole secret files round-trip for scripting and backups.
- **Browser extension** (Firefox/Chrome): domain-matched entries (PSL-aware, so
  `mail.google.com` finds your Google login and `facebook.com.evil.net` finds nothing),
  form fill, save prompts, PIN unlock, and on-demand secret reveal — secrets never sit
  in extension state from passive browsing.
- **TPM PIN unlock** (optional, `--features tpm`): vault keys sealed in TPM2 under a
  `PolicyPCR ∧ PolicyAuthValue` session bound to firmware + Secure Boot state (PCR 0/7).

## Install

Prerequisites: Rust toolchain, `just`, and libcosmic's system deps
(`libxkbcommon-dev libwayland-dev libegl1-mesa-dev pkg-config cmake`; add `libtss2-dev`
for the `tpm` feature). Details in [docs/build_and_run.md](docs/build_and_run.md).

```sh
git clone https://github.com/donatengit/cosmic-bwarden
cd cosmic-bwarden
just user-install    # to ~/.local (no sudo); or: sudo just install
just enable-agent    # systemd --user enable + start the agent
just restart-panel   # let the COSMIC panel discover the applet
```

A `.deb` is produced by `cargo deb -p cosmic-bwarden-ui --no-build` after `just build`;
an AUR `PKGBUILD` lives in [`packaging/`](packaging/).

## First run

1. Add the applet: COSMIC Settings → Desktop → Panel → Configure panel applets →
   **CosmicBWarden**.
2. Click the panel icon → **Open Vault Window** → log in (email, master password, and
   optionally your self-hosted server URL under *Advanced*).
3. That's it — search from the applet, copy with one click. The agent keeps the vault
   available (locked) across reboots; you re-enter only the master password, never the
   full login.

Browser extension: `just pack-extension`, load `target/cosmic-bwarden-extension.zip`,
and register the native host with `just register-browser-host` (dev) or `just install`
(system). See [docs/browser_extension.md](docs/browser_extension.md).

## Security model, honestly

- The **agent** is the only process holding secrets: keys live in memory-locked
  (`mlock`) buffers, zeroized on lock/drop; `PR_SET_DUMPABLE=0`; sockets are `0600` with
  per-connection same-UID (`SO_PEERCRED`) checks.
- The **on-disk cache** stores only server-encrypted data (AES-256-CBC + HMAC-SHA256,
  encrypt-then-MAC, verified before decrypt). Session tokens go to the Secret Service
  keyring, never to the JSON cache.
- **Out of scope**: a hostile process running *as your user* can ultimately reach the
  agent — same-UID is the trust boundary on a single-user desktop, and master-password
  reprompts raise the cost of an attack rather than preventing it. Unencrypted swap can
  page session tokens; use encrypted swap if that's in your threat model.
- **Not audited.** The nine-phase review in [docs/review/](docs/review/) was self-imposed
  (me, plus AI, reviewing me plus AI). It found real S0 bugs — TPM blobs that weren't
  actually PCR-bound, bulk reads bypassing reprompt, credentials reaching the log at
  `info!` — and all findings are written up rather than quietly fixed. An outside review
  is still a release gate this project hasn't passed.
- Start with [`docs/review/01_security.md`](docs/review/01_security.md); report issues
  per [SECURITY.md](SECURITY.md).

## Development

Start with [`AGENTS.md`](AGENTS.md) (rules, invariants, validation commands) and
[`CONTEXT.md`](CONTEXT.md) (architecture map). Tests: `just test` (unit → agent → CLI →
UI; needs a podman/docker socket for the Vaultwarden container), `just
test-extension-unit`, and see [docs/testing.md](docs/testing.md). Current backlog:
[docs/roadmap.md](docs/roadmap.md). Contribution notes: [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[GPL-3.0-only](LICENSE), matching the COSMIC ecosystem. Not affiliated with Bitwarden,
Inc. or System76. "Bitwarden" is a trademark of Bitwarden, Inc.; "COSMIC" is a trademark
of System76, Inc. This is an independent client that speaks their API.
