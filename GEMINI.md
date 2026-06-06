# corbw Project Guidance

This project uses specialized documentation files for architecture, security, and agent workflows. 

- **Architectural & Security Mandates**: See [CONTEXT.md](./CONTEXT.md) for the "Thin Client Invariant" and reactive state model.
- **Agent Workflows & IPC Security**: See [AGENTS.md](./AGENTS.md) for agent hardening, IPC protocols, and developer personas.

All development MUST adhere to the mandates defined in these files.

## Testing Hierarchy Mandate
To optimize context usage and ensure logical verification, tests MUST be run in the following order:
1. **Unit Tests** (`core`, `ui --lib`)
2. **Agent & Protocol E2E** (`tests/agent.rs`, `security.rs`, `vault_ops.rs`, `pinned_ops.rs`)
3. **CLI E2E** (`tests/cli_lifecycle.rs`, etc.)
4. **UI E2E** (`tests/window_flow.rs`, etc.)

Use `just test` to execute the full suite in this order.
