# Test Cleanup Plan — tests must clean up their own state

Status: **implemented** (2026-09-06). Every fix below has landed; the file
paths and helper names in each section name the code as it now exists.

This plan makes the test suites remove the state they create, so nothing
accumulates under `~/.config`, `~/.cache`, `~/.local/share`, `/run/user/$UID`,
or `/tmp`, and so no suite leaves the developer's browser wired to a test
profile. **No manual cleanup command is required** — each test removes its own
state on teardown.

Every claim below was verified against the tree at the cited `file:line`.

**Reading the citations.** Line numbers for files this change *edited*
(`paths.rs`, `common.rs`, the browser-extension scripts) describe the tree
**before** the fix, so they no longer resolve to the code they name; those are
cited by symbol instead. Line numbers for files the change did not touch
(`dirs.rs`, `agent/src/lib.rs`, `config.rs`) are still current.

## Problem summary

Two distinct classes of residue, with very different severity:

1. **Empty profile directories** named `cosmic-bwarden-test-<uuid>` and
   `cosmic-bwarden-test-<fixed-name>` under `~/.cache`, `~/.local/share`,
   `~/.config`, and `/run/user/$UID`. On a developer machine with a long
   history these number in the dozens. Every UUID-named one inspected
   contained **zero files** — they are cosmetic clutter, not data.
2. **Mutated real user state that survives the run.** The browser-extension
   suites overwrite the developer's *real* native-messaging host manifests and
   point them at a test profile, and never restore them. This is a live
   violation of AGENTS.md's "Tests must never touch real user state" rule and
   is the more consequential of the two.

## Root cause

### Which code creates which directory

`cosmic_bwarden_core::dirs::make_all()` (`crates/cosmic-bwarden-core/src/dirs.rs:17-23`)
`create_dir_all`s three dirs at 0700 for the current profile — **cache,
runtime, and data**. It does **not** create the config dir:

| Dir | Path | Created by |
|---|---|---|
| cache | `$XDG_CACHE_HOME/cosmic-bwarden-<profile>` | `dirs.rs:18` (`make_all`), `db/persistence.rs:60,85` (`Db::save`) |
| runtime | `$XDG_RUNTIME_DIR/cosmic-bwarden-<profile>`, else `/tmp/cosmic-bwarden-<profile>-<uid>` (`dirs.rs:141-155`) | `dirs.rs:19` (`make_all`), `lib.rs:179-190` (socket parent) |
| data | `$XDG_DATA_HOME/cosmic-bwarden-<profile>` | `dirs.rs:20` (`make_all`), `generator_settings.rs:29`, `handler/generator/storage.rs:60,94`, `session_store.rs:128` |
| config | `$XDG_CONFIG_HOME/cosmic-bwarden-<profile>` | **`config.rs:192`** on config save — never by `make_all` |

The agent calls `make_all()` at `crates/cosmic-bwarden-agent/src/lib.rs:171`.
Note that `browser-host` mode **returns at `lib.rs:154-165`, before that call**,
and `agent/src/browser_host.rs` contains no `dirs::` use at all — so a
browser-host spawn creates no profile dirs and is not a leak source.

The profile name comes from `dirs::profile()` (`dirs.rs:157-162`), which maps
`COSMIC_BWARDEN_PROFILE=<x>` to `cosmic-bwarden-<x>` and an unset/empty value
to the **live** `cosmic-bwarden` profile.

### Why dirs land in the real home

An agent or CLI subprocess writes into the real home whenever it runs with
`XDG_CACHE_HOME` / `XDG_DATA_HOME` / `XDG_CONFIG_HOME` / `XDG_RUNTIME_DIR`
unset or inherited. Note these are commonly **unset** on a normal Linux
desktop (only `XDG_RUNTIME_DIR` is typically set), so `directories` falls back
to `$HOME/.cache`, `$HOME/.local/share`, `$HOME/.config`.

### The live producer: inherited process-global profile

`paths.rs`'s `test_config_socket_override` and `test_override_priority` spawned
the agent with **no environment set at all** — no profile, no XDG. `Command` inherits the
parent's current environment, and 59 sibling tests in the same crate call
`std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile)` process-globally and
**never restore it** (0 matching `remove_var` calls). The tests crate is a
single lib-test binary, i.e. one process.

