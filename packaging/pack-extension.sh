#!/usr/bin/env bash
# Single source of truth for the distributable browser-extension zip.
#
# Called by `just pack-extension`, by CI on every push, and by the release
# workflow — so the artifact CI exercises is built exactly the way a release
# builds it. It previously existed twice (justfile recipe + an inline `zip` in
# release.yml) and the two had already drifted: the release copy shipped
# package.json and package-lock.json that the justfile and the docs excluded.
#
# File selection is an ALLOWLIST: only the preselected production files below
# are zipped. The old blocklist approach (`zip -r .` minus excludes) shipped
# whatever new files appeared in browser-extension/ until someone remembered
# to exclude them — including browser-extension/.env with real AMO credentials
# once that file came to exist. Nothing that is not listed here ships.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
src="$repo_root/browser-extension"
out="$repo_root/target/cosmic-bwarden-extension.zip"

mkdir -p "$repo_root/target"
# `zip -r` UPDATES an existing archive instead of replacing it: without this,
# a file deleted from the extension lingers in every zip built afterwards.
rm -f "$out"

cd "$src"
# Preselected production files, enumerated explicitly so dotfiles, tests,
# package.json, and node_modules can never match. (Whole-directory inclusion
# shipped popup/*.test.js the moment it was tried.) icons/ ships whole as a
# curated asset dir. ONE list drives both the zip and the assertion below:
# `zip` exits 0 even when some named files are missing, so a listed file that
# was deleted or renamed must be caught by the required-file loop instead of
# shipping a manifest that references nothing.
allowlist="manifest.json background.js background-save.js content.js content-heuristics.js content-submit.js content-bar.js content-generate.js popup/popup.css popup/popup-detail.js popup/popup-edit.js popup/popup.html popup/popup.js popup/popup-list-actions.js popup/popup-lock.js popup/popup-state.js"
# shellcheck disable=SC2086
zip -r "$out" $allowlist icons >/dev/null

# The zip is what reaches users and extension stores, so assert its shape
# rather than trusting the allowlist above to stay correct as files move.
contents="$(unzip -Z1 "$out")"
fail=0

while IFS= read -r pattern; do
    if grep -qE "$pattern" <<<"$contents"; then
        echo "pack-extension: FAIL — artifact contains $pattern" >&2
        fail=1
    fi
done <<'PATTERNS'
(^|/)\.env($|[./])
(^|/)node_modules/
^package(-lock)?\.json$
\.test\.js$
(^|/)test-results/
PATTERNS

# Every allowlisted file must be present (zip exits 0 on partial matches).
# shellcheck disable=SC2086
for required in $allowlist icons/black16.png icons/black32.png icons/black64.png icons/black128.png; do
    if ! grep -qxF "$required" <<<"$contents"; then
        echo "pack-extension: FAIL — artifact is missing $required" >&2
        fail=1
    fi
done

if ! python3 -c "import json; json.load(open('manifest.json'))" 2>/dev/null; then
    echo "pack-extension: FAIL — manifest.json is not valid JSON" >&2
    fail=1
fi

[ "$fail" -eq 0 ] || exit 1
echo "pack-extension: $out ($(wc -l <<<"$contents") entries)"
