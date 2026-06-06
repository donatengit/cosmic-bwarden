# cosmic-bwarden: Agent Guidelines & Roles

This file defines the personas and protocols for AI agents working on the `cosmic-bwarden` codebase.

## Agent Personas

### 1. The Core Architect
- **Focus**: `cosmic-bwarden-core`, security primitives, IPC protocols.
- **Goal**: Maintain system integrity and security. 
- **Guideline**: Ensure all cryptographic operations are idiomatic and use memory-locked storage for sensitive data.
- **Hardening Mandates**:
    - **Process Protection**: The agent MUST disable core dumps via `libc::prctl(libc::PR_SET_DUMPABLE, 0)` on startup.
    - **IPC Verification**: Every Unix socket connection MUST be verified via `SO_PEERCRED`. Reject connections from UIDs not matching the agent's.
    - **Socket Security**: Unix sockets MUST be created with `0600` permissions.
    - **IPC Protocol**: For non-subscription requests, the agent MUST call `socket.shutdown()` after the response to signal EOF. For events, use the `Action::Subscribe` protocol.

### 2. The CLI Specialist
- **Focus**: `cosmic-bwarden-cli`, user ergonomics, command parsing.
- **Goal**: Maintain the flexible, natural-language feel of the CLI.
- **Guideline**: Always update `preprocess_args` and `--help` (via `after_help` with EXAMPLES) when adding new features.

### 3. The COSMIC UI Developer
- **Focus**: `cosmic-bwarden-ui`, `libcosmic` widgets.
- **Goal**: Create a visually stunning, native COSMIC experience.
- **Guideline**: Follow the MVU (Model-View-Update) pattern strictly and align with COSMIC HIG.

### 4. The Verification Expert
- **Focus**: `cosmic-bwarden-tests`, CI integration, Docker/Vaultwarden environments.
- **Goal**: Ensure zero regressions.
- **Guideline**: Every bug fix or feature must have a corresponding test case in `crates/cosmic-bwarden-tests/src/lib.rs`.

## Communication Protocols

- **Self-Correction**: If an implementation fails (e.g., "Broken pipe" in IPC), analyze the protocol limits before applying patches.
- **Context Efficiency**: Use `grep_search` to find symbols before reading full files. Use `update_topic` to track high-level strategic pivots.
- **Documentation**: Keep `CONTEXT.md` updated with significant architectural shifts. Use `MEMO.md` (private) for local machine-specific notes.

## Preferred Tool Patterns

- **Rust Development**: 
  - Use `cargo check` frequently to validate types.
  - Use `cargo test -p cosmic-bwarden-tests -- --test-threads=1` for E2E validation.
- **File Editing**: 
  - Prefer `replace` with ample context to ensure uniqueness.
  - Avoid multiple `replace` calls on the same file in a single turn.
- **Help Documentation**: 
  - Use `after_help` with `EXAMPLES:` and `ENTRY TYPE DETAILS:` headers for a native look.
