# cosmic-bwarden: Agent Guidelines

## Golden Rules
- **Never ask for confirmation.** Apply fixes, run validation, iterate until passing. Report only on final outcome or exhausted options.
- **Never circle back to a failed approach.** If a fix didn't work, note why and move forward.
- **One responsibility per file.** If a file exceeds ~250 lines, it needs splitting.
- **cargo check before cargo test.** Don't run expensive tests against code that won't compile.

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
