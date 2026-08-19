# Run every recipe under bash, not POSIX sh: GitHub runners (ubuntu-latest)
# have sh = dash, which rejects `set -o pipefail` and other bashisms the
# recipes rely on. This matches what developers run locally.
set shell := ["bash", "-cu"]

prefix := "/usr/local"
bin_dir := prefix + "/bin"
share_dir := prefix + "/share"
applets_dir := share_dir + "/cosmic/applets"
apps_dir := share_dir + "/applications"
metainfo_dir := share_dir + "/metainfo"
icons_dir := share_dir + "/icons/hicolor"
systemd_user_dir := "/usr/lib/systemd/user"

# Resolve the invoking user's home dir even when run via `sudo just ...`,
# so user-local install/uninstall paths don't end up under /root.
real_home := `if [ -n "${SUDO_USER:-}" ]; then getent passwd "$SUDO_USER" | cut -d: -f6; else echo "$HOME"; fi`
local_share := real_home + "/.local/share"
local_applets := local_share + "/cosmic/applets"
local_apps := local_share + "/applications"
local_metainfo := local_share + "/metainfo"
local_icons := local_share + "/icons/hicolor"

# Auto-detect TPM2 support: enable the agent's `tpm` feature when libtss2-esys is present.
_tpm_features := `pkg-config --exists tss2-esys 2>/dev/null && echo '--features cosmic-bwarden-agent/tpm' || true`

# Default task: build the project
default: build

# Build all components in release mode
build:
    if [ -n "${SUDO_USER:-}" ]; then \
        echo "Detected sudo, running cargo build as $SUDO_USER..."; \
        sudo -u "$SUDO_USER" env PATH="$PATH" RUSTFLAGS="-C target-cpu=native" cargo build --release --quiet {{_tpm_features}}; \
    else \
        RUSTFLAGS="-C target-cpu=native" cargo build --release --quiet {{_tpm_features}}; \
    fi

# Install binaries, desktop entry, and COSMIC applet metadata system-wide
install: build
    echo "Installing binaries..."
    install -Dm755 target/release/cosmic-bwarden-agent {{bin_dir}}/cosmic-bwarden-agent
    install -Dm755 target/release/cosmic-applet-bwarden {{bin_dir}}/cosmic-applet-bwarden
    install -Dm755 target/release/cosmic-bwarden-cli {{bin_dir}}/cosmic-bwarden-cli
    
    echo "Installing desktop entry..."
    install -Dm644 crates/cosmic-bwarden-ui/resources/com.enikeev.cosmic_bwarden.desktop {{apps_dir}}/com.enikeev.cosmic_bwarden.desktop

    echo "Installing AppStream metainfo..."
    install -Dm644 crates/cosmic-bwarden-ui/resources/com.enikeev.cosmic_bwarden.metainfo.xml {{metainfo_dir}}/com.enikeev.cosmic_bwarden.metainfo.xml

    echo "Installing application icon..."
    install -Dm644 icons/black.svg {{icons_dir}}/scalable/apps/com.enikeev.cosmic_bwarden.svg
    install -Dm644 icons/black16.png {{icons_dir}}/16x16/apps/com.enikeev.cosmic_bwarden.png
    install -Dm644 icons/black32.png {{icons_dir}}/32x32/apps/com.enikeev.cosmic_bwarden.png
    install -Dm644 icons/black64.png {{icons_dir}}/64x64/apps/com.enikeev.cosmic_bwarden.png
    install -Dm644 icons/black128.png {{icons_dir}}/128x128/apps/com.enikeev.cosmic_bwarden.png
    install -Dm644 crates/cosmic-bwarden-ui/resources/icons/cosmic-bwarden-symbolic.svg {{icons_dir}}/scalable/apps/com.enikeev.cosmic_bwarden-symbolic.svg

    echo "Installing COSMIC applet metadata..."
    mkdir -p {{applets_dir}}
    echo '( name: "COSMIC BWarden", description: "Secure Bitwarden client for COSMIC", identifier: "com.enikeev.cosmic_bwarden", icon: "com.enikeev.cosmic_bwarden-symbolic", )' > {{applets_dir}}/com.enikeev.cosmic_bwarden.ron

    echo "Installing systemd user service..."
    mkdir -p {{systemd_user_dir}}
    sed "s|@BINDIR@|{{bin_dir}}|g" crates/cosmic-bwarden-agent/res/cosmic-bwarden-agent.service > /tmp/cosmic-bwarden-agent.service
    install -Dm644 /tmp/cosmic-bwarden-agent.service {{systemd_user_dir}}/cosmic-bwarden-agent.service
    rm /tmp/cosmic-bwarden-agent.service
    echo "Reloading systemd user daemon..."
    if [ -n "$SUDO_USER" ]; then \
        sudo -u "$SUDO_USER" XDG_RUNTIME_DIR="/run/user/$(id -u $SUDO_USER)" DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$(id -u $SUDO_USER)/bus" systemctl --user daemon-reload; \
    else \
        systemctl --user daemon-reload; \
    fi
    if [ -n "{{_tpm_features}}" ]; then \
        TARGET_USER="${SUDO_USER:-$USER}"; \
        if ! id -nG "$TARGET_USER" 2>/dev/null | grep -qw tss; then \
            echo "TPM2 support compiled in — adding $TARGET_USER to the 'tss' group..."; \
            usermod -aG tss "$TARGET_USER" && \
            echo "NOTE: log out and back in (or run 'newgrp tss') for TPM access to take effect."; \
        else \
            echo "TPM2 support compiled in — $TARGET_USER is already in the 'tss' group."; \
        fi; \
    fi
    echo "Registering Firefox native messaging host..."
    sudo -u "${SUDO_USER:-$USER}" python3 tests/browser-extension/register_host.py \
        --agent-path {{bin_dir}}/cosmic-bwarden-agent \
        --home {{real_home}}
    echo "Done. Please run 'just restart-panel' and 'just enable-agent'."

