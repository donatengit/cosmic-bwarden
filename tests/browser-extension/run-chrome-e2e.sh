#!/bin/bash
set -e

PROJECT_ROOT=$(git rev-parse --show-toplevel)
cd "$PROJECT_ROOT"

# shellcheck source=cleanup.sh
source "$PROJECT_ROOT/tests/browser-extension/cleanup.sh"

TEST_PROFILE=test-chrome-e2e

cleanup() {
    echo "Cleaning up..."
    if [ -n "$AGENT_PID" ]; then
        kill "$AGENT_PID" 2>/dev/null || true
        # Wait before removing dirs: a killed agent can still be mid-write,
        # and its shutdown would otherwise recreate what we just deleted.
        wait "$AGENT_PID" 2>/dev/null || true
    fi
    DOCKER_HOST="${DOCKER_HOST:-}" docker stop corbw-vaultwarden-chrome 2>/dev/null || true
    DOCKER_HOST="${DOCKER_HOST:-}" docker rm   corbw-vaultwarden-chrome 2>/dev/null || true
    cleanup_profile "$TEST_PROFILE" || true
    rm -f -- /tmp/agent_chrome_test.log /tmp/vaultwarden_chrome_test.log \
             /tmp/native-host-debug.log
}
trap cleanup EXIT

echo "Building agent and CLI..."
cargo build -p cosmic-bwarden-agent -p cosmic-bwarden-cli --quiet

echo "Starting Vaultwarden..."
# Use Podman socket if Docker is not available
if [ -z "$DOCKER_HOST" ] && [ -S "/run/user/$(id -u)/podman/podman.sock" ]; then
    export DOCKER_HOST="unix:///run/user/$(id -u)/podman/podman.sock"
fi
DOCKER_HOST="$DOCKER_HOST" docker stop corbw-vaultwarden-chrome 2>/dev/null || true
DOCKER_HOST="$DOCKER_HOST" docker rm   corbw-vaultwarden-chrome 2>/dev/null || true
# Keep the container off the developer's cores for the length of the run. These
# match the caps the Rust harness applies (container_limits.rs); a container is
# not covered by the systemd scope `just` puts around the test command, because
# podman places it in a sibling cgroup. See docs/testing.md.
CONTAINER_CPUS=2
CONTAINER_MEM_MB=1024
LIMIT_ARGS=(--cpus "$CONTAINER_CPUS" --memory "${CONTAINER_MEM_MB}m")

DOCKER_HOST="$DOCKER_HOST" docker run -d --name corbw-vaultwarden-chrome \
    --pids-limit=-1 \
    "${LIMIT_ARGS[@]}" \
    -e SIGNUPS_ALLOWED=true \
    -e I_REALLY_WANT_VOLATILE_STORAGE=true \
    -p 8081:80 \
    vaultwarden/server:latest > /tmp/vaultwarden_chrome_test.log 2>&1
VW_PID=""  # managed by docker, not a shell process
export VW_URL=http://localhost:8081
timeout 30 bash -c 'until curl -s http://localhost:8081/api/alive > /dev/null 2>&1; do sleep 1; done' \
    || { echo "Vaultwarden failed to start"; exit 1; }

echo "Starting agent (profile: $TEST_PROFILE)..."
export COSMIC_BWARDEN_PROFILE="$TEST_PROFILE"
./target/debug/cosmic-bwarden-agent > /tmp/agent_chrome_test.log 2>&1 &
AGENT_PID=$!
sleep 2

export VW_EMAIL=test-chrome@example.com
export VW_PASSWORD=password123

echo "Installing npm dependencies..."
cd browser-extension
npm install --quiet
npx playwright install chromium --quiet 2>/dev/null || true
# Resolve @playwright/test from the spec directory (see playwright/link-deps.js).
npm run e2e:link

echo "Running Chrome extension E2E tests..."
npx playwright test \
    --config=../tests/browser-extension/playwright/playwright.config.js \
    --project=chrome-full \
    "$@"
