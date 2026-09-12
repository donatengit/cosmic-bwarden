# Build and Run Instructions

## Prerequisites

Before building, ensure you have the following installed on your system:
- **Rust Toolchain**: `rustc` and `cargo` (1.75.0 or later recommended).
- **`just`** — the task runner used for all build/install/test orchestration (see `justfile`).
- **System Dependencies** (for `libcosmic` and cryptography):
  - `libxkbcommon-dev`
  - `libwayland-dev`
  - `libegl1-mesa-dev`
  - `pkg-config`
  - `cmake`
  - `openssl` (or `pkg-config` for system-wide detection)
- **For the Rust E2E test suite**: a container socket — either Docker, or podman with
  `systemctl --user start podman.socket` (auto-detected by the test harness).
- **For the browser extension**: `npm` (unit tests via vitest, E2E via Playwright).

> The reference repositories checked in as git submodules (`bitwarden-official-clients`,
> `libcosmic`, `cosmic_examples`, `rbw_reference`) are **not** build inputs — a plain
> `git clone` without `--recurse-submodules` builds fine; `libcosmic` is fetched by Cargo
> from GitHub.

## Building for Native Architecture

To build the entire workspace with optimizations for your specific CPU architecture:

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

This will produce binaries in `target/release/`:
- `cosmarden-agent`: The background daemon.
- `cosmarden-applet`: The main GUI application and panel tray applet (built from the `cosmarden-ui` crate).
- `cosmarden`: The command-line interface (crate `cosmarden-cli`).

## Running the Components

### 1. Start the Agent
The agent must be running for the UI or CLI to function.

```bash
./target/release/cosmarden-agent
```
*Note: In a production setup, this would typically be managed by a systemd user unit.*

### 2. Launch the UI / Applet
The same binary handles both the main window and the applet tray.

```bash
./target/release/cosmarden-applet
```

### 3. CLI Usage
The `cosmarden` CLI provides a powerful interface for scripting and advanced management.

```bash
# Register a new account (prompts for the master password interactively)
./target/release/cosmarden register user@example.com

# Register/login/unlock accept --password for scripts; without it the master
# password is prompted interactively (keeping it out of argv/shell history).
./target/release/cosmarden register user@example.com --password 'correct horse battery staple'

# Add a secure note
./target/release/cosmarden add-note "My Private Key" --note "Content of the note..."

# Add an SSH key
./target/release/cosmarden add-ssh-key "My SSH Key" --private-key-path ~/.ssh/id_rsa

# List and Search
./target/release/cosmarden list
./target/release/cosmarden list | grep "My Secret"

# Get password (interactive reprompt if needed)
./target/release/cosmarden get "My Secret"

# Get a specific field (e.g. Note or SSH Private Key), revealing secrets
./target/release/cosmarden get "My Private Key" --fields notes --show-secrets

# Store a whole file as a note's contents, without it ever touching argv/shell
# history, then restore it byte-for-byte later. `--stdin` works with either
# a pipe or `< file` redirection (the latter skips the useless `cat`):
cat credentials.yaml | ./target/release/cosmarden note add "AWS Creds" --stdin
./target/release/cosmarden note add "AWS Creds" --stdin < credentials.yaml
./target/release/cosmarden get "AWS Creds" --fields notes --show-secrets > credentials.yaml

# `edit --stdin` replaces an existing entry's notes the same way:
./target/release/cosmarden edit "AWS Creds" --stdin < credentials.yaml

# Names are not unique (Bitwarden allows e.g. two logins both called
# "GitHub"), so `add` never overwrites an existing entry by name. Re-running
# the add above a second time warns on stderr but still creates a second
# "AWS Creds" note. Use --replace to delete any same-name-and-type entry
# first instead:
./target/release/cosmarden note add "AWS Creds" --replace --stdin < credentials.yaml

# Remove an entry entirely with `edit --delete` (there is no separate
# `delete`/`rm` subcommand):
./target/release/cosmarden edit "AWS Creds" --delete
```

`get --fields notes --show-secrets` is special-cased: when `notes` is the
*only* requested field, the CLI prints just the note body with no `Notes:`
label or other fields mixed in, so the pipeline above round-trips a file
exactly. Requesting `--fields all` (the default) or multiple fields still
prints the labeled, human-readable form.

### 4. Launch the Applet
If you are running the COSMIC desktop, you can launch the applet to see it in your panel.

```bash
./target/release/cosmarden-applet
```

## Environment Variables

- `COSMARDEN_PROFILE`: Set this to use a different configuration profile (default is `cosmarden`).
- `RUST_LOG`: Set to `info` or `debug` for verbose logging (e.g., `RUST_LOG=cosmarden_agent=debug`).
  The HTTP stack (`reqwest`/`hyper`/`rustls`/`h2`) is capped at `info` even under
  `RUST_LOG=trace`: at trace level those crates print full request headers,
  including `Authorization: Bearer …` session tokens, and agent logs are
  persisted to disk by journald. The cap is applied in code (agent and UI
  logger setup) and cannot be raised via the environment.