# Perform a completely fresh installation (removes old files first)
clean-install: uninstall build
    echo "Installing binaries..."
    sudo install -Dm755 target/release/cosmic-bwarden-agent {{bin_dir}}/cosmic-bwarden-agent
    sudo install -Dm755 target/release/cosmic-applet-bwarden {{bin_dir}}/cosmic-applet-bwarden
    sudo install -Dm755 target/release/cosmic-bwarden-cli {{bin_dir}}/cosmic-bwarden-cli

    echo "Installing desktop entry..."
    sudo install -Dm644 crates/cosmic-bwarden-ui/resources/com.enikeev.cosmic_bwarden.desktop {{apps_dir}}/com.enikeev.cosmic_bwarden.desktop

    echo "Installing AppStream metainfo..."
    sudo install -Dm644 crates/cosmic-bwarden-ui/resources/com.enikeev.cosmic_bwarden.metainfo.xml {{metainfo_dir}}/com.enikeev.cosmic_bwarden.metainfo.xml

    echo "Installing application icon..."
    sudo install -Dm644 icons/black.svg {{icons_dir}}/scalable/apps/com.enikeev.cosmic_bwarden.svg
    sudo install -Dm644 icons/black16.png {{icons_dir}}/16x16/apps/com.enikeev.cosmic_bwarden.png
    sudo install -Dm644 icons/black32.png {{icons_dir}}/32x32/apps/com.enikeev.cosmic_bwarden.png
    sudo install -Dm644 icons/black64.png {{icons_dir}}/64x64/apps/com.enikeev.cosmic_bwarden.png
    sudo install -Dm644 icons/black128.png {{icons_dir}}/128x128/apps/com.enikeev.cosmic_bwarden.png
    sudo install -Dm644 crates/cosmic-bwarden-ui/resources/icons/cosmic-bwarden-symbolic.svg {{icons_dir}}/scalable/apps/com.enikeev.cosmic_bwarden-symbolic.svg

    echo "Installing COSMIC applet metadata..."
    sudo mkdir -p {{applets_dir}}
    sudo sh -c "echo '( name: \"COSMIC BWarden\", description: \"Secure Bitwarden client for COSMIC\", identifier: \"com.enikeev.cosmic_bwarden\", icon: \"com.enikeev.cosmic_bwarden-symbolic\" )' > {{applets_dir}}/com.enikeev.cosmic_bwarden.ron"

    echo "Installing systemd user service..."
    sudo mkdir -p {{systemd_user_dir}}
    sed "s|@BINDIR@|{{bin_dir}}|g" crates/cosmic-bwarden-agent/res/cosmic-bwarden-agent.service > /tmp/cosmic-bwarden-agent.service
    sudo install -Dm644 /tmp/cosmic-bwarden-agent.service {{systemd_user_dir}}/cosmic-bwarden-agent.service
    rm /tmp/cosmic-bwarden-agent.service
    echo "Reloading systemd user daemon..."
    if [ -n "$SUDO_USER" ]; then \
        sudo -u "$SUDO_USER" XDG_RUNTIME_DIR="/run/user/$(id -u $SUDO_USER)" DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$(id -u $SUDO_USER)/bus" systemctl --user daemon-reload; \
    else \
        systemctl --user daemon-reload; \
    fi
    if [ -n "{{_tpm_features}}" ]; then \
        TARGET_USER="${SUDO_USER:-$USER}"; \
        if ! id -nG "$TARGET_USER" 2>/dev/null | grep -qw tss; then \
            echo "TPM2 support compiled in — adding $TARGET_USER to the 'tss' group..."; \
            usermod -aG tss "$TARGET_USER" && \
            echo "NOTE: log out and back in (or run 'newgrp tss') for TPM access to take effect."; \
        else \
            echo "TPM2 support compiled in — $TARGET_USER is already in the 'tss' group."; \
        fi; \
    fi
    echo "Registering Firefox native messaging host..."
    sudo -u "${SUDO_USER:-$USER}" python3 tests/browser-extension/register_host.py \
        --agent-path {{bin_dir}}/cosmic-bwarden-agent \
        --home {{real_home}}
    echo "Done. Please run 'just restart-panel' and 'just enable-agent'."

