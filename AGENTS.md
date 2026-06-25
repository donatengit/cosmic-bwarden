# cosmic-bwarden: Agent Guidelines

## Key Documents

Start here, then go deeper as needed:

| Document | Purpose |
|---|---|
| [`CONTEXT.md`](CONTEXT.md) | Architecture, security invariants, key workflows, "game-changing" features — the canonical project map |
| [`docs/summary.md`](docs/summary.md) | User-facing project overview and feature list |
| [`docs/build_and_run.md`](docs/build_and_run.md) | Prerequisites, build commands, run modes |
| [`docs/testing.md`](docs/testing.md) | Complexity-ordered testing strategy and what each test suite covers |
| [`docs/ssh-agent.md`](docs/ssh-agent.md) | SSH agent protocol implementation and socket paths |
| [`docs/browser_integration.md`](docs/browser_integration.md) | Native browser extension architecture and IPC protocol |
| [`docs/configurable_paths.md`](docs/configurable_paths.md) | Socket, config, and SSH path overrides; multi-instance isolation |
| [`docs/cosmic_integration.md`](docs/cosmic_integration.md) | COSMIC panel applet registration and metadata |
| [`docs/implementation.md`](docs/implementation.md) | Crypto, vault sync, and data model internals |

## Golden Rules
- **Never ask for confirmation.** Apply fixes, run validation, iterate until passing. Report only on final outcome or exhausted options.
- **Never circle back to a failed approach.** If a fix didn't work, note why and move forward.
- **One responsibility per file.** If a file exceeds ~250 lines, it needs splitting.
- **cargo check before cargo test.** Don't run expensive tests against code that won't compile.

## Versioning

- **Build version**: `YYYY.MM-N-<git_id>` generated in `core/build.rs`. Reused across crates via a 30-second `target/build_version.txt` cache.
- **Protocol version**: Currently identical to build version. `Response::Version` always includes both `version` and `protocol_version` fields.
- **Compatibility check**: Pure function `check_protocol_compatibility()` in the CLI crate compares local version against agent's `protocol_version`. Unit-tested for both match and mismatch scenarios.
- **Adding a version subcommand**: Always add `Commands::Version` to the CLI's enum, route it to the auth handler, and include the `check_protocol_compatibility()` call. Update `preprocess_args` if the new command name conflicts with type keywords.
- **Breaking protocol changes**: Bump the `protocol_version` in `Response::Version` by updating `check_protocol_compatibility` expectations if the protocol surface changes incompatibly.

## Workspace Structure

| Crate | Responsibility |
|---|---|
| `cosmic-bwarden-core` | Daemon, IPC server, crypto, vault state |
| `cosmic-bwarden-cli` | Argument parsing, IPC client, output formatting |
| `cosmic-bwarden-ui` | libcosmic applet, MVU model/view/update |
| `cosmic-bwarden-tests` | E2E tests against Vaultwarden in Docker |

## Security Invariants (core)
These must never regress. Treat violations as build-blocking bugs.

- **Core dumps**: `libc::prctl(PR_SET_DUMPABLE, 0)` on daemon startup.
- **IPC auth**: Verify every connection via `SO_PEERCRED`. Reject mismatched UIDs.
- **Socket perms**: Create Unix sockets with mode `0600`.
- **EOF signaling**: Call `socket.shutdown()` after non-subscription responses. Subscriptions use `Action::Subscribe` protocol.
- **Sensitive memory**: Use memory-locked storage for all key material and plaintext secrets.

## Workflow

### Fixing failing tests
1. Run `cargo test -p cosmic-bwarden-tests -- --test-threads=1`, capture full output.
2. Identify root cause from panic/error line — do not guess from test name alone.
3. `cargo check` after each edit before re-running tests.
4. If a fix attempt fails, document why before trying the next approach.
5. Every bug fix requires a corresponding test case.

### Adding features
1. Update `preprocess_args` and `--help` (`after_help` with `EXAMPLES:` block) for any CLI change.
2. Follow MVU strictly for UI changes — no logic in view functions.
3. Update `CONTEXT.md` for architectural changes.

## Code Organization
- **Target file size: 150–250 lines.** This is the range where edits are reliable and context fits cleanly.
- **Hard limit: 500 lines.** If a file exceeds this, split it before adding more code. No exceptions.
- **One module = one responsibility.** If you find yourself writing "and also" when describing what a file does, it needs splitting.

### Modular Patterns (Mandatory)
When a crate's main logic grows, decompose using these established patterns:
- **`cosmic-bwarden-agent`**: Split into `handler.rs` (request routing), `server.rs` (API interaction), and `logind.rs` (DBus events).
- **`cosmic-bwarden-core`**: 
    - `api/`: Split into `models.rs` (DTOs) and `client.rs` (Network logic).
    - `db/`: Split into `models.rs` (Data structs) and `persistence.rs` (File I/O).
- **`cosmic-bwarden-ui`**: Split into `app/state.rs` (State), `app/update.rs` (MVU logic), and `app/tasks.rs` (Async tasks).

- **When splitting**: prefer extracting into a sibling module (`mod foo;` in the parent) rather than a new crate unless the boundary is a genuine abstraction layer.
- **Before adding to a file**: check its current line count. If it's above 200, consider whether the new code belongs in an existing or new sibling module instead.

## Tool Discipline

- **Symbol lookup**: `grep_search` first, read full file only if needed.
- **File edits**: `replace` with enough surrounding context for uniqueness. One `replace` per file per turn maximum.
- **State tracking**: `update_topic` on strategic pivots. `MEMO.md` for local/machine-specific notes only.

## Validation Commands
```
cargo check -p <crate>
cargo test -p cosmic-bwarden-tests -- --test-threads=1
```
