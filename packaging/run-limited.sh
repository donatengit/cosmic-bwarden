#!/bin/bash
# Run a command inside a transient systemd scope with CPU and memory caps, so
# a long test run leaves the desktop usable.
#
# Usage: run-limited.sh <cpus> <memory> <command> [args...]
#   cpus   - whole or fractional cores, e.g. "6" or "1.5". "0" disables the cap.
#   memory - systemd MemoryHigh value, e.g. "8G". "0" disables the cap.
#
# Scope, and what this does NOT cover: the scope constrains the command and
# every process it spawns (cargo, rustc, the test binary, the agents the suite
# launches). It does NOT constrain rootless podman containers. Podman places
# each container in its own scope under `user@<uid>.service/user.slice/
# libpod-<id>.scope`, which is a sibling of this scope rather than a child, so
# the container inherits none of these limits. Verified on podman 6.1.1: a
# container started from inside a `CPUQuota=200%` scope still reports
# `cpu.max: max`. Container caps are therefore applied separately, from inside
# the test harness — see crates/cosmic-bwarden-tests/src/container_limits.rs
# and docs/testing.md.
#
# Falls back to running the command directly when systemd is unavailable (a
# container, a CI runner without a user manager, a non-systemd distro). The
# caps are a courtesy to the developer's machine, never a correctness
# requirement, so an unavailable systemd must not stop the tests.

set -uo pipefail

if [ "$#" -lt 3 ]; then
    echo "usage: $0 <cpus> <memory> <command> [args...]" >&2
    exit 2
fi

cpus="$1"; shift
memory="$1"; shift

# Validate before use. An empty `just` variable (`just test_cpus= test`) would
# otherwise shift the arguments by one and silently reinterpret the memory
# value as the CPU count and the first word of the command as the memory
# value, producing "Failed to find executable" instead of naming the mistake.
if ! [[ "$cpus" =~ ^[0-9]+(\.[0-9]+)?$ ]]; then
    echo "[run-limited] invalid cpus '$cpus': expected a number such as 6 or 1.5, or 0 to disable." >&2
    echo "              Check the test_cpus justfile variable." >&2
    exit 2
fi
if ! [[ "$memory" =~ ^[0-9]+(\.[0-9]+)?[KMGT]?$ ]]; then
    echo "[run-limited] invalid memory '$memory': expected a size such as 8G or 512M, or 0 to disable." >&2
    echo "              Check the test_memory justfile variable." >&2
    exit 2
fi

args=()

if [ "$cpus" != "0" ] && [ -n "$cpus" ]; then
    # systemd expresses CPUQuota in percent of ONE core: 100% = 1 core.
    quota=$(awk -v c="$cpus" 'BEGIN { printf "%d", c * 100 }')
    if [ "$quota" -gt 0 ] 2>/dev/null; then
        args+=(-p "CPUQuota=${quota}%")
    fi
fi

if [ "$memory" != "0" ] && [ -n "$memory" ]; then
    # MemoryHigh, not MemoryMax. MemoryMax is a hard limit: the kernel
    # OOM-kills processes that exceed it, so a browser E2E run (nested
    # compositor + Firefox + agent) that spikes past the value would die with a
    # confusing failure rather than a slow one. MemoryHigh throttles the cgroup
    # and pushes it into reclaim instead, which is what "keep the desktop
    # usable" actually calls for — the goal is to stop the tests monopolising
    # the machine, not to enforce a ceiling that tests must respect.
    args+=(-p "MemoryHigh=${memory}")
fi

# Nothing to enforce: run directly rather than paying for a scope.
if [ "${#args[@]}" -eq 0 ]; then
    exec "$@"
fi

if ! command -v systemd-run >/dev/null 2>&1; then
    echo "[run-limited] systemd-run not found; running without resource caps" >&2
    exec "$@"
fi

# A user manager must be running for --user scopes. In a container or on a
# non-systemd host this fails, and the tests should still run.
if ! systemctl --user show-environment >/dev/null 2>&1; then
    echo "[run-limited] no systemd user manager; running without resource caps" >&2
    exec "$@"
fi

echo "[run-limited] cpus=${cpus} memory=${memory} -- $*" >&2

# --collect removes the transient unit once it exits, so repeated runs do not
# accumulate failed units. --same-dir keeps the working directory.
# Not --pty: it mangles cargo's output and breaks piping into a log.
# Capture the status directly. Do NOT wrap this in `if ! ...`: inside such a
# block `$?` is the *negated* condition status (always 0), which silently turns
# a failing test suite into a passing one.
systemd-run --user --scope --quiet --collect --same-dir "${args[@]}" -- "$@"
status=$?

if [ "$status" -ne 0 ]; then
    # Distinguish "the command failed" (propagate the status) from "the scope
    # could not be created" (retry unconfined). A retry on every failure would
    # run the whole suite twice, so probe cheaply: if a trivial scope cannot
    # start, the environment is at fault rather than the command.
    if ! systemd-run --user --scope --quiet --collect true >/dev/null 2>&1; then
        echo "[run-limited] scope creation failed; retrying without caps" >&2
        exec "$@"
    fi
fi

exit "$status"