# Install metadata and desktop files for the current user (only if not installing system-wide)
user-install: build
    echo "Warning: user-install may conflict with system-wide install. Running uninstall first..."
    just uninstall
    echo "Installing desktop entry to local apps..."
    mkdir -p {{local_apps}}
    install -Dm644 crates/cosmic-bwarden-ui/resources/com.enikeev.cosmic_bwarden.desktop {{local_apps}}/com.enikeev.cosmic_bwarden.desktop

    echo "Installing AppStream metainfo to local metainfo..."
    install -Dm644 crates/cosmic-bwarden-ui/resources/com.enikeev.cosmic_bwarden.metainfo.xml {{local_metainfo}}/com.enikeev.cosmic_bwarden.metainfo.xml

    echo "Installing application icon to local icons..."
    install -Dm644 icons/black.svg {{local_icons}}/scalable/apps/com.enikeev.cosmic_bwarden.svg
    install -Dm644 icons/black16.png {{local_icons}}/16x16/apps/com.enikeev.cosmic_bwarden.png
    install -Dm644 icons/black32.png {{local_icons}}/32x32/apps/com.enikeev.cosmic_bwarden.png
    install -Dm644 icons/black64.png {{local_icons}}/64x64/apps/com.enikeev.cosmic_bwarden.png
    install -Dm644 icons/black128.png {{local_icons}}/128x128/apps/com.enikeev.cosmic_bwarden.png
    install -Dm644 crates/cosmic-bwarden-ui/resources/icons/cosmic-bwarden-symbolic.svg {{local_icons}}/scalable/apps/com.enikeev.cosmic_bwarden-symbolic.svg

    echo "Installing binaries to user bin..."
    mkdir -p {{local_share}}/../bin
    install -Dm755 target/release/cosmic-bwarden-agent {{local_share}}/../bin/cosmic-bwarden-agent
    install -Dm755 target/release/cosmic-applet-bwarden {{local_share}}/../bin/cosmic-applet-bwarden
    install -Dm755 target/release/cosmic-bwarden-cli {{local_share}}/../bin/cosmic-bwarden-cli

    echo "Installing COSMIC applet metadata to local applets..."
    mkdir -p {{local_applets}}
    echo '( name: "COSMIC BWarden", description: "Secure Bitwarden client for COSMIC", identifier: "com.enikeev.cosmic_bwarden", icon: "com.enikeev.cosmic_bwarden-symbolic", )' > {{local_applets}}/com.enikeev.cosmic_bwarden.ron
    echo "Registering Firefox native messaging host..."
    python3 tests/browser-extension/register_host.py \
        --agent-path {{local_share}}/../bin/cosmic-bwarden-agent \
        --home {{real_home}}
    echo "Done. Please run 'just restart-panel'."



# Restart the COSMIC panel to discover new applets
restart-panel:
    systemctl --user restart cosmic-panel