So the profile those two spawns inherit is *whatever the last test to run set*:

- If a UUID-profile test ran first → the agent creates
  `~/.cache/`, `~/.local/share/`, and `/run/user/$UID/cosmic-bwarden-test-<uuid>`.
  Reproduced directly: spawning the debug agent with an inherited UUID profile
  and no XDG created exactly those three dirs, all empty.
- If `paths.rs` runs before any of them → the var is unset and the agent runs
  against the **live `cosmic-bwarden` profile**.

This is why the leaked UUID dirs are cache+data+runtime but never config: both
spawns pass `--config <tempdir>/config.json`, and `lib.rs:117-118` turns that
into a `COSMIC_BWARDEN_CONFIG` override, so `config_file()` never resolves under
`config_dir()`.

**Consequence for the fix:** the bug is environment *inheritance*, not merely
absence. Adding XDG vars is not sufficient on its own — the spawn must set the
profile explicitly too, or the ordering-dependent live-profile case remains.

### The fixed-profile scripts

`tests/browser-extension/run-e2e.sh`, `run-chrome-e2e.sh`, and
`playwright/setup_native_host.sh` exported a fixed profile
(`test-extension-e2e`, `test-chrome-e2e`) and start the agent with no XDG
redirect; `playwright/test-utils.js`'s `runCli` sets the profile only.
These produce the fixed-name dirs, including the `config.json` written via
`config.rs:192`.

`tests/browser-extension/minimal_verify.sh` is a special case: it exports
`HOME="$TEST_TMP"` *before* starting the agent, so its cache/data/
config dirs already land inside `$TEST_TMP`. Its only real leaks are the
runtime dir (`XDG_RUNTIME_DIR` is inherited, not under the fake HOME) and
`$TEST_TMP` itself, whose removal was **commented out**.

### Already correct (do not change)

- `common.rs`'s `start_agent_with_env` and `apply_cli_env` set
  the profile and all four XDG vars on every agent and CLI spawn.
- `ipc_hardening.rs`'s `spawn_bare_agent` builds `config/cache/data/runtime` under a
  `tempfile::tempdir()` and sets all four XDG vars plus a throwaway UUID profile.
- `browser_host.rs` passes `--socket` explicitly (`set_socket_override`
  runs at `lib.rs:120-122`, ahead of the browser-host branch) and, as noted
  above, browser-host mode creates no dirs.

## Guiding principle

> Every test and test script that spawns the agent or CLI must (a) pass an
> **explicit** profile, `HOME`, and all four XDG vars rather than inheriting
> them — a partial set is not partial safety, and
> (b) remove or restore everything it caused to be created, in the same
> teardown path that already kills the agent. No exceptions; no separate
> command.

Two ways to satisfy (b), chosen per producer:

- **Redirect all four XDG vars into a temp dir and let `tempfile::TempDir`
  drop.** The profile dirs then never appear in the home at all. **Preferred**
  for Rust spawns.
- **Explicitly remove/restore in the script's `cleanup()` trap**, for the
  shell and Playwright scripts, which do not use `tempfile`.

## Fixes

### Fix 1 — Give every Rust agent/CLI spawn an explicit environment

`paths.rs`'s `test_config_socket_override` and `test_override_priority` were the
only remaining Rust spawns without one.

**Action:** apply the pattern `ipc_hardening.rs`'s `spawn_bare_agent` already uses — create
`config`/`cache`/`data`/`runtime` subdirs (0700) under a `tempfile::tempdir()`,
then on the `Command`:

```rust
.env_clear()                      // or set every var below explicitly
.env("PATH", std::env::var("PATH").unwrap_or_default())
.env("HOME", base)                // see the warning below — required
.env("COSMIC_BWARDEN_PROFILE", format!("test-{}", uuid::Uuid::new_v4()))
.env("XDG_CONFIG_HOME",  base.join("config"))
.env("XDG_CACHE_HOME",   base.join("cache"))
.env("XDG_DATA_HOME",    base.join("data"))
.env("XDG_RUNTIME_DIR",  base.join("runtime"))
```

