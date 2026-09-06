#!/bin/bash
set -e

# Get the absolute project root (4 levels up from this script)
SCRIPT_PATH=$(realpath "$0")
PROJECT_ROOT=$(dirname $(dirname $(dirname $(dirname "$SCRIPT_PATH"))))
cd "$PROJECT_ROOT"

echo "DEBUG: setup_native_host.sh PROJECT_ROOT=$PROJECT_ROOT"

echo "Building agent..."
cargo build -p cosmic-bwarden-agent --quiet

# Register native messaging host
python3 tests/browser-extension/register_host.py

# Override wrapper to use test profile
# Prefer $HOME when set (sandboxed/CI runs may point it at a writable tree);
# fall back to the passwd entry so normal invocations behave exactly as before.
HOME_DIR="${HOME:-$(eval echo ~$USER)}"
MANIFEST_DIR="$HOME_DIR/.mozilla/native-messaging-hosts"
WRAPPER_PATH="$MANIFEST_DIR/cosmic-bwarden-browser-host.sh"
AGENT_PATH="$PROJECT_ROOT/target/debug/cosmic-bwarden-agent"

if [ ! -f "$AGENT_PATH" ]; then
    echo "Error: Agent binary not found at $AGENT_PATH"
    exit 1
fi

# WARNING: this OVERWRITES the developer's real Firefox native-messaging
# wrapper with one hardcoding the test profile. The caller is responsible for
# backing it up and restoring it — run-e2e.sh does that via backup_file() /
# restore_file() in tests/browser-extension/cleanup.sh. Running this script
# standalone leaves the developer's Firefox talking to the test profile; undo
# with `just register-browser-host`.
echo "Customizing wrapper at $WRAPPER_PATH..."
cat <<EOF > "$WRAPPER_PATH"
#!/bin/bash
export COSMIC_BWARDEN_PROFILE=test-extension-e2e
export RUST_LOG=debug
exec "$AGENT_PATH" browser-host "\$@" >> /tmp/cosmic-bwarden-browser-host.log 2>&1
EOF
chmod +x "$WRAPPER_PATH"

echo "Native messaging host registered."
