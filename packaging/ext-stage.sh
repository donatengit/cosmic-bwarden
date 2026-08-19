#!/usr/bin/env bash
# Stage the exact production file set of the browser extension into a clean
# directory for web-ext (lint/sign).
#
# File selection deliberately has ONE source of truth: the zip built by
# packaging/pack-extension.sh, whose exclude list and shape assertions already
# run in CI on every push. Staging unzips that artifact instead of repeating
# the exclusion list a second time (a second copy is how package.json drifted
# into a release zip once — see pack-extension.sh's header).
#
# Usage: ext-stage.sh [stage-dir]   (default: target/ext-stage)
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
stage_dir="$(realpath -m "${1:-$repo_root/target/ext-stage}")"
zip_path="$repo_root/target/cosmic-bwarden-extension.zip"

# The stage dir is wiped with rm -rf below: confine it to the repo (the
# justfile never passes an argument; this guards direct invocations).
case "$stage_dir" in
    "$repo_root"/*) ;;
    *) echo "ext-stage: FAIL — stage dir $stage_dir is outside the repo" >&2; exit 1 ;;
esac

# Build (or refresh) the canonical zip — also validates its shape.
"$repo_root/packaging/pack-extension.sh"

# Reproducible staging: wipe every run so deleted source files cannot linger.
rm -rf "$stage_dir"
mkdir -p "$stage_dir"

# Defense in depth BEFORE extracting: refuse path-traversal entries (the zip
# is built from the repo by pack-extension.sh, which is the same trust domain,
# but ../-entries would let staging write outside the stage dir if that ever
# changed).
if unzip -Z1 "$zip_path" | grep -Eq '(^|/)\.\.(/|$)'; then
    echo "ext-stage: FAIL — zip contains a path-traversal entry" >&2
    exit 1
fi

unzip -q "$zip_path" -d "$stage_dir"

# Belt and braces behind pack-extension.sh's exclusions: secrets files must
# never survive into the staged artifact that web-ext signs.
find "$stage_dir" -maxdepth 1 -name ".env*" -exec rm -f {} +

# Normalize mtimes so the zip web-ext builds from this directory embeds no
# checkout-time timestamps (zip stores file mtimes; web-ext zips from here).
find "$stage_dir" -type f -exec touch -d @0 {} +

[ -f "$stage_dir/manifest.json" ] || {
    echo "ext-stage: FAIL — staged output has no manifest.json" >&2
    exit 1
}

echo "ext-stage: $stage_dir"
