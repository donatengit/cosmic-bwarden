#!/bin/bash
set -e

PROJECT_ROOT=$(git rev-parse --show-toplevel)
cd "$PROJECT_ROOT"

cleanup() {
    echo "Cleaning up..."
    [ -n "$AGENT_PID" ] && kill "$AGENT_PID" 2>/dev/null || true
    DOCKER_HOST="${DOCKER_HOST:-}" docker stop corbw-vaultwarden-chrome 2>/dev/null || true
    DOCKER_HOST="${DOCKER_HOST:-}" docker rm   corbw-vaultwarden-chrome 2>/dev/null || true
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
DOCKER_HOST="$DOCKER_HOST" docker run -d --name corbw-vaultwarden-chrome \
    --pids-limit=-1 \
    -e SIGNUPS_ALLOWED=true \
    -e I_REALLY_WANT_VOLATILE_STORAGE=true \
    -p 8081:80 \
    vaultwarden/server:latest > /tmp/vaultwarden_chrome_test.log 2>&1
VW_PID=""  # managed by docker, not a shell process
export VW_URL=http://localhost:8081
timeout 30 bash -c 'until curl -s http://localhost:8081/api/alive > /dev/null 2>&1; do sleep 1; done' \
    || { echo "Vaultwarden failed to start"; exit 1; }

echo "Starting agent (profile: test-chrome-e2e)..."
export COSMIC_BWARDEN_PROFILE=test-chrome-e2e
./target/debug/cosmic-bwarden-agent > /tmp/agent_chrome_test.log 2>&1 &
AGENT_PID=$!
sleep 2

export VW_EMAIL=test-chrome@example.com
export VW_PASSWORD=password123

echo "Installing npm dependencies..."
cd browser-extension
npm install --quiet
npx playwright install chromium --quiet 2>/dev/null || true

echo "Running Chrome extension E2E tests..."
npx playwright test \
    --config=../tests/browser-extension/playwright/playwright.config.js \
    --project=chrome-full \
    "$@"
