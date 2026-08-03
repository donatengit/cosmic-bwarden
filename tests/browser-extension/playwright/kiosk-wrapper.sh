#!/bin/bash
# Resolve the repo root from this script's own location
# (tests/browser-extension/playwright/kiosk-wrapper.sh) instead of a
# hardcoded machine-specific path.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
cd "$PROJECT_ROOT/browser-extension"
echo "Kiosk started. Running Playwright..."
"$PROJECT_ROOT/browser-extension/node_modules/.bin/playwright" test --config=../tests/browser-extension/playwright/playwright.config.js --project=firefox-full "$@"