# Enable and start the agent service for the current user
enable-agent:
    systemctl --user enable --now cosmic-bwarden-agent

# Disable and stop the agent service for the current user
disable-agent:
    systemctl --user disable --now cosmic-bwarden-agent

# Uninstall all components from both system and local paths
uninstall:
    echo "Uninstalling from system paths..."
    sudo rm -f {{bin_dir}}/cosmic-bwarden-agent
    sudo rm -f {{bin_dir}}/cosmic-applet-bwarden
    sudo rm -f {{bin_dir}}/cosmic-bwarden-cli
    sudo rm -f {{apps_dir}}/com.enikeev.cosmic_bwarden.desktop
    sudo rm -f {{metainfo_dir}}/com.enikeev.cosmic_bwarden.metainfo.xml
    sudo rm -f {{applets_dir}}/com.enikeev.cosmic_bwarden.ron
    sudo rm -f {{systemd_user_dir}}/cosmic-bwarden-agent.service
    # Legacy app-ID / binary names (transitional cleanup across renames)
    sudo rm -f {{apps_dir}}/com.system76.CosmicBWarden.desktop
    sudo rm -f {{applets_dir}}/com.system76.CosmicBWarden.ron
    sudo rm -f {{bin_dir}}/com.system76.CosmicBWarden
    sudo rm -f {{bin_dir}}/cosmic-bwarden-ui
    # cosmic-bwarden.enikeev.com had a hyphenated last segment for one commit;
    # invalid as a D-Bus object path (only [A-Za-z0-9_] allowed), so it broke
    # "Open Vault Window" (dbus_activation::subscription exits(1) on failure).
    sudo rm -f {{apps_dir}}/com.enikeev.cosmic-bwarden.desktop
    sudo rm -f {{applets_dir}}/com.enikeev.cosmic-bwarden.ron
    sudo rm -f {{icons_dir}}/scalable/apps/com.enikeev.cosmic_bwarden.svg
    sudo rm -f {{icons_dir}}/scalable/apps/com.enikeev.cosmic_bwarden-symbolic.svg
    sudo rm -f {{icons_dir}}/16x16/apps/com.enikeev.cosmic_bwarden.png
    sudo rm -f {{icons_dir}}/32x32/apps/com.enikeev.cosmic_bwarden.png
    sudo rm -f {{icons_dir}}/64x64/apps/com.enikeev.cosmic_bwarden.png
    sudo rm -f {{icons_dir}}/128x128/apps/com.enikeev.cosmic_bwarden.png
    echo "Uninstalling from local paths..."
    rm -f {{local_apps}}/com.enikeev.cosmic_bwarden.desktop
    rm -f {{local_metainfo}}/com.enikeev.cosmic_bwarden.metainfo.xml
    rm -f {{local_applets}}/com.enikeev.cosmic_bwarden.ron
    rm -f {{local_icons}}/scalable/apps/com.enikeev.cosmic_bwarden.svg
    rm -f {{local_icons}}/scalable/apps/com.enikeev.cosmic_bwarden-symbolic.svg
    rm -f {{local_icons}}/16x16/apps/com.enikeev.cosmic_bwarden.png
    rm -f {{local_icons}}/32x32/apps/com.enikeev.cosmic_bwarden.png
    rm -f {{local_icons}}/64x64/apps/com.enikeev.cosmic_bwarden.png
    rm -f {{local_icons}}/128x128/apps/com.enikeev.cosmic_bwarden.png
    rm -f {{local_share}}/../bin/cosmic-bwarden-agent
    rm -f {{local_share}}/../bin/cosmic-applet-bwarden
    rm -f {{local_share}}/../bin/cosmic-bwarden-cli
    # Legacy app-ID / binary names (transitional cleanup across renames)
    rm -f {{local_apps}}/com.system76.CosmicBWarden.desktop
    rm -f {{local_applets}}/com.system76.CosmicBWarden.ron
    rm -f {{local_share}}/../bin/com.system76.CosmicBWarden
    rm -f {{local_share}}/../bin/cosmic-bwarden-ui
    rm -f {{local_apps}}/com.enikeev.cosmic-bwarden.desktop
    rm -f {{local_applets}}/com.enikeev.cosmic-bwarden.ron
    echo "Removing Firefox native messaging host..."
    rm -f {{real_home}}/.mozilla/native-messaging-hosts/com.enikeev.cosmic_bwarden.json
    rm -f {{real_home}}/.mozilla/native-messaging-hosts/cosmic-bwarden-browser-host.sh

