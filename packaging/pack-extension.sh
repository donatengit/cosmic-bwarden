#!/usr/bin/env bash
# Single source of truth for the distributable browser-extension zip.
#
# Called by `just pack-extension`, by CI on every push, and by the release
# workflow — so the artifact CI exercises is built exactly the way a release
# builds it. It previously existed twice (justfile recipe + an inline `zip` in
# release.yml) and the two had already drifted: the release copy shipped
# package.json and package-lock.json that the justfile and the docs excluded.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
src="$repo_root/browser-extension"
out="$repo_root/target/cosmic-bwarden-extension.zip"

mkdir -p "$repo_root/target"
# `zip -r` UPDATES an existing archive instead of replacing it: without this,
# a file deleted from the extension lingers in every zip built afterwards.
rm -f "$out"

cd "$src"
zip -r "$out" . \
    --exclude "node_modules/*" \
    --exclude "package.json" \
    --exclude "package-lock.json" \
    --exclude "test-results/*" \
    --exclude "*.test.js" \
    --exclude "*.tmp" \
    --exclude ".gitignore" >/dev/null

# The zip is what reaches users and extension stores, so assert its shape
# rather than trusting the exclude list above to stay correct as files move.
contents="$(unzip -Z1 "$out")"
fail=0

while IFS= read -r pattern; do
    if grep -qE "$pattern" <<<"$contents"; then
        echo "pack-extension: FAIL — artifact contains $pattern" >&2
        fail=1
    fi
done <<'PATTERNS'
^node_modules/
^package(-lock)?\.json$
\.test\.js$
^test-results/
PATTERNS

for required in manifest.json background.js content.js popup/popup.html popup/popup.js; do
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