## SSH Agent Setup
To use the built-in SSH agent, export the following environment variable in your shell profile:

```bash
export SSH_AUTH_SOCK=$(cosmarden-agent --print-ssh-socket-path) 
# Or manually find it in the runtime directory managed by the agent.
```

## Justfile (task runner)

`just` is the entry point for every build, install, and test step. Run
`just --list` for the authoritative recipe list; `just` reads the recipes
themselves, so that output is never stale. The table below covers the recipes
used day to day.

| Recipe | What it does |
|---|---|
| `just build` | Release build of all Rust crates (auto-detects TPM) |
| `just install` | Copy already-built `target/release` binaries to `~/.local/bin`, systemd `--user` unit, applet metadata, Firefox native host. Does not compile — run `just build` first. `just user-install` is an alias. |
| `just clean-install` | `uninstall` then `install` |
| `just pack-extension` | Zips preselected production files only (explicit allowlist in `packaging/pack-extension.sh` — nothing unlisted can ship; the old exclude-list approach leaked `.env` once) → `target/cosmarden-extension.zip`, with shape assertions. Not part of `build`. |
| `just register-browser-host` | Registers native host pointing at debug build (dev workflow) |
| `just test` | Full Rust test suite in order: unit → agent → CLI → UI. The E2E steps auto-ensure a container socket via `ensure-container-socket` — podman is the primary runtime (`systemctl --user start podman.socket`; the harness auto-detects the user socket and needs no docker group); Docker is the fallback when `DOCKER_HOST`/`/var/run/docker.sock` exists |
| `just test-unit` | Unit tests for every crate that has them |
| `just test-extension-unit` | Extension JS unit tests (vitest) |
| `just test-extension-e2e` | Extension Playwright E2E (Firefox, mock agent) |
| `just test-extension-e2e-full` | Extension Playwright E2E (Firefox, real agent + Vaultwarden) |
| `just test-extension-e2e-chrome` | Same but Chrome |
| `just sign-extension` | The single signing entry point: stage production files → inject a fresh timestamp version (`YYYY.M.D.mmm`, dev signing — no tag/clean-tree requirements) and gecko `update_url` from `EXT_UPDATE_BASE_URL` → `web-ext lint` → AMO unlisted sign → `dist/cosmarden-<version>.xpi` (+ `dist/updates.json` when the base URL is set; append preserves entries; `update_hash` = sha256 of the signed XPI). Requires `WEB_EXT_API_KEY`/`WEB_EXT_API_SECRET` and a pinned web-ext (10.6.0, checked at entry); refuses versions already shipped in `dist/`; prints one absolute output path per line for CI capture |
| `just test-ext-release` | Offline unit tests for the release pipeline's pure logic (`node --test packaging/*.test.mjs`) |
| `just restart-panel` | Restart COSMIC panel after install |
| `just enable-agent` | Enable + start agent systemd user service |

### Notes that are easy to get wrong

- **Builds and tests run inside a systemd scope** with a CPU quota, a memory
  throttle and a lowered CPU/I/O share, so a long run does not starve the
  desktop while other agents and containers are working. It needs no setup and
  degrades to an unwrapped run when systemd is unavailable. One budget covers
  builds and tests: [`docs/testing.md`](testing.md), "Resource limits".
- **Never inline the extension zip command anywhere else** — CI and the release
  workflow call `packaging/pack-extension.sh` so every consumer produces the
  identical artifact.
- **`sign-extension` reads its AMO credentials from the environment or the
  gitignored `browser-extension/.env`** (parse-only loader
  `packaging/load-ext-env.sh`, mode 0600, explicit exports win; web-ext also
  reads the env natively via its `WEB_EXT` prefix, the config trampoline is
  belt-and-braces — never argv).
- **Release mode**: `EXT_SIGN_MODE=release` switches to the strict tag-based
  preflight (`vYYYY.MM.P[-alphaN]` tag on HEAD, clean tree; the tag version is
  injected into the staged manifest — alpha maps to the 4th component, e.g.
  `v2026.8.0-alpha` → `2026.8.0.1`; no `manifest.json` bumps are needed). It is
  used by the `sign-extension` job in `.github/workflows/release.yml`, which
  runs on every `v*` tag and attaches the signed XPI to the draft release.
- **A new test module, feature flag, service, or fixture must be wired into a
  recipe in the same change.** A suite that only runs from a hand-typed command
  line is not wired up. A recipe that covers only part of a suite must say so in
  its comment, and `just test` must run the whole Rust suite with no filters.

Test-run resource limits and what each suite covers:
[`docs/testing.md`](testing.md).
