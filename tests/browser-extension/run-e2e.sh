#!/bin/bash
set -e

# Get absolute PROJECT_ROOT
PROJECT_ROOT=$(git rev-parse --show-toplevel)
cd "$PROJECT_ROOT"

# shellcheck source=cleanup.sh
source "$PROJECT_ROOT/tests/browser-extension/cleanup.sh"

TEST_PROFILE=test-extension-e2e
# setup_native_host.sh overwrites this with a wrapper hardcoding the test
# profile. Without a restore, the developer's real Firefox extension keeps
# talking to the test profile after the suite exits.
NATIVE_HOST_WRAPPER="${HOME}/.mozilla/native-messaging-hosts/cosmarden-browser-host.sh"

# Setup cleanup
cleanup() {
    echo "Cleaning up..."
    if [ -n "$AGENT_PID" ]; then
        kill $AGENT_PID 2>/dev/null || true
        # Wait before removing dirs: a killed agent can still be mid-write,
        # and its shutdown would otherwise recreate what we just deleted.
        wait $AGENT_PID 2>/dev/null || true
    fi
    if [ -n "$VW_PID" ]; then kill $VW_PID 2>/dev/null || true; fi
    restore_file "$NATIVE_HOST_WRAPPER" || true
    cleanup_profile "$TEST_PROFILE" || true
    rm -f -- /tmp/agent_test.log /tmp/vaultwarden_test.log \
             /tmp/cosmarden-browser-host.log
}
trap cleanup EXIT

# 1. Build
echo "Building agent and CLI..."
cargo build -p cosmarden-agent -p cosmarden-cli --quiet

# 2. Vaultwarden
echo "Starting Vaultwarden..."
./tools/run_vaultwarden.sh > /tmp/vaultwarden_test.log 2>&1 &
VW_PID=$!
timeout 30 bash -c 'until curl -s http://localhost:8080/health > /dev/null; do sleep 1; done' || (echo "Vaultwarden failed"; exit 1)

# 3. Agent
echo "Starting Agent..."
export COSMARDEN_PROFILE="$TEST_PROFILE"
./target/debug/cosmarden-agent > /tmp/agent_test.log 2>&1 &
AGENT_PID=$!
sleep 2

# 4. Account
echo "Initializing test account..."
EMAIL="test-extension@example.com"
PASSWORD="password123"
SERVER="http://localhost:8080"
./target/debug/cosmarden register --server "$SERVER" --password "$PASSWORD" "$EMAIL" || true
./target/debug/cosmarden login --server "$SERVER" --password "$PASSWORD" "$EMAIL"

# 5. Native Host
# Back up the developer's real wrapper first — setup_native_host.sh replaces it
# with a test-profile one, and cleanup() restores this on exit.
echo "Setting up native messaging host..."
mkdir -p "$(dirname "$NATIVE_HOST_WRAPPER")"
backup_file "$NATIVE_HOST_WRAPPER"
bash tests/browser-extension/playwright/setup_native_host.sh

# 6. Isolation & Tests
# The spec directory needs its node_modules symlink before playwright loads the
# config from there (see playwright/link-deps.js) — it is gitignored, so a fresh
# clone starts without it.
node "$PROJECT_ROOT/tests/browser-extension/playwright/link-deps.js"

KIOSK_WRAPPER="$PROJECT_ROOT/tests/browser-extension/playwright/kiosk-wrapper.sh"

# kiosk-wrapper.sh is tracked and resolves PROJECT_ROOT from its own location
# — no regeneration (a heredoc here previously overwrote it with a hardcoded
# path, dirtying the working tree on every run).
echo "Launching isolated compositor with kiosk: $KIOSK_WRAPPER"
# Use sh -c and pass absolute path to kiosk
# cosmic-comp is a smithay compositor: the wlroots-style WLR_* env vars are
# ignored. COSMIC_BACKEND selects the backend explicitly (see
# cosmic-comp src/backend/mod.rs): winit opens a nested compositor window on
# the current session's display — works on a real desktop (Wayland or X11)
# and under Xvfb with software GL (LIBGL_ALWAYS_SOFTWARE=1).
export COSMIC_BACKEND=winit
cosmic-comp -- sh -c "$KIOSK_WRAPPER"
