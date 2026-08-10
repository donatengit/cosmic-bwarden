// Make `@playwright/test` resolvable from this directory.
//
// Playwright loads playwright.config.js and the spec files from
// tests/browser-extension/playwright/, so Node resolves their imports by
// walking up from THAT directory — which has no node_modules, because the npm
// project lives in browser-extension/. Without this link the run dies at
// startup with "Cannot find module '@playwright/test'".
//
// A relative symlink to the extension's node_modules is enough, and avoids a
// second package.json + lockfile here. It is gitignored (see the node_modules
// rule in .gitignore), so it must be created on demand: a fresh clone and CI
// both start without it. This ran as a hand-made absolute symlink on one
// machine for months, which is exactly why CI failed the first time the mocked
// E2E ran there.
const fs = require('fs');
const path = require('path');

const linkPath = path.join(__dirname, 'node_modules');
const target = path.join('..', '..', '..', 'browser-extension', 'node_modules');

try {
    // statSync follows symlinks: a healthy link (or a real directory) resolves
    // to browser-extension/node_modules and there is nothing to do.
    if (fs.statSync(linkPath).isDirectory()) {
        process.exit(0);
    }
} catch {
    // Missing, or a dangling symlink (e.g. left by a machine with a different
    // absolute path) — fall through and recreate it.
}

try {
    fs.unlinkSync(linkPath);
} catch {
    // Nothing to remove.
}

fs.symlinkSync(target, linkPath, 'dir');
console.log(`link-deps: ${linkPath} -> ${target}`);