`.env_clear()` (or an equivalently exhaustive explicit set) is the point — it
is what makes the spawn independent of whichever sibling test ran last. Because
the profile dirs then live under the tempdir, `TempDir::drop` removes them —
no leftover, no manual command.

> **`.env_clear()` alone does not protect you — all four XDG vars are
> mandatory, and so is `HOME`.** Verified by experiment: an agent spawned with
> `env -i` (no `HOME` at all) and only three of the four XDG vars set still
> created `~/.local/share/cosmic-bwarden-<profile>` in the **real** home. The
> `directories` crate resolves the home directory from the passwd database
> (`getpwuid`) when `$HOME` is unset, so clearing the environment removes the
> protection you might expect and silently falls back to the developer's
> account. Setting `HOME` to the tempdir makes the fallback land somewhere
> harmless if a future refactor drops one of the XDG vars. With all four set,
> the same spawn leaked nothing.

This also fixes the real-user-state violation: `paths.rs` can currently run the
agent against the live `cosmic-bwarden` profile.

**Also (done):** all 59 unrestored
`std::env::set_var("COSMIC_BWARDEN_PROFILE", …)` calls are now
`let _profile = state_guard::ProfileEnv::set(&env.profile);`, a scoped guard
that restores the previous value on drop (the pattern at
`agent/src/lib.rs:500-523`). They were the reason spawn inheritance was
non-deterministic, and being process-global they also affected any in-process
`dirs::` call. The guard is `#[must_use]` so a future call site cannot silently
drop it at the end of the statement and lose the profile for the rest of the
test body.

### Fix 2 — Restore the native-messaging host manifests (highest severity)

These are not empty dirs; they change how the developer's real browser behaves
after the suite exits.

- **`tests/browser-extension/playwright/setup_native_host.sh`**
  overwrites `~/.mozilla/native-messaging-hosts/cosmic-bwarden-browser-host.sh`
  with a wrapper hardcoding `COSMIC_BWARDEN_PROFILE=test-extension-e2e`, and
  never restores it. After `just test-extension-e2e`, the developer's real
  Firefox extension talks to the test profile.
  **Action:** back the file up before overwriting and restore it in
  `run-e2e.sh`'s `cleanup()` trap; if no backup exists, re-run
  `just register-browser-host` (the existing recipe that writes the correct
  development wrapper) rather than deleting the manifest.
- **`tests/browser-extension/playwright/chrome-full.spec.js`**
  (`registerNativeHost`) writes a wrapper and manifest into
  `~/.config/chromium/`, `~/.config/google-chrome/`, and
  `~/.config/google-chrome-for-testing/NativeMessagingHosts/` — real user
  config dirs — pointed at the test socket, and never removes them.
  **Action:** record which of those paths did not exist before the run, and
  remove exactly those in the spec's teardown; restore any pre-existing file
  from a backup. Prefer writing only into `userDataDir` (which is disposable)
  if the Chrome under test will find it there.

This is a required teardown step, not an optional sweep: leaving it undone
means the suite silently repoints a developer's browser.

### Fix 3 — Make the browser-extension scripts self-clean in their `cleanup()` trap

Each script already has a `cleanup()` trap (`run-chrome-e2e.sh`,
`run-e2e.sh`, `minimal_verify.sh`) that kills PIDs / stops
containers. Each must also remove the dirs its fixed profile created.

Resolve the roots via `XDG_*` with a `$HOME` fallback, **not** by hardcoding
`$HOME/.cache` — and `wait` for the agent to exit before removing, or its
shutdown writes recreate what was just removed. (The `$HOME` fallback matches
the agent in every case that matters here; strictly, `directories` falls back
to the passwd entry rather than `$HOME`, but a shell script always has `$HOME`
set, and where a script *reassigns* it — `minimal_verify.sh` — the agent it
spawned inherited the same value.)

