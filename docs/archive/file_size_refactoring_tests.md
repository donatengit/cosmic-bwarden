# Test Suite Refactoring Plan

This document outlines the strategy for decomposing large test files in the `corbw` project to adhere to the maintainability target of **150-250 lines per file**.

## Target 1: `cosmic-bwarden-tests/src/vault_ops.rs` (~407 lines)

Current state: Contains CRUD lifecycle tests for all Bitwarden entry types.
Target state: Decomposed into a `vault/` module.

### Structure
- **`crates/cosmic-bwarden-tests/src/vault/mod.rs`**: Module declarations.
- **`crates/cosmic-bwarden-tests/src/vault/crud.rs`**: Lifecycle tests for `Login`, `SecureNote`, `Card`, and `Identity`.
- **`crates/cosmic-bwarden-tests/src/vault/ssh.rs`**: Dedicated lifecycle tests for `SshKey` (Bitwarden Type 5).

### Tasks
1. Create `vault/` directory and `mod.rs`.
2. Move generic CRUD tests to `vault/crud.rs`.
3. Move SSH-specific tests to `vault/ssh.rs`.
4. Update `lib.rs` to replace `mod vault_ops` with `mod vault`.

---

## Target 2: `cosmic-bwarden-ui/src/tests.rs` (~335 lines)

Current state: Single file covering all UI unit tests from state transitions to component logic.
Target state: Decomposed into a `app/tests/` module.

### Structure
- **`crates/cosmic-bwarden-ui/src/app/tests/mod.rs`**: Module declarations.
- **`crates/cosmic-bwarden-ui/src/app/tests/lifecycle.rs`**: Window differentiation, popup lifecycle, and surface isolation tests.
- **`crates/cosmic-bwarden-ui/src/app/tests/flows.rs`**: Multi-step user flows (e.g., Login -> Add Note), settings editing, and state clearing logic.
- **`crates/cosmic-bwarden-ui/src/app/tests/interactions.rs`**: Search/filtering, field editing, and reveal toggle logic.
- **`crates/cosmic-bwarden-ui/src/app/tests/events.rs`**: Reactive protocol event handling (Locked/Unlocked) and error state transitions.

### Tasks
1. Create `app/tests/` directory and `mod.rs`.
2. Categorize and move tests from `ui/src/tests.rs` into the new modules.
3. Update `ui/src/app/state.rs` or `ui/src/lib.rs` to include the new test module.

---

## Coordination Updates
- Update `justfile` test targets to reflect the new module structure.
- Update `docs/testing.md` if command paths for specific test modules change.
