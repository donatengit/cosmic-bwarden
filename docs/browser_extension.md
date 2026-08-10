# Browser Extension

The COSMIC BWarden browser extension provides vault access inside Chrome/Firefox. It is a plain MV3 WebExtension (HTML/CSS/JS — no compilation step) and communicates with the running `cosmic-bwarden-agent` over native messaging.

For architecture details see [`browser_integration.md`](browser_integration.md).

---

## Prerequisites

| Requirement | Purpose |
|---|---|
| `just install` | Installs agent binary and registers the native messaging host |
| `npm install` in `browser-extension/` | Playwright and Vitest dev dependencies |
| Docker or Podman | Full E2E tests spin up a Vaultwarden container |
| `cosmic-comp` | Firefox full E2E tests need an isolated Wayland compositor |

The extension itself requires **no build step** — the files in `browser-extension/` are loaded directly by the browser.

---

## Browser compatibility

`manifest.json` targets MV3 and includes both `background.service_worker` (Chrome) and `background.scripts` (Firefox). Each browser uses the key it understands and ignores the other, so a single manifest works for both without patching.

---

## Loading the extension

### After `just install`

The native messaging host is registered automatically as part of `just install` / `just clean-install` / `just user-install`. No extra steps are needed.

### Chrome / Chromium

1. Open `chrome://extensions`
2. Enable **Developer mode**
3. Click **Load unpacked** → select `browser-extension/`

### Firefox

1. Open `about:debugging#/runtime/this-firefox`
2. Click **Load Temporary Add-on** → select `browser-extension/manifest.json`

For a permanent install, package the extension with `just pack-extension` and submit to AMO.

---

## Native messaging host

The agent binary acts as the native messaging host (`com.enikeev.cosmic_bwarden`). Registration writes a wrapper script and a manifest into `~/.mozilla/native-messaging-hosts/`.

**Post-install (normal users):** registration is done automatically by `just install`.

**Development (without a full install):** use the dev recipe, which builds only the agent in debug mode and points the manifest at `target/debug/`:

```bash
just register-browser-host
```

Chrome/Chromium registration is handled automatically during Playwright E2E tests — the test suite writes a manifest into the Playwright user-data directory at startup.

---

## Justfile targets

| Target | What it does |
|---|---|
| `just install` | Full system install — binaries, service, and native messaging host |
| `just pack-extension` | Zip production files → `target/cosmic-bwarden-extension.zip` |
| `just test-extension-setup` | `npm install` in `browser-extension/` |
| `just test-extension-unit` | Run Vitest unit tests (popup logic, no browser) |
| `just test-extension-e2e` | Playwright firefox-mock project (mocked native messaging) |
| `just test-extension-e2e-chrome` | Build + start Vaultwarden + agent → Chrome E2E (Playwright) |
| `just test-extension-e2e-full` | Same but Firefox full E2E inside isolated `cosmic-comp` compositor |
| `just test-extension-e2e-debug` | Playwright UI mode for interactive debugging |
| `just register-browser-host` | **[Dev]** Debug-build agent + register host (no full install needed) |

---

## Test projects

| Project | Spec file | Native messaging | Status |
|---|---|---|---|
| `chrome-full` | `chrome-full.spec.js` | Real (Playwright Chromium + agent) | Passing |
| `firefox-mock` | non-`full` specs | Mocked (page.evaluate shims) | Passing |
| `firefox-full` | `full.spec.js` | Real (Firefox + agent) | Failing — Firefox debugging protocol cannot attach to MV3 background pages; manual loading works fine |

### Running Chrome E2E manually

```bash
just test-extension-e2e-chrome
```

The script (`tests/browser-extension/run-chrome-e2e.sh`) handles everything:

1. Builds `cosmic-bwarden-agent` and `cosmic-bwarden-cli`
2. Starts a Vaultwarden container on port 8081 (Docker or Podman auto-detected)
3. Starts the agent under `COSMIC_BWARDEN_PROFILE=test-chrome-e2e`
4. Runs `npx playwright test --project=chrome-full`
5. Cleans up agent and container on exit

Environment variables used by the test suite:

| Variable | Default in script | Description |
|---|---|---|
| `VW_URL` | `http://localhost:8081` | Vaultwarden base URL |
| `VW_EMAIL` | `test-chrome@example.com` | Test account email |
| `VW_PASSWORD` | `password123` | Test account password |
| `COSMIC_BWARDEN_PROFILE` | `test-chrome-e2e` | Agent profile (isolates test data) |

### Running unit tests

```bash
just test-extension-setup   # first time only
just test-extension-unit
```

---

## E2E test coverage

The `chrome-full` suite (`tests/browser-extension/playwright/chrome-full.spec.js`) covers:

- Vault list rendering (entry names, types)
- Password copy via detail view (clipboard mock)
- Autofill via content script (Login entries)
- Non-latin / emoji data in all fields (Cyrillic, Japanese, emoji names)
- Lock → Unlock lifecycle (popup shows locked/unlocked state correctly)
- Logout → Login lifecycle
- Create **Login** entry via popup form
- Create **Card** entry via popup form (unicode cardholder)
- Create **Identity** entry via popup form (emoji in name)
- **SecureNote** created via CLI, viewed in popup
- **SshKey** created via CLI (`add-ssh-key`), public key displayed + copyable in popup
- Edit entry name and fields via popup form
- Delete entry via popup detail view
- Search / filter entries by name

---

## Entry types in the popup

| Type | Create via popup | View in popup | Edit via popup |
|---|---|---|---|
| Login | Yes | Yes | Yes |
| Card | Yes | Yes | Yes |
| Identity | Yes | Yes | Yes |
| SecureNote | Yes | Yes | Yes |
| SshKey | **No** | Yes (public key + fingerprint only) | No |

SSH keys must be created via the CLI (`cosmic-bwarden-cli add-ssh-key`) or the native COSMIC UI. The popup form intentionally has no SSH key type option — entering private key material via a browser popup is a security anti-pattern.

---

## Packaging

```bash
just pack-extension
```

Produces `target/cosmic-bwarden-extension.zip` — alongside the Rust build artifacts, already covered by `.gitignore`. The zip contains only the production extension files (`manifest.json`, `background.js`, `content.js`, `popup/`); dev files (`node_modules/`, `package.json`, `package-lock.json`, `test-results/`, `*.test.js`, `*.tmp`, `.gitignore`) are excluded.

The packing logic lives in [`packaging/pack-extension.sh`](../packaging/pack-extension.sh) and is the *only* copy: `just pack-extension`, the CI `extension` job (every push, artifact uploaded), and `release.yml` (on a tag) all invoke it. It removes any previous zip first — `zip -r` updates an existing archive rather than replacing it, so a deleted file would otherwise survive in later builds — and then asserts the result: no dev files, all required files present, `manifest.json` parses.

The zip is suitable for uploading to the Chrome Web Store or Firefox Add-ons (AMO). The extension ID in `manifest.json` is `cosmic-bwarden@enikeev.com` (Firefox) — Chrome assigns its own ID on first load.