```sh
cleanup_profile() {
    local profile="$1" uid
    uid=$(id -u)
    rm -rf -- \
      "${XDG_CONFIG_HOME:-$HOME/.config}/cosmic-bwarden-$profile" \
      "${XDG_CACHE_HOME:-$HOME/.cache}/cosmic-bwarden-$profile" \
      "${XDG_DATA_HOME:-$HOME/.local/share}/cosmic-bwarden-$profile" \
      "${XDG_RUNTIME_DIR:-/run/user/$uid}/cosmic-bwarden-$profile" \
      "${TMPDIR:-/tmp}/cosmic-bwarden-$profile-$uid"
}
```

Per script:

- `run-chrome-e2e.sh` — `kill "$AGENT_PID"; wait "$AGENT_PID" 2>/dev/null`, then
  `cleanup_profile test-chrome-e2e`.
- `run-e2e.sh` — same, with `test-extension-e2e`, plus the Firefox manifest
  restore from Fix 2.
- `minimal_verify.sh` — the cache/data/config dirs are already inside
  `$TEST_TMP` (the script sets `HOME` before starting the agent), so the fix
  here is to restore the commented-out `rm -rf "$TEST_TMP"`, which subsumes all three. The one thing that
  escapes is the **runtime** dir: `XDG_RUNTIME_DIR` is inherited, so it points
  at the real `/run/user/$UID`, not the fake HOME. Calling
  `cleanup_profile test-verify` here is harmless but largely redundant (its
  `$HOME`-based paths resolve inside `$TEST_TMP`); the runtime line is the
  part that does the work.

`cleanup_profile()` lives in one sourced file,
`tests/browser-extension/cleanup.sh` (beside the three scripts that source it,
not under `playwright/`, since `minimal_verify.sh` is not a Playwright script),
so the removal list stays in sync. It refuses an empty name, a non-`test-*`
profile, and any name containing a path separator or `..`, and re-checks each
target's basename against the live `cosmic-bwarden` dir before removing.
`backup_file()` / `restore_file()` in the same file handle the native-messaging
manifests, recording *absence* too so a file the test creates is removed rather
than left behind.

`test-utils.js`'s `runCli` sets the profile but no XDG; it is only
invoked from these scripts, so Fix 3's trap covers it. If it ever runs
standalone, give it the same explicit XDG env.

### Fix 4 — Replace the "belt-and-braces" removal with an assertion

The original draft of this plan proposed recording
`config_dir`/`cache_dir`/`data_dir` in `TestEnv` and `remove_dir_all`-ing them
in `Drop`. **Do not do that.** Those functions read process-global env
(`dirs.rs:126-162`), and — per the Root cause section — `COSMIC_BWARDEN_PROFILE`
in the test process is whatever the last sibling test left, or unset. Unset
resolves to the live `cosmic-bwarden` profile, so `Drop` would
`remove_dir_all` the developer's real vault cache. That is the same class of
incident as the 2026-08-10 `test_settings_flow` config wipe recorded in
AGENTS.md, reintroduced inside the teardown meant to prevent it. (It also does
not compile: `dirs::config_dir()` and `dirs::runtime_dir()` are private; only
`cache_dir` and `data_dir` are `pub`.)

Everything `TestEnv` creates already lives under `_temp_dir`, which drops. So
the safety net should **detect** a non-redirected spawn rather than delete
after one:

- Add a helper that snapshots the set of `cosmic-bwarden*` entries under the
  four real roots (`${XDG_CONFIG_HOME:-$HOME/.config}` etc., plus
  `${XDG_RUNTIME_DIR:-/run/user/$uid}` and `${TMPDIR:-/tmp}`).
- Take a snapshot at the start of the test binary and compare at the end;
  fail if the set grew. This is the mechanical enforcement hook for the
  AGENTS.md rule below — without it, "review-blocking" decays exactly the way
  a habit does.
- If a removal is ever genuinely wanted in `TestEnv::Drop`, derive the paths
  **only** from the struct's own `config_home`/`cache_home`/`data_home`/
  `runtime_home` fields on `TestEnv` and assert each target is under
  `_temp_dir` before removing. Never from `dirs::`, never from process env,
  and never panic in `Drop` — log at `warn!` per AGENTS.md's no-silent-failures
  rule.

### Fix 5 — Stray files outside the profile dirs

