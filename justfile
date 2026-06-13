prefix := "/usr/local"
bin_dir := prefix + "/bin"
share_dir := prefix + "/share"
applets_dir := share_dir + "/cosmic/applets"
apps_dir := share_dir + "/applications"
systemd_user_dir := "/usr/lib/systemd/user"

# Resolve the invoking user's home dir even when run via `sudo just ...`,
# so user-local install/uninstall paths don't end up under /root.
real_home := `if [ -n "${SUDO_USER:-}" ]; then getent passwd "$SUDO_USER" | cut -d: -f6; else echo "$HOME"; fi`
local_share := real_home + "/.local/share"
local_applets := local_share + "/cosmic/applets"
local_apps := local_share + "/applications"

# Default task: build the project
default: build

# Build all components in release mode
build:
    if [ -n "${SUDO_USER:-}" ]; then \
        echo "Detected sudo, running cargo build as $SUDO_USER..."; \
        sudo -u "$SUDO_USER" env PATH="$PATH" RUSTFLAGS="-C target-cpu=native" cargo build --release --quiet; \
    else \
        RUSTFLAGS="-C target-cpu=native" cargo build --release --quiet; \
    fi

# Install binaries, desktop entry, and COSMIC applet metadata system-wide
install: build
    echo "Installing binaries..."
    install -Dm755 target/release/cosmic-bwarden-agent {{bin_dir}}/cosmic-bwarden-agent
    install -Dm755 target/release/cosmic-bwarden-ui {{bin_dir}}/com.system76.CosmicBWarden
    install -Dm755 target/release/cosmic-bwarden-cli {{bin_dir}}/cosmic-bwarden-cli
    
    echo "Installing desktop entry..."
    install -Dm644 crates/cosmic-bwarden-ui/resources/com.system76.CosmicBWarden.desktop {{apps_dir}}/com.system76.CosmicBWarden.desktop
    
    echo "Installing COSMIC applet metadata..."
    mkdir -p {{applets_dir}}
    echo '( name: "CosmicBWarden", description: "Secure Bitwarden client for COSMIC", identifier: "com.system76.CosmicBWarden", icon: "password-manager-symbolic", )' > {{applets_dir}}/com.system76.CosmicBWarden.ron

    echo "Installing systemd user service..."
    mkdir -p {{systemd_user_dir}}
    sed "s|@BINDIR@|{{bin_dir}}|g" crates/cosmic-bwarden-agent/res/cosmic-bwarden-agent.service > /tmp/cosmic-bwarden-agent.service
    install -Dm644 /tmp/cosmic-bwarden-agent.service {{systemd_user_dir}}/cosmic-bwarden-agent.service
    rm /tmp/cosmic-bwarden-agent.service

# Perform a completely fresh installation (removes old files first)
clean-install: uninstall build
    echo "Installing binaries..."
    sudo install -Dm755 target/release/cosmic-bwarden-agent {{bin_dir}}/cosmic-bwarden-agent
    sudo install -Dm755 target/release/cosmic-bwarden-ui {{bin_dir}}/cosmic-bwarden-ui
    sudo ln -sf {{bin_dir}}/cosmic-bwarden-ui {{bin_dir}}/com.system76.CosmicBWarden
    sudo install -Dm755 target/release/cosmic-bwarden-cli {{bin_dir}}/cosmic-bwarden-cli

    echo "Installing desktop entry..."
    sudo install -Dm644 crates/cosmic-bwarden-ui/resources/com.system76.CosmicBWarden.desktop {{apps_dir}}/com.system76.CosmicBWarden.desktop

    echo "Installing COSMIC applet metadata..."
    sudo mkdir -p {{applets_dir}}
    sudo sh -c "echo '( name: \"CosmicBWarden\", description: \"Secure Bitwarden client for COSMIC\", identifier: \"com.system76.CosmicBWarden\", icon: \"password-manager-symbolic\" )' > {{applets_dir}}/com.system76.CosmicBWarden.ron"

    echo "Installing systemd user service..."
    sudo mkdir -p {{systemd_user_dir}}
    sed "s|@BINDIR@|{{bin_dir}}|g" crates/cosmic-bwarden-agent/res/cosmic-bwarden-agent.service > /tmp/cosmic-bwarden-agent.service
    sudo install -Dm644 /tmp/cosmic-bwarden-agent.service {{systemd_user_dir}}/cosmic-bwarden-agent.service
    rm /tmp/cosmic-bwarden-agent.service
    echo "Done. Please run 'just restart-panel' and 'just enable-agent'."

