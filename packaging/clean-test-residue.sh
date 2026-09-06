#!/bin/bash
# Remove leftover `cosmic-bwarden-test-*` profile directories.
#
# A correctly-behaving suite leaves none of these: each test redirects its XDG
# dirs into a tempfile::TempDir, and each browser-extension script removes its
# fixed profile in a cleanup() trap. This script exists for residue from before
# that was true, and as a recovery tool after a run is SIGKILLed (which skips
# every teardown path). It is not part of the normal test loop — if a clean run
# leaves anything behind, fix the test, do not reach for this.
#
# SAFETY. Every path removed must satisfy all of:
#   * basename starts with `cosmic-bwarden-test-`  (so the live `cosmic-bwarden`
#     profile can never match — it has no `-test-` suffix)
#   * is a real directory, not a symlink (rm -rf through a symlink would delete
#     the target)
#   * sits directly under one of the known roots, at depth 1
# Anything else is skipped with a message. Nothing is read from arguments.
#
# Usage:
#   clean-test-residue.sh            # list what would be removed, remove nothing
#   clean-test-residue.sh --apply    # actually remove

set -uo pipefail

apply=0
case "${1:-}" in
    --apply) apply=1 ;;
    "")      apply=0 ;;
    *) echo "usage: $0 [--apply]" >&2; exit 2 ;;
esac

uid=$(id -u)
roots=(
    "${XDG_CONFIG_HOME:-$HOME/.config}"
    "${XDG_CACHE_HOME:-$HOME/.cache}"
    "${XDG_DATA_HOME:-$HOME/.local/share}"
    "${XDG_RUNTIME_DIR:-/run/user/$uid}"
    "${TMPDIR:-/tmp}"
)

found=0
removed=0
skipped=0

for root in "${roots[@]}"; do
    [ -d "$root" ] || continue
    # -maxdepth 1: only direct children, never a nested match.
    while IFS= read -r -d '' path; do
        name=$(basename "$path")

        case "$name" in
            cosmic-bwarden-test-*) ;;
            *)
                echo "skip (not a test profile): $path" >&2
                skipped=$((skipped + 1)); continue ;;
        esac

        if [ -L "$path" ]; then
            echo "skip (symlink): $path" >&2
            skipped=$((skipped + 1)); continue
        fi
        if [ ! -d "$path" ]; then
            echo "skip (not a directory): $path" >&2
            skipped=$((skipped + 1)); continue
        fi

        found=$((found + 1))
        size=$(find "$path" -mindepth 1 2>/dev/null | wc -l)
        if [ "$apply" -eq 1 ]; then
            rm -rf -- "$path" && removed=$((removed + 1))
            echo "removed ($size files): $path"
        else
            echo "would remove ($size files): $path"
        fi
    done < <(find "$root" -maxdepth 1 -name 'cosmic-bwarden-*' -print0 2>/dev/null)
done

echo
if [ "$apply" -eq 1 ]; then
    echo "removed $removed of $found test profile directories ($skipped skipped)"
else
    echo "$found test profile directories would be removed ($skipped skipped)"
    echo "re-run with --apply to remove them"
fi

# Report, never touch, the live profile: it must still be there afterwards.
for root in "${roots[@]}"; do
    if [ -e "$root/cosmic-bwarden" ]; then
        echo "live profile intact: $root/cosmic-bwarden"
    fi
done