- `common.rs`'s `setup_env_no_agent` wrote **`agent_test.log` into the repo
  root** on every run. It is covered by `.gitignore:64` (`*.log`), so it is not a commit
  hazard, but it dirties the working tree. Move it under the test's temp dir,
  or into `target/`.
- `/tmp` logs written by the scripts and specs and never removed:
  `/tmp/native-host-debug.log` (`chrome-full.spec.js`),
  `/tmp/cosmic-bwarden-browser-host.log` (`setup_native_host.sh`,
  `minimal_verify.sh`), `/tmp/agent_test.log`, `/tmp/agent_chrome_test.log`,
  `/tmp/vaultwarden_test.log`, `/tmp/vaultwarden_chrome_test.log`.
  Either route them into the run's temp dir or remove them in `cleanup()`.
- `~/.cache/cosmic-bwarden-probe` exists on at least one developer machine and
  matches **no profile anywhere in the tree** — an orphan from a removed test.
  Note it here so the one-time sweep below is not scoped to `test-*` only.
- **Removing a container must also remove its anonymous volumes** — pass
  `v: true` in `RemoveContainerOptions`. Vaultwarden mounts `/data` as an
  anonymous volume; without `v: true` the container goes and the volume stays
  dangling in `~/.cache/podman/storage/volumes` forever. testcontainers' own
  teardown already does this; `common::cleanup_stale_containers` did not, so
  volumes leaked only when a killed run's remains were swept by the next run.

## Guiding documents must require cleanup after the tests

The invariant cannot hold on tinkering alone — it has to be written into the
normative docs so every current *and future* test follows it.

- **`AGENTS.md`** — extend the "Tests must never touch real user state"
  section (today about not *overwriting* the live profile) with a self-cleanup
  rule, cross-referencing the existing Golden Rule on generated artifacts:

  > **Tests must clean up their own state.** Any test or test script that
  > spawns the agent or CLI must pass an **explicit** `COSMIC_BWARDEN_PROFILE`,
  > `HOME`, and **all four** `XDG_*` vars — never inherit them, since
  > `COSMIC_BWARDEN_PROFILE` is process-global and an unset value resolves to
  > the *live* profile. Setting only some of them is not partial safety:
  > `directories` falls back to the passwd entry when `$HOME` is unset, so a
  > missing `XDG_DATA_HOME` lands in the developer's real
  > `~/.local/share` even under `env_clear()`. It must then remove, in its own teardown path (Rust
  > `Drop`/async guard, script `cleanup()` trap), every
  > `cosmic-bwarden-<profile>` directory it caused to be created under
  > `$XDG_CONFIG_HOME`, `$XDG_CACHE_HOME`, `$XDG_DATA_HOME`,
  > `$XDG_RUNTIME_DIR`, and `$TMPDIR` — and restore any real user file it
  > overwrote (native-messaging manifests especially). Test profiles must be
  > named `test-*`. Redirecting all four XDG vars into a `tempfile::tempdir()`
  > satisfies the directory half. Leaving residue behind, or leaving a
  > developer's browser pointed at a test profile, is a review-blocking
  > regression. This is not satisfied by a manual sweep or an external
  > `clean-test-data` command; the snapshot assertion in the E2E suite is the
  > mechanical check.

  The `test-*` naming requirement is load-bearing and currently unenforced:
  `dirs::profile()` (`dirs.rs:157`) accepts any string, so without it the
  verification snapshot below cannot distinguish a test profile from a real one.