# Clean build artifacts
clean: uninstall
    cargo clean --quiet

# Run all tests in complexity order
test: test-unit test-agent test-cli test-ui

# 1. Unit Tests (Core & UI Logic)
test-unit:
    echo "--- 1. Unit Tests (Core & UI Logic) ---"
    cargo test --quiet -p cosmic-bwarden-core
    cargo test --quiet -p cosmic-bwarden-ui

# Rebuild the binaries the E2E harness launches from target/debug — running
# the suite against stale binaries fails the version-compatibility test with a
# confusing mismatch (docs/review/00_ground_truth.md F9).
build-test-binaries:
    cargo build --quiet -p cosmic-bwarden-agent -p cosmic-bwarden-cli

# 2. Agent & Protocol E2E Tests
# Needs a container socket: Docker, or podman via `systemctl --user start podman.socket`
# (the test harness auto-detects the podman user socket — no docker group required).
test-agent: build-test-binaries
    echo "--- 2. Agent & Protocol E2E Tests ---"
    cargo test --quiet -p cosmic-bwarden-tests --lib -- agent security vault pinned_ops ipc_hardening --test-threads=1

# 3. CLI E2E Tests
test-cli: build-test-binaries
    echo "--- 3. CLI E2E Tests ---"
    cargo test --quiet -p cosmic-bwarden-tests --lib -- cli_lifecycle cli_secret_mask_test custom_fields_cli --test-threads=1

# 4. UI E2E Tests
test-ui: build-test-binaries
    echo "--- 4. UI E2E Tests ---"
    cargo test --quiet -p cosmic-bwarden-tests --lib -- window_flow custom_fields_ui --test-threads=1

# Run the agent and UI for testing
run: build
    echo "Starting agent in background..."
    ./target/release/cosmic-bwarden-agent &
    echo "Starting UI..."
    ./target/release/cosmic-applet-bwarden

# [Dev] Register native messaging host pointing to debug build (use without a full install)
register-browser-host:
    cargo build -p cosmic-bwarden-agent --quiet
    python3 tests/browser-extension/register_host.py

# Pack the browser extension for distribution (production files only → target/)
# Implementation lives in packaging/pack-extension.sh so that this recipe, CI,
# and the release workflow all produce the identical artifact.
pack-extension:
    packaging/pack-extension.sh

# ---------------------------------------------------------------------------
# Extension signing (self-hosted / unlisted AMO channel).
#
# `just sign-extension` is the single user-facing target: it stages, lints,
# signs the CURRENT files on the unlisted AMO channel under a fresh timestamp
# version, and leaves a ready-to-install XPI in gitignored dist/. Signing
# requires AMO credentials (env-only or browser-extension/.env) and network
# access — it is never part of `default`.
#
# Dev signing versions are fresh timestamps (YYYY.M.D.mmm). For releases
# (CI later), the strict preflight in `node packaging/ext-release.mjs
# preflight` uses the `vYYYY.MM.P` git tag on HEAD (see
# docs/review/07_packaging.md) and requires manifest.json to match it exactly.
# With EXT_UPDATE_BASE_URL set, the staged update_url is baked into the XPI
# and dist/updates.json (the Firefox update manifest) is updated in the same
# run. Outputs print one absolute path per line for CI to capture.
# ---------------------------------------------------------------------------

# Install the pinned extension toolchain. web-ext is pinned exactly in
# browser-extension/package.json + package-lock.json; npm install is
# idempotent and stays offline once node_modules exists.
ext-deps:
    cd browser-extension && npm install --no-audit --no-fund

# Fail with a clear message when the installed web-ext differs from the pinned
# version — pre-v8/v10 flags differ, and silently passing the wrong flags to a
# different major is exactly the failure class we want to catch here.
ext-check-webext: ext-deps
    @set -euo pipefail; \
    expected="$(node -p "require('./browser-extension/package.json').devDependencies['web-ext']")"; \
    actual="$(browser-extension/node_modules/.bin/web-ext --version)"; \
    if [ "$actual" != "$expected" ]; then \
        echo "error: web-ext version mismatch — installed $actual, pinned $expected" >&2; \
        echo "Run 'just ext-deps' to sync browser-extension/node_modules." >&2; \
        exit 1; \
    fi

