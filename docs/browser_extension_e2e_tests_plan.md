# Plan: Real E2E Browser Extension Testing

This plan outlines the implementation of comprehensive end-to-end tests for the Cosmic BWarden Firefox extension, utilizing the real Rust agent and a Vaultwarden instance.

## 1. Infrastructure Orchestration

To avoid complex mocking and ensure behavioral correctness, we will use a "Real-Bridge" approach.

### Components:
- **Vaultwarden**: Running via Podman/Docker (using existing `run_vaultwarden.sh` or `TestEnv` logic).
- **Rust Agent**: Compiled in debug mode, running with a dedicated test profile.
- **Native Messaging Host**: The agent's `browser-host` subcommand, registered in a temporary location for the test run.
- **Firefox**: Launched by Playwright with extension support and remote debugging enabled.

### Orchestration (`justfile`):
- `just test-extension-e2e-full`:
    1.  Ensure agent is built.
    2.  Start Vaultwarden.
    3.  Generate a temporary Native Messaging manifest pointing to the debug agent.
    4.  Run Playwright tests with environment variables pointing to the temporary manifest and test profile.
    5.  Cleanup on exit.

## 2. Test Environment Configuration

### Playwright (`playwright.config.js`):
- Use `firefox` as the primary browser.
- Configure `launchOptions` to:
    - Load the extension temporarily.
    - Enable remote debugging port (for extension installation).
    - Set preferences to allow unsigned extensions and bypass security prompts in the test environment.
- Set `headless: false` (required for extensions and Native Messaging in Firefox), utilizing `xvfb-run` for CI.

### Native Messaging Setup:
- A helper script will create the JSON manifest in `~/.mozilla/native-messaging-hosts/com.8bit.cosmic_bwarden.json` (or a custom profile path) before tests start.

## 3. Test Scenarios

### A. Authentication Lifecycle
- **Scenario**: Verify UI transition from "Not Logged In" to "Unlocked".
- **Steps**:
    1. Start agent/extension in clean state.
    2. Open popup -> Verify "Not logged in" message.
    3. Use CLI to login/unlock.
    4. Re-open popup -> Verify entries are visible.

### B. Vault Operations (Search & Actions)
- **Scenario**: Verify searching and interacting with real data.
- **Steps**:
    1. Create a test entry via CLI: `cosmic-bwarden-cli add --name "Test Service" --username "testuser" --password "testpass"`.
    2. Open popup -> Search for "Service".
    3. Verify "Test Service" appears.
    4. Click "Copy Password" -> Verify clipboard contains "testpass".
    5. Click "TOTP" -> Verify clipboard contains a 6-digit code (if TOTP secret was added).

### C. Real Autofill
- **Scenario**: Verify content script interaction with a live page.
- **Steps**:
    1. Navigate to a local HTML test page with a login form.
    2. Click "Fill" on the "Test Service" entry in the popup.
    3. Verify form fields on the page are populated correctly.

### D. Sync & Lock
- **Scenario**: Verify state propagation.
- **Steps**:
    1. Update an entry via CLI.
    2. Click "Sync" in popup -> Verify updated data appears.
    3. Click "Lock" in popup -> Verify UI immediately shows "Vault is locked".

## 4. Implementation Steps

1.  **Refactor `justfile`**: Add `test-extension-e2e-full` and supporting setup/cleanup tasks.
2.  **Create `browser-extension/tests/setup_native_host.sh`**: Script to register the agent manifest.
3.  **Update `browser-extension/playwright.config.js`**: Configure for real Firefox extension testing.
4.  **Create `browser-extension/tests/full_cycle.spec.js`**: Implement the scenarios above.
5.  **Verify**: Run the suite and ensure all components interact correctly.