- **`docs/testing.md`** / **`docs/configurable_paths.md`** — add a "Cleanup
  after tests" subsection naming the teardown path each suite must use
  (`TestEnv`'s `_temp_dir`, script `cleanup()` trap) and how to verify.
  `docs/testing.md:124` documents `export COSMIC_BWARDEN_PROFILE=manual-test`
  for manual runs with no cleanup note — add one there, pointing at
  `cleanup_profile()`.

- **Review checklist** — "did the test leave state behind, or overwrite a real
  user file?" becomes a standard gate alongside "does it touch the live
  profile?".

## Non-goals / explicitly out of scope

- **No `clean-test-data` command, no `just` sweep, no init hook.** Cleanup
  lives with each test/script teardown so it happens automatically.
- Do **not** add cleanup to the agent's production `make_all()` or any
  production code path — the dirs are created by test runs, not by the agent.
- Do **not** remove the live `cosmic-bwarden` profile dirs, and do **not**
  compute any removal path from process-global env (see Fix 4).

## Verification

A glob `ls` is too weak — a no-match glob returns nonzero and silently misses
non-`test-*` profiles and the runtime dir. Use a set diff over all roots.

Note the `find` rather than a quoted glob: `ls -d "${roots[@]/%//cosmic-bwarden*}"`
does **not** expand the `*` (quoted expansions are not globbed), so with
`2>/dev/null` it reports "clean" unconditionally. The version below was run
against a dirty machine and correctly returned 67 entries.

```bash
# Requires the container socket for the Rust E2E suite:
#   systemctl --user start podman.socket
snapshot() {
  local uid; uid=$(id -u)
  local r
  for r in "${XDG_CONFIG_HOME:-$HOME/.config}" "${XDG_CACHE_HOME:-$HOME/.cache}" \
           "${XDG_DATA_HOME:-$HOME/.local/share}" "${XDG_RUNTIME_DIR:-/run/user/$uid}" \
           "${TMPDIR:-/tmp}"; do
    [ -d "$r" ] && find "$r" -maxdepth 1 -name "cosmic-bwarden*" 2>/dev/null
  done | sort
}

NM=~/.mozilla/native-messaging-hosts/cosmic-bwarden-browser-host.sh
snapshot > /tmp/cbw-before.txt
[ -f "$NM" ] && md5sum "$NM" > /tmp/cbw-nm-before.txt

cargo test -p cosmic-bwarden-tests -- --test-threads=1
just test-extension-e2e
just test-extension-e2e-chrome
bash tests/browser-extension/minimal_verify.sh

# No cleanup command in between. All three checks must be silent / pass:
snapshot > /tmp/cbw-after.txt; diff /tmp/cbw-before.txt /tmp/cbw-after.txt
[ -f /tmp/cbw-nm-before.txt ] && md5sum -c /tmp/cbw-nm-before.txt
find ~/.config/chromium ~/.config/google-chrome ~/.config/google-chrome-for-testing \
     -maxdepth 2 -name 'cosmic-bwarden*' -o -name 'com.enikeev.cosmic_bwarden.json' 2>/dev/null
```

Run the suites in that order and do **not** interleave a cleanup. Dropping the
separate `cargo test … paths` line is deliberate: `paths.rs` leaks only via
inherited process state, so it must be exercised inside the full run, not in
isolation where nothing has set the profile.

The already-collected residue from past runs is a pre-existing one-time
condition. A single `rm -rf` of the `cosmic-bwarden-test-*` dirs (plus the
orphaned `cosmic-bwarden-probe` from Fix 5) clears it; that sweep is not part
of the ongoing workflow, and the diff above is what keeps it from returning.

## Acceptance criteria

- Every Rust spawn of the agent or CLI passes an explicit profile, `HOME`, and
  all four XDG vars; none inherits `COSMIC_BWARDEN_PROFILE` from the test
  process.
- `paths.rs` never runs the agent against the live `cosmic-bwarden` profile,
  under any test ordering.
- Every test/script removes the `cosmic-bwarden-<profile>` dirs it created —
  config, cache, data, **and runtime** — in its own teardown path.
- The Firefox and Chrome native-messaging manifests are byte-identical before
  and after every suite run, or restored via `just register-browser-host`.
- No `remove_dir_all` target anywhere in the test suite is derived from
  process-global env; the E2E suite asserts (not deletes) via the before/after
  snapshot.
- `AGENTS.md` requires explicit-env spawning, `test-*` profile naming, and
  cleanup-after-test, and names the snapshot assertion as the mechanical check;
  `docs/testing.md` and `docs/configurable_paths.md` carry the same rule for
  their suites.
- Running every suite in sequence with **no** cleanup command in between ends
  with an empty `diff` from the Verification block.
- The live `cosmic-bwarden` profile dirs are never removed by any test/script.
