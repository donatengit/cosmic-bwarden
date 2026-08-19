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
| `npm run e2e:link` | Creates the `node_modules` symlink the spec directory needs to resolve `@playwright/test`. Run automatically by every E2E entry point — see [`link-deps.js`](../tests/browser-extension/playwright/link-deps.js); you should never need it directly. |
| `just test-extension-e2e-chrome` | Build + start Vaultwarden + agent → Chrome E2E (Playwright) |
| `just test-extension-e2e-full` | Same but Firefox full E2E inside isolated `cosmic-comp` compositor |
| `just test-extension-e2e-debug` | Playwright UI mode for interactive debugging |
| `just register-browser-host` | **[Dev]** Debug-build agent + register host (no full install needed) |

---

## Test projects

| Project | Spec file | Native messaging | Status |
|---|---|---|---|
| `chrome-full` | `chrome-full.spec.js` | Real (Playwright Chromium + agent) | Passing |
| `firefox-mock` | non-`full` specs | Mocked (page.evaluate shims) | Passing — runs in CI on every push (headless: the project sets `headless: !!process.env.CI`, and these specs never install the extension) |
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

Produces `target/cosmic-bwarden-extension.zip` — alongside the Rust build artifacts, already covered by `.gitignore`. The zip contains only the **preselected production files**: an explicit allowlist (`manifest.json`, the `background*.js` and `content*.js` sets, every `popup/` file enumerated, `icons/`). Nothing that is not listed can ship — the previous exclude-list approach (`zip -r .` minus excludes) let `browser-extension/.env` leak into the artifact once that file came to exist.

The packing logic lives in [`packaging/pack-extension.sh`](../packaging/pack-extension.sh) and is the *only* copy: `just pack-extension`, the CI `extension` job (every push, artifact uploaded), and `release.yml` (on a tag) all invoke it. It removes any previous zip first — `zip -r` updates an existing archive rather than replacing it, so a deleted file would otherwise survive in later builds — a listed file that does not exist fails the build, and the result is asserted: no dev or secrets files, all required files present, `manifest.json` parses.

The zip is suitable for uploading to the Chrome Web Store or Firefox Add-ons (AMO). The extension ID in `manifest.json` is `cosmic-bwarden@enikeev.com` (Firefox) — Chrome assigns its own ID on first load.

## Self-hosted release pipeline (unlisted AMO channel)

`just sign-extension` is the single user-facing target: it stages the
production files, lints them, signs on the AMO `unlisted` channel, and leaves
a ready-to-install XPI in gitignored `dist/`:

```bash
just sign-extension
# → dist/cosmic-bwarden-<version>.xpi
```

**Dev signing**: the current files are signed under a fresh **timestamp
version** (`YYYY.M.D.mmm`, minutes-of-day as the last component — valid for
AMO, `web-ext lint`, and Chrome) injected into the staged manifest. No git
tag, manifest-version match, or clean-tree requirement — this is for testing
on real AMO infrastructure. With `EXT_UPDATE_BASE_URL` set, the same run bakes
the gecko `update_url` into the XPI and appends the version to
`dist/updates.json` (the Firefox update manifest). Output paths print one
absolute path per line for CI to capture.

The strict release preflight (tag `vYYYY.MM.P` on HEAD, clean tree) is what CI
uses: `EXT_SIGN_MODE=release just sign-extension` (or
`node packaging/ext-release.mjs preflight` without `--dev`). The tag is the
version — the pipeline injects it into the staged manifest at sign time, so the
committed `manifest.json` version needs no release bumps (it only matters for
manual store uploads via the packed zip).

## CI/CD (GitHub Actions)

Every `v*` tag triggers `.github/workflows/release.yml`, whose
`sign-extension` job runs the same pipeline as a maintainer would:

