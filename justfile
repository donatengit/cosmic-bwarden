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
    echo '( name: "CosmicBWarden", description: "Secure Bitwarden client for COSMIC", identifier: "com.enikeev.cosmic_bwarden", icon: "com.enikeev.cosmic_bwarden-symbolic", )' > {{applets_dir}}/com.enikeev.cosmic_bwarden.ron

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
    sudo sh -c "echo '( name: \"CosmicBWarden\", description: \"Secure Bitwarden client for COSMIC\", identifier: \"com.enikeev.cosmic_bwarden\", icon: \"com.enikeev.cosmic_bwarden-symbolic\" )' > {{applets_dir}}/com.enikeev.cosmic_bwarden.ron"

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
    echo '( name: "CosmicBWarden", description: "Secure Bitwarden client for COSMIC", identifier: "com.enikeev.cosmic_bwarden", icon: "com.enikeev.cosmic_bwarden-symbolic", )' > {{local_applets}}/com.enikeev.cosmic_bwarden.ron
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
