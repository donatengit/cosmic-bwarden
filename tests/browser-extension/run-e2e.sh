#!/bin/bash
set -e

# Get absolute PROJECT_ROOT
PROJECT_ROOT=$(git rev-parse --show-toplevel)
cd "$PROJECT_ROOT"

# Setup cleanup
cleanup() {
    echo "Cleaning up..."
    if [ -n "$AGENT_PID" ]; then kill $AGENT_PID 2>/dev/null || true; fi
    if [ -n "$VW_PID" ]; then kill $VW_PID 2>/dev/null || true; fi
}
trap cleanup EXIT

# 1. Build
echo "Building agent and CLI..."
cargo build -p cosmic-bwarden-agent -p cosmic-bwarden-cli --quiet

# 2. Vaultwarden
echo "Starting Vaultwarden..."
./tools/run_vaultwarden.sh > /tmp/vaultwarden_test.log 2>&1 &
VW_PID=$!
timeout 30 bash -c 'until curl -s http://localhost:8080/health > /dev/null; do sleep 1; done' || (echo "Vaultwarden failed"; exit 1)

# 3. Agent
echo "Starting Agent..."
export COSMIC_BWARDEN_PROFILE=test-extension-e2e
./target/debug/cosmic-bwarden-agent > /tmp/agent_test.log 2>&1 &
AGENT_PID=$!
sleep 2

# 4. Account
echo "Initializing test account..."
EMAIL="test-extension@example.com"
PASSWORD="password123"
SERVER="http://localhost:8080"
./target/debug/cosmic-bwarden-cli register --server "$SERVER" --password "$PASSWORD" "$EMAIL" || true
./target/debug/cosmic-bwarden-cli login --server "$SERVER" --password "$PASSWORD" "$EMAIL"

# 5. Native Host
echo "Setting up native messaging host..."
bash tests/browser-extension/playwright/setup_native_host.sh

# 6. Isolation & Tests
# The spec directory needs its node_modules symlink before playwright loads the
# config from there (see playwright/link-deps.js) — it is gitignored, so a fresh
# clone starts without it.
node "$PROJECT_ROOT/tests/browser-extension/playwright/link-deps.js"

KIOSK_WRAPPER="$PROJECT_ROOT/tests/browser-extension/playwright/kiosk-wrapper.sh"
PW_BIN="$PROJECT_ROOT/browser-extension/node_modules/.bin/playwright"

cat <<EOF > "$KIOSK_WRAPPER"
#!/bin/bash
PROJECT_ROOT="$PROJECT_ROOT"
cd "\$PROJECT_ROOT/browser-extension"
echo "Kiosk started. Running Playwright..."
"\$PROJECT_ROOT/browser-extension/node_modules/.bin/playwright" test --config=../tests/browser-extension/playwright/playwright.config.js --project=firefox-full "\$@"
EOF
chmod +x "$KIOSK_WRAPPER"

echo "Launching isolated compositor with kiosk: $KIOSK_WRAPPER"
export WLR_BACKENDS=headless
export SMITHAY_BACKEND=headless
export WLR_LIBINPUT_NO_DEVICES=1
export WLR_RENDERER=pixman

# Use sh -c and pass absolute path to kiosk
cosmic-comp -- sh -c "$KIOSK_WRAPPER"
