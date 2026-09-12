#!/bin/bash
# Run a command inside a transient systemd scope with CPU, memory and scheduling
# limits, so a long build or test run leaves the desktop usable.
#
# Why this exists: several agents, background tasks and containers can be
# running at once, each willing to take every core and as much memory as it can
# get. Without a scope they all run flat out and the desktop lags. The scope
# gives each run a bounded share and a lower priority, so the foreground session
# wins every contention and the machine stays responsive.
#
# Usage: run-limited.sh <cpus> <memory> <command> [args...]
#   cpus   - whole or fractional cores, e.g. "6" or "1.5". "0" disables the cap.
#   memory - systemd MemoryHigh value, e.g. "8G". "0" disables the cap.
#
# "0" for both runs the command directly, with no scope at all. That is the only
# unscoped path — there is no environment override.
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
# container, a CI runner without a user manager, a non-systemd distro) or when
# systemd-run rejects the limits. The caps are a courtesy to the developer's
# machine, never a correctness requirement, so an unavailable systemd must not
# stop the build or the tests.

set -uo pipefail

# ── Settings ──────────────────────────────────────────────────────────────
# Fixed constants, deliberately not environment variables: these are a courtesy
# to the local machine, and a knob per invocation is how limits stop being
# applied at all. Edit a value here if a run needs a different share.
#
# CPUWeight and IOWeight are relative shares — 100 is an ordinary process, so 20
# means the run gets CPU or disk only when nothing else wants them. `nice`
# lowers the command's own priority below everything interactive. None of the
# three can kill a process; they only decide who wins a contention.
cpu_weight=20
io_weight=20
nice_level=10

# Hard ceilings, off by default. MemoryMax OOM-kills whatever exceeds it, so a
# browser E2E run that spikes would die with a confusing failure instead of a
# slow one, and rustc would die mid-link. Set one only if a runaway costs more
# than a kill: memory_max, memory_swap_max (zram/swap), tasks_max (pids).
memory_max=0
memory_swap_max=0
tasks_max=0
# ──────────────────────────────────────────────────────────────────────────

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
    # usable" actually calls for — the goal is to stop a run monopolising the
    # machine, not to enforce a ceiling it must respect.
    args+=(-p "MemoryHigh=${memory}")
fi

# The scheduling limits ride along with the caps. They are what keeps the
# desktop responsive when several runs are going at once, so they apply to every
# scoped run — including a build, which has no hard cap of its own. When the
# caller passes "0" for both caps they asked for no scope at all, and this block
# is skipped so the run really is unconfined. "0" disables an individual knob;
# an empty value would reach systemd as `-p CPUWeight=` and be rejected as an
# invalid assignment.
if [ "${#args[@]}" -gt 0 ]; then
    for spec in \
        "CPUWeight:${cpu_weight}" \
        "IOWeight:${io_weight}" \
        "MemoryMax:${memory_max}" \
        "MemorySwapMax:${memory_swap_max}" \
        "TasksMax:${tasks_max}"
    do
        key="${spec%%:*}"
        value="${spec#*:}"
        if [ -n "$value" ] && [ "$value" != "0" ]; then
            args+=(-p "${key}=${value}")
        fi
    done
fi

# Nothing to enforce: run directly rather than paying for a scope. This is the
# "0 0" path.
if [ "${#args[@]}" -eq 0 ]; then
    exec "$@"
fi

if ! command -v systemd-run >/dev/null 2>&1; then
    echo "[run-limited] systemd-run not found; running without resource caps" >&2
    exec "$@"
fi

# A user manager must be running for --user scopes. In a container or on a
# non-systemd host this fails, and the build/tests should still run.
if ! systemctl --user show-environment >/dev/null 2>&1; then
    echo "[run-limited] no systemd user manager; running without resource caps" >&2
    exec "$@"
fi

# --expand-environment=no passes the command line through verbatim; without it
# systemd-run expands $VAR/${VAR} in our arguments itself. It arrived in systemd
# 254, and an unrecognized option would abort the scope rather than degrade, so
# probe for it instead of assuming.
expand_flag=()
if systemd-run --help 2>&1 | grep -q -- '--expand-environment'; then
    expand_flag=(--expand-environment=no)
fi

echo "[run-limited] cpus=${cpus} memory=${memory} nice=${nice_level} ${args[*]} -- $*" >&2

# Build the command: `nice` only when a level was asked for.
cmd=()
if [ "$nice_level" != "0" ]; then
    cmd=(nice -n "$nice_level")
fi
cmd+=("$@")

# --collect removes the transient unit once it exits, so repeated runs do not
# accumulate failed units. --same-dir keeps the working directory.
# Not --pty: it mangles cargo's output and breaks piping into a log.
# Capture the status directly. Do NOT wrap this in `if ! ...`: inside such a
# block `$?` is the *negated* condition status (always 0), which silently turns
# a failing test suite into a passing one.
systemd-run --user --scope --quiet --collect --same-dir "${expand_flag[@]}" "${args[@]}" -- "${cmd[@]}"
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
