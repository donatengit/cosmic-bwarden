# Test Implementation Plan

This plan outlines the steps to address identified testing gaps in the `cosmic-bwarden` project, following the complexity-ordered testing hierarchy.

## 1. Missing SSH Key CRUD E2E Test
**Goal:** Implement the currently missing `test_ssh_key_crud_lifecycle` in `crates/cosmic-bwarden-tests/src/vault_ops.rs`.
- **Steps:**
    - Create a test case that registers a user.
    - Add an entry of type `SshKey` with private key, public key, and fingerprint.
    - Sync and verify the entry was created.
    - Update the private key and verify the change.
    - Delete the entry and verify deletion.

## 2. E2E Coverage for Card and Identity Types
**Goal:** Expand `vault_ops.rs` to include CRUD tests for `Card` and `Identity` entry types.
- **Steps:**
    - Implement `test_card_crud_lifecycle`.
    - Implement `test_identity_crud_lifecycle`.
    - Ensure all fields (including secrets like credit card numbers) are correctly handled.

## 3. Agent Unit Tests
**Goal:** Add unit tests to `crates/cosmic-bwarden-agent/src` to verify internal state logic without requiring a full Vaultwarden instance.
- **Proposed Scope:**
    - `state.rs`: Test state transitions (Locked -> Unlocked, etc.).
    - `keyring.rs`: Mock keyring interactions to test secret storage logic.
    - `ssh_agent.rs`: Test SSH key formatting and agent protocol response generation.

## 4. CLI Unit Tests
**Goal:** Add unit tests to `crates/cosmic-bwarden-cli/src` to verify argument parsing and output formatting.
- **Proposed Scope:**
    - Verify that sensitive fields are masked in CLI output when requested.
    - Test that the CLI correctly translates command-line flags into `Action` variants.

## 5. Verification
- **Execution:** Run `just test` to ensure all new tests pass and do not break the existing hierarchy.
- **Documentation:** Update `docs/testing.md` once gaps are closed.

## Timeline
- **Phase 1:** SSH Key & Vault Ops Expansion (E2E)
- **Phase 2:** Agent & CLI Unit Tests