- **Repository secrets** `WEB_EXT_API_KEY` / `WEB_EXT_API_SECRET`
  (generated at <https://addons.mozilla.org/developers/addon/api/key/>) are
  injected as environment variables only.
- **Update hosting (default: GitHub Releases)**: `update_url` is baked into
  the XPI as
  `https://github.com/<owner>/<repo>/releases/latest/download/updates.json` —
  a stable, always-current manifest location that every installed version
  polls. `update_link` entries are tag-scoped
  (`…/releases/download/<tag>/cosmic-bwarden-<version>.xpi`) so the
  `update_hash` always describes the exact immutable file Firefox downloads;
  a moving "latest" link would break hash verification for older entries.
  Each release also carries a stable-named copy
  (`cosmic-bwarden-firefox.xpi`) for manual "latest build" downloads. Note:
  `releases/latest/…` only resolves once the draft release is **published** —
  publishing is the go-live step. The repository variable
  `EXT_UPDATE_BASE_URL` overrides the base for non-GitHub hosting; the job
  fetches the currently published `updates.json` first, so each release
  appends to the real update manifest instead of overwriting it (the first
  release starts empty).
- **Release discipline**: just tag `vYYYY.MM.P` — the tag is the version and
  is injected into the staged manifest at sign time, so
  `browser-extension/manifest.json` needs no bumping. A failed preflight
  uploads nothing to AMO, so it consumes no version.
- **Outputs**: the signed XPI is uploaded as a workflow artifact
  (`cosmic-bwarden-signed-xpi`) and attached to the draft GitHub release next
  to the `.deb` and the source zip.

- **Credentials**: read from the environment, or from the local gitignored
  `browser-extension/.env` (template: `browser-extension/.env.example`,
  `chmod 600`; loader:
  [`packaging/load-ext-env.sh`](../packaging/load-ext-env.sh)). Generate them
  at <https://addons.mozilla.org/developers/addon/api/key/>. Variables already
  exported in the shell take precedence over the file. The file is parse-only
  (plain `KEY=VALUE` lines, values exported literally, never evaluated) and
  must not be world-readable. Credentials reach `web-ext` only via a generated
  config trampoline that references `process.env` — they never appear on a
  command line or in trace output. (As with any environment-variable
  credential, another process of the same user could read them from the
  signing process's environment — that is inherent to env-based auth.)
- **Opt-in by design**: `sign-extension` is never part of `default`; it
  requires network access (and AMO credentials — see above).
- **Version**: dev signing uses a fresh timestamp (`YYYY.M.D.mmm`), so every
  run is unique; re-signing the same minute hits the duplicate guard (a
  version already shipped in `dist/` is refused — AMO rejects duplicate
  versions). Releases (CI later) use the `vYYYY.MM.P` git tag on HEAD (the
  same tag that triggers `release.yml`); `manifest.json`'s `version` must
  match the tag with the `v` stripped.
- **web-ext** is pinned exactly (`10.6.0`) in `browser-extension/package.json`
  and version-checked at target entry — v8+ removed `--use-submission-api`,
  `--api-url-prefix`, and `--id` and made `--channel` mandatory, so a version
  mismatch fails loudly instead of passing flags that silently don't exist.
- **update_url**: `sign-extension` injects
  `browser_specific_settings.gecko.update_url = <EXT_UPDATE_BASE_URL>/updates.json`
  into the staged manifest (the signed artifact must carry it for Firefox to
  check for updates). A hardcoded *different* URL in the source manifest is a
  hard error; an unset `EXT_UPDATE_BASE_URL` is a warning — the XPI still
  signs, but without an `update_url` self-hosted updates won't work and no
  `updates.json` is written. `EXT_UPDATE_BASE_URL` must be a plain `https://`
  URL (no embedded credentials, query, or fragment) — it is baked into the
  signed artifact and every `update_link`.
- **Never add `--verbose` to the sign command**: web-ext logs its resolved
  config (including credential values) at verbose level.
- **Timeouts / polling**: web-ext polls the AMO version-detail API every
  second, and its `--timeout` covers both the upload validation and the wait
  for approval before the signed XPI is downloaded. Unlisted submissions
  receive a "tentatively approved" review that can take minutes, so the
  default window is 30 minutes (`EXT_SIGN_TIMEOUT_MS` to override).
- **Resume**: if a run is interrupted or times out, re-running
  `just sign-extension` resumes the *same* AMO submission instead of
  uploading a new version — the upload uuid is persisted at
  `target/ext-sign-upload-uuid`, and web-ext reuses it while the staged XPI is
  unchanged (it re-uploads automatically if the files changed). Delete
  `target/ext-sign-upload-uuid` to force a fresh upload. A timed-out
  submission may still be approved afterwards at AMO; its XPI can also be
  downloaded from the Dev Hub (Manage Status & Versions).
- **`updates.json`** entries carry `version`, `update_link`
  (`<EXT_UPDATE_BASE_URL>/cosmic-bwarden-<version>.xpi`), and `update_hash`
  (`sha256:<hex>` of the signed XPI); appending a new version preserves every
  existing entry.

The version preflight, `updates.json` generation, sha256 computation, and
`update_url` injection are pure, dependency-free functions in
[`packaging/ext-release.mjs`](../packaging/ext-release.mjs), unit-tested offline
with `just test-ext-release` (`node --test`) — no network, no AMO credentials.
The `web-ext sign` call itself is the only networked step and is a thin shell
(the `sign-extension` recipe) around that tested logic.
