#!/bin/bash
# Run a command inside a transient systemd scope that caps its CPU and memory
# and lowers its priority, so a long build or test run leaves the desktop usable.
#
# Why this exists: several agents, background tasks and containers can be
# running at once, each willing to take every core and as much memory as it can
# get. Without a scope they all run flat out and the desktop lags. The scope
# gives each run a bounded share and a lower priority, so the foreground session
# wins every contention and the machine stays responsive.
#
# Usage: run-limited.sh <cpus> <memory> <command> [args...]
#   cpus   - whole or fractional cores, e.g. "6" or "1.5". Must be positive.
#   memory - systemd MemoryHigh value, e.g. "8G" or "512M". Must be positive.
#
# Both are required and neither may be zero: every run through this wrapper is
# capped. There is no environment override and no opt-out — to change what a run
# is allowed, edit the settings below, which is also where the scheduling shares
# live. A "0" here would mean "no cap", which is the thing this script exists to
# prevent, so it is rejected rather than honoured.
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
# The one unwrapped path is an environment that cannot make a scope: no
# systemd-run, no systemd user manager, or a rejected scope. That is a missing
# facility, not an opt-out, and it is reported on stderr. The caps are a
# courtesy to the developer's machine rather than a correctness requirement, so
# a machine without systemd still builds and tests.

set -uo pipefail

# ── Settings ──────────────────────────────────────────────────────────────
# Fixed constants, deliberately not environment variables: this is a courtesy
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

# Hard ceilings, off by default. CPUQuota and MemoryHigh above already bound what
# a run may take; these bound it in the way that *kills*. MemoryMax OOM-kills
# whatever exceeds it, so a browser E2E run that spikes would die with a
# confusing failure instead of a slow one, and rustc would die mid-link. Set one
# only if a runaway costs more than a kill: memory_max, memory_swap_max
# (zram/swap), tasks_max (pids).
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

if ! [[ "$cpus" =~ ^[0-9]+(\.[0-9]+)?$ ]]; then
    echo "[run-limited] invalid cpus '$cpus': expected a number such as 6 or 1.5." >&2
    echo "              Check the test_cpus justfile variable." >&2
    exit 2
fi
if ! [[ "$memory" =~ ^[0-9]+(\.[0-9]+)?[KMGT]?$ ]]; then
    echo "[run-limited] invalid memory '$memory': expected a size such as 8G or 512M." >&2
    echo "              Check the test_memory justfile variable." >&2
    exit 2
fi

# Reject "0" rather than treating it as "no cap". An empty `just` variable
# (`just test_cpus= test`) would also shift the arguments by one and silently
# reinterpret the memory value as the CPU count and the first word of the
# command as the memory value, producing "Failed to find executable" instead of
# naming the mistake, so both are caught here.
if ! awk -v c="$cpus" 'BEGIN { exit !(c + 0 > 0) }'; then
    echo "[run-limited] cpus '$cpus' does not cap anything: every run is capped." >&2
    echo "              Pass a positive core count, or edit cpu_weight/settings in $0." >&2
    exit 2
fi
memory_number="${memory%[KMGT]}"
if ! awk -v m="$memory_number" 'BEGIN { exit !(m + 0 > 0) }'; then
    echo "[run-limited] memory '$memory' does not cap anything: every run is capped." >&2
    echo "              Pass a positive size such as 8G, or edit the settings in $0." >&2
    exit 2
fi

args=()

# systemd expresses CPUQuota in percent of ONE core: 100% = 1 core.
quota=$(awk -v c="$cpus" 'BEGIN { printf "%d", c * 100 }')
args+=(-p "CPUQuota=${quota}%")

# MemoryHigh, not MemoryMax. MemoryMax is a hard limit: the kernel OOM-kills
# processes that exceed it, so a browser E2E run (nested compositor + Firefox +
# agent) that spikes past the value would die with a confusing failure rather
# than a slow one. MemoryHigh throttles the cgroup and pushes it into reclaim
# instead, which is what "keep the desktop usable" actually calls for — the goal
# is to stop a run monopolising the machine, not to enforce a ceiling it must
# respect.
args+=(-p "MemoryHigh=${memory}")

# The scheduling shares ride along with the caps: capping CPU and memory alone
# still lets a run take everything it is allowed the moment the machine is idle,
# whereas the shares mean it only ever uses what nothing else wants. "0" turns
# an individual knob off; an empty value would reach systemd as
# `-p CPUWeight=` and be rejected as an invalid assignment.
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
