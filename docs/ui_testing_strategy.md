# UI Testing Strategy

This document outlines the testing approach for the `cosmic-bwarden` COSMIC-native user interface.

## Core Philosophy
We leverage the Model-View-Update (MVU) architecture provided by `iced` and `libcosmic` to test the application at two levels:
1. **State Logic (unit tests):** Testing the `update()` function in isolation by injecting messages and asserting on the resulting state.
2. **UI Interaction (headless integration tests):** Using `iced_test` to simulate user events (clicks, typing) and verifying the produced messages and resulting UI tree.

## 1. Unit Testing the `update()` Function
Since `update(&mut self, message: Message)` is a pure state transition function (returning a `Task`), it is highly testable.

### Methodology
- Initialize a `CorBwApp` instance in a known state.
- Call `update()` with a specific `Message`.
- Assert that the internal fields (e.g., `view`, `search_query`, `selected_entry_id`) have updated correctly.
- Verify that sensitive data is cleared upon `LockResult` or `LogoutResult`.

## 2. Headless UI Testing
*Note: Direct use of `iced_test` from crates.io is currently unavailable because `libcosmic` uses a specific git-based version of `iced`, leading to type incompatibilities.*

We prioritize **Pure Update Unit Tests** which provide the highest return on investment by verifying all state transitions and logic.

## 3. Handling Async Operations
As we use **Manual Message Injection**, we do not mock the `AgentClient` networking layer for UI tests. Instead:
- We trigger an action that would normally start a `Task` (e.g., `LoginSubmitted`).
- We manually call `update()` with the expected result message (e.g., `AuthResult(Ok(()))`).
- This decouples UI logic testing from the background agent's availability.

## 4. Test Environment
- Tests should be run via `cargo test -p cosmic-bwarden-ui`.
- CI environments do not need `WAYLAND_DISPLAY` as these tests are headless.
