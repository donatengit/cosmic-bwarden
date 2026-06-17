#!/bin/bash
PROJECT_ROOT="$HOME/src/corbw"
cd "$PROJECT_ROOT/browser-extension"
echo "Kiosk started. Running Playwright..."
"$PROJECT_ROOT/browser-extension/node_modules/.bin/playwright" test --config=../tests/browser-extension/playwright/playwright.config.js --project=firefox-full "$@"