# Install metadata and desktop files for the current user (only if not installing system-wide)
user-install: build
    echo "Warning: user-install may conflict with system-wide install. Running uninstall first..."
    just uninstall
    echo "Installing desktop entry to local apps..."
    mkdir -p {{local_apps}}
    install -Dm644 crates/cosmic-bwarden-ui/resources/com.system76.CosmicBWarden.desktop {{local_apps}}/com.system76.CosmicBWarden.desktop

    echo "Installing binaries to user bin..."
    mkdir -p {{local_share}}/../bin
    install -Dm755 target/release/cosmic-bwarden-agent {{local_share}}/../bin/cosmic-bwarden-agent
    install -Dm755 target/release/cosmic-bwarden-ui {{local_share}}/../bin/cosmic-bwarden-ui
    ln -sf {{local_share}}/../bin/cosmic-bwarden-ui {{local_share}}/../bin/com.system76.CosmicBWarden
    install -Dm755 target/release/cosmic-bwarden-cli {{local_share}}/../bin/cosmic-bwarden-cli

    echo "Installing COSMIC applet metadata to local applets..."
    mkdir -p {{local_applets}}
    echo '( name: "CosmicBWarden", description: "Secure Bitwarden client for COSMIC", identifier: "com.system76.CosmicBWarden", icon: "password-manager-symbolic", )' > {{local_applets}}/com.system76.CosmicBWarden.ron
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
    sudo rm -f {{bin_dir}}/cosmic-bwarden-ui
    sudo rm -f {{bin_dir}}/com.system76.CosmicBWarden
    sudo rm -f {{bin_dir}}/cosmic-bwarden-cli
    sudo rm -f {{apps_dir}}/com.system76.CosmicBWarden.desktop
    sudo rm -f {{applets_dir}}/com.system76.CosmicBWarden.ron
    sudo rm -f {{systemd_user_dir}}/cosmic-bwarden-agent.service
    echo "Uninstalling from local paths..."
    rm -f {{local_apps}}/com.system76.CosmicBWarden.desktop
    rm -f {{local_applets}}/com.system76.CosmicBWarden.ron
    rm -f {{local_share}}/../bin/com.system76.CosmicBWarden

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

# 2. Agent & Protocol E2E Tests
test-agent:
    echo "--- 2. Agent & Protocol E2E Tests ---"
    sg docker -c "cargo test --quiet -p cosmic-bwarden-tests --lib -- agent security vault pinned_ops --test-threads=1"

# 3. CLI E2E Tests
test-cli:
    echo "--- 3. CLI E2E Tests ---"
    sg docker -c "cargo test --quiet -p cosmic-bwarden-tests --lib -- cli_lifecycle cli_secret_mask_test custom_fields_cli --test-threads=1"

# 4. UI E2E Tests
test-ui:
    echo "--- 4. UI E2E Tests ---"
    sg docker -c "cargo test --quiet -p cosmic-bwarden-tests --lib -- window_flow custom_fields_ui --test-threads=1"

# Run the agent and UI for testing
run: build
    echo "Starting agent in background..."
    ./target/release/cosmic-bwarden-agent &
    echo "Starting UI..."
    ./target/release/cosmic-bwarden-ui