# Fail fast BEFORE any build work: credentials are loaded from
# browser-extension/.env when not already exported (explicit exports win) and
# must be present — otherwise abort immediately with the URL where to generate
# them. Then the version is resolved:
#   - dev mode (default): a fresh timestamp (YYYY.M.D.mmm) — no tag,
#     manifest-match, or clean-tree requirements; the current files are what
#     gets signed. Only duplicates already shipped in dist/ are refused.
#   - release mode (EXT_SIGN_MODE=release, used by CI on v* tags): the strict
#     tag-based preflight — HEAD must be a vYYYY.MM.P[-alphaN] tag and the
#     tree must be clean. The tag IS the version (it is injected into the
#     staged manifest at build time; alpha tags map into the numeric 4th
#     component, e.g. v2026.8.0-alpha -> 2026.8.0.1), so the committed
#     manifest.json version needs no release bumps. Nothing is uploaded to
#     AMO when this fails, so a failed run consumes no version.
sign-extension-preflight:
    @set -euo pipefail; \
    . packaging/load-ext-env.sh; \
    if [ -z "${WEB_EXT_API_KEY:-}" ] || [ -z "${WEB_EXT_API_SECRET:-}" ]; then \
        echo "error: WEB_EXT_API_KEY and WEB_EXT_API_SECRET are not set." >&2; \
        echo "Export them, or fill browser-extension/.env (template: browser-extension/.env.example)." >&2; \
        echo "Generate credentials at https://addons.mozilla.org/developers/addon/api/key/ (used only by 'just sign-extension')." >&2; \
        exit 1; \
    fi; \
    if [ "${EXT_SIGN_MODE:-dev}" = "release" ]; then mode=""; else mode="--dev"; fi; \
    if [ "${ALLOW_DIRTY:-0}" = "1" ]; then allow="--allow-dirty"; else allow=""; fi; \
    mkdir -p target; \
    node packaging/ext-release.mjs preflight $mode $allow > target/ext-sign-version.txt; \
    echo "signing version: $(cat target/ext-sign-version.txt)" >&2

