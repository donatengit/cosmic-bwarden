#!/bin/bash
# minimal_verify.sh - Verify Native Messaging with geckodriver

set -e

PROJECT_ROOT=$(git rev-parse --show-toplevel)
TEST_TMP=$(mktemp -d)
AGENT_BIN="$PROJECT_ROOT/target/debug/cosmic-bwarden-agent"
EXT_DIR="$PROJECT_ROOT/browser-extension"

# EXPORT HOME globally so everything (Agent Server + Firefox children) sees the same isolated environment
export HOME="$TEST_TMP"
export COSMIC_BWARDEN_PROFILE=test-verify
export RUST_LOG=debug

echo "Using isolated HOME: $HOME"

# Clear old logs
rm -f /tmp/cosmic-bwarden-browser-host.log

# 1. Setup Native Messaging Host Manifest
HOST_NAME="com.8bit.cosmic_bwarden"
MANIFEST_DIR="$HOME/.mozilla/native-messaging-hosts"
mkdir -p "$MANIFEST_DIR"

cat <<EOF > "$MANIFEST_DIR/$HOST_NAME.json"
{
  "name": "$HOST_NAME",
  "description": "COSMIC BWarden Test Host",
  "path": "$AGENT_BIN",
  "type": "stdio",
  "allowed_extensions": ["cosmic-bwarden@enikeev.com"]
}
EOF

# 2. Start Agent Server
echo "Starting Agent Server..."
"$AGENT_BIN" > "$TEST_TMP/agent_server.log" 2>&1 &
AGENT_PID=$!

# 3. Start geckodriver
echo "Starting geckodriver..."
geckodriver --port 4444 > "$TEST_TMP/geckodriver.log" 2>&1 &
GECKO_PID=$!

cleanup() {
    echo "Cleaning up..."
    kill $AGENT_PID $GECKO_PID 2>/dev/null || true
    # rm -rf "$TEST_TMP"
}
trap cleanup EXIT

sleep 2

# 4. Create Session
echo "Creating Firefox session..."
SESSION_JSON=$(curl -s -X POST -H "Content-Type: application/json" http://localhost:4444/session -d '{
    "capabilities": {
        "alwaysMatch": {
            "browserName": "firefox",
            "moz:firefoxOptions": {
                "args": ["-headless"]
            }
        }
    }
}')

SESSION_ID=$(echo "$SESSION_JSON" | python3 -c "import sys, json; print(json.load(sys.stdin)['value']['sessionId'])")
echo "Session ID: $SESSION_ID"

# 5. Package extension
echo "Zipping extension..."
python3 -c "
import zipfile, os
with zipfile.ZipFile('$TEST_TMP/extension.xpi', 'w', zipfile.ZIP_DEFLATED) as zipf:
    for root, dirs, files in os.walk('$EXT_DIR'):
        if 'node_modules' in root or '.git' in root or 'tests' in root: continue
        for file in files:
            abs_path = os.path.join(root, file)
            rel_path = os.path.relpath(abs_path, '$EXT_DIR')
            zipf.write(abs_path, rel_path)
"

# 6. Install extension
echo "Installing extension..."
INSTALL_RESPONSE=$(curl -s -X POST -H "Content-Type: application/json" "http://localhost:4444/session/$SESSION_ID/moz/addon/install" -d '{
    "path": "'"$TEST_TMP/extension.xpi"'",
    "temporary": true
}')
echo "Install Response: $INSTALL_RESPONSE"

echo "Waiting for background script to trigger native messaging..."
sleep 5

# 7. Check logs
echo "--- Native Host Log Snapshot ---"
if [ -f /tmp/cosmic-bwarden-browser-host.log ]; then
    cat /tmp/cosmic-bwarden-browser-host.log
else
    echo "Log file /tmp/cosmic-bwarden-browser-host.log not found!"
fi

echo "--- Agent Server Log Snapshot ---"
grep "Parsed request: Version" "$TEST_TMP/agent_server.log" || echo "Version request not found in agent log."

echo "Verification complete."