# The whole signing flow in one target: stage the production files (update_url
# injected from EXT_UPDATE_BASE_URL when set) → lint → AMO unlisted sign →
# dist/cosmic-bwarden-<version>.xpi, plus dist/updates.json when the base URL
# is set. Dev mode (default) signs the current files under a timestamp
# version; EXT_SIGN_MODE=release signs a v* tag release (tag version injected
# into the staged manifest — no manifest.json bumps needed). Credentials (from the
# environment or browser-extension/.env) reach web-ext only through a
# generated config trampoline referencing process.env — never on a command
# line, in a file, or in trace output; the trampoline is removed on exit.
#
# URL split: EXT_UPDATE_BASE_URL is baked into the signed XPI as update_url
# (a stable always-current manifest location). EXT_UPDATE_LINK_BASE (optional,
# defaults to EXT_UPDATE_BASE_URL) builds update_link entries in updates.json
# and MUST be version-immutable — e.g. GitHub: update_url points at
# .../releases/latest/download, links at .../releases/download/<tag>.
#
# Polling semantics: web-ext polls the AMO version-detail API every 1 second
# (approvalCheckInterval=1000) and --timeout covers BOTH the validation poll
# and the wait for approval before the signed XPI is downloaded (sign.js:
# approvalCheckTimeout falls back to timeout). Unlisted submissions get a
# "tentatively approved" review that can take minutes, so the default window
# is 30 minutes (web-ext's own approval default is 15); override with
# EXT_SIGN_TIMEOUT_MS.
#
# Resume: web-ext saves the submission's upload uuid to
# target/ext-stage/.amo-upload-uuid; this recipe persists it at
# target/ext-sign-upload-uuid, so a re-run after an interruption or timeout
# resumes the SAME AMO submission instead of uploading a new version —
# provided the staged XPI is unchanged (web-ext compares a CRC and uploads
# fresh on any change). Delete target/ext-sign-upload-uuid to force a fresh
# upload.
sign-extension: sign-extension-preflight ext-check-webext
    @set -euo pipefail; \
    . packaging/load-ext-env.sh; \
    case "${EXT_SIGN_TIMEOUT_MS:-1800000}" in *[!0-9]*) echo "error: EXT_SIGN_TIMEOUT_MS must be a number of milliseconds" >&2; exit 1;; esac; \
    packaging/ext-stage.sh; \
    ver="$(cat target/ext-sign-version.txt)"; \
    if [ -f target/ext-sign-upload-uuid ]; then cp -f target/ext-sign-upload-uuid target/ext-stage/.amo-upload-uuid; fi; \
    node packaging/ext-release.mjs inject-version target/ext-stage/manifest.json "$ver"; \
    if [ -n "${EXT_UPDATE_BASE_URL:-}" ]; then \
        node packaging/ext-release.mjs inject-update-url target/ext-stage/manifest.json "$EXT_UPDATE_BASE_URL"; \
    else \
        echo "warning: EXT_UPDATE_BASE_URL unset — the XPI will not carry an update_url and no updates.json will be written (self-hosted updates won't work until a signed build has one)"; \
    fi; \
    browser-extension/node_modules/.bin/web-ext lint --source-dir target/ext-stage --self-hosted --no-input; \
    cfg="$(mktemp --suffix=.cjs)"; \
    trap 'rm -f "$cfg"' EXIT; \
    printf 'module.exports = { sign: { apiKey: process.env.WEB_EXT_API_KEY, apiSecret: process.env.WEB_EXT_API_SECRET } };\n' > "$cfg"; \
    mkdir -p target/ext-sign-artifacts; \
    rm -f target/ext-sign-artifacts/*.xpi; \
    browser-extension/node_modules/.bin/web-ext sign \
        --source-dir target/ext-stage \
        --channel unlisted \
        --config "$cfg" \
        --no-config-discovery \
        --no-input \
        --artifacts-dir target/ext-sign-artifacts \
        --timeout "${EXT_SIGN_TIMEOUT_MS:-1800000}" || sign_rc=$?; \
    cp -f target/ext-stage/.amo-upload-uuid target/ext-sign-upload-uuid 2>/dev/null || true; \
    if [ -n "${sign_rc:-}" ]; then exit "$sign_rc"; fi; \
    node packaging/ext-release.mjs finalize-sign target/ext-sign-artifacts "$ver" dist >/dev/null; \
    echo "$PWD/dist/cosmic-bwarden-$ver.xpi"; \
    if [ -n "${EXT_UPDATE_BASE_URL:-}" ]; then \
        node packaging/ext-release.mjs updates-json "dist/cosmic-bwarden-$ver.xpi" "$EXT_UPDATE_BASE_URL" "$ver" dist/updates.json "${EXT_UPDATE_LINK_BASE:-}" >/dev/null; \
        echo "$PWD/dist/updates.json"; \
    fi

# Unit tests for the release pipeline's pure logic (version preflight,
# updates.json generation, sha256) — offline, no AMO credentials.
test-ext-release:
    node --test "packaging/*.test.mjs"

# Setup extension testing environment (installs npm dependencies)
test-extension-setup:
    cd browser-extension && npm install

# Run extension unit/logic tests
test-extension-unit:
    cd browser-extension && npm run test:unit

# Run extension E2E tests (Playwright, mocked — no agent needed)
# Goes through the npm script rather than calling playwright directly, so the
# `e2e:link` step runs: the spec dir needs a node_modules symlink to resolve
# @playwright/test (see tests/browser-extension/playwright/link-deps.js).
test-extension-e2e: test-extension-setup
    cd browser-extension && npx playwright install firefox
    cd browser-extension && npm run test:e2e

# Run full extension E2E tests with real agent and vaultwarden
test-extension-e2e-full: build test-extension-setup
    @echo "--- Extension Full E2E (Real Agent & Vaultwarden) ---"
    bash tests/browser-extension/run-e2e.sh

# Run full extension E2E tests in Chrome (headless-compatible, real agent + Vaultwarden)
test-extension-e2e-chrome: build test-extension-setup
    @echo "--- Extension Chrome Full E2E ---"
    bash tests/browser-extension/run-chrome-e2e.sh

# Debug extension E2E tests (Playwright with UI)
test-extension-e2e-debug: test-extension-setup
    cd browser-extension && npx playwright install firefox
    bash tests/browser-extension/playwright/setup_native_host.sh
    cd browser-extension && npm run e2e:link
    cd browser-extension && npx playwright test --config=../tests/browser-extension/playwright/playwright.config.js --ui
