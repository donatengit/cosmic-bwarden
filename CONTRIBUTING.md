# Contributing

Thanks for looking. This is a small, opinionated project; a few things will save you time.

**Security bugs do not go here** — see [SECURITY.md](SECURITY.md).

## Before you start

Open an issue first for anything larger than a bug fix. The backlog in
[docs/roadmap.md](docs/roadmap.md) says what's already planned, and
[docs/review/](docs/review/) records decisions that were made deliberately — including
things that look like gaps and aren't.

Read [`AGENTS.md`](AGENTS.md) (rules and invariants — it applies to humans too, it's just
written for the agent sessions that produced much of this code) and
[`CONTEXT.md`](CONTEXT.md) (architecture map). Every rule in AGENTS.md is a scar from a
real bug; please don't relitigate them in a PR.

## The gates

A change is ready when all of these are green:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features
cargo check --workspace --all-features       # --all-features matters: tpm code is
                                             # invisible to a plain check
cargo test -p cosmic-bwarden-core -p cosmic-bwarden-agent \
           -p cosmic-bwarden-cli -p cosmic-bwarden-ui    # unit tests, seconds
just test                                    # full E2E; needs a podman/docker socket
just test-extension-unit                     # browser-extension unit tests
```

`warnings = "deny"` is set workspace-wide. If a warning is genuinely unavoidable, use a
narrowly scoped `#[allow(...)]` with a comment explaining why — don't relax the lint.

## House rules worth knowing up front

- **File size**: target 150–250 lines, hard limit 500. One responsibility per file. The
  first week of this project was spent editing 2,000-line files; that's why the rule exists.
- **Every bug fix ships with a test that fails without it.** Reintroduce the bug and
  confirm the test catches it.
- **Test the decision seam, not a simulated response.** A test that hand-builds the
  `Action` it sends proves the agent handles it, not that the client emits it. Prefer a
  pure `state → Action` builder with a unit test over an E2E that feeds itself the answer.
- **Never `.ok()` a decrypt silently.** A swallowed error once put a cipherstring in a
  plaintext position and double-encrypted data.
- **Never `take()` an edit buffer before the agent confirms** — clone it, and clear only
  on success, or a rejected save throws away what the user typed.
- **Failures that can lose data log at `error!`**, loudly. Not `warn!`, not `debug!`.
- **Protocol changes bump `core::PROTOCOL_VERSION`** (a small integer, independent of the
  build-version string) only when the wire format actually breaks.
- Single-source constants (`MIN_PIN_LEN` and friends) rather than repeating a number in a
  UI caption.

## Commits and PRs

- Conventional-commit prefixes (`feat:`, `fix:`, `refactor:`, `docs:`, `chore:`), with a
  scope where it helps: `fix(ui): …`. Explain *why* in the body — the best commit in this
  repo's history is a full postmortem of the bug it fixes.
- Keep PRs focused; a refactor and a behaviour change in one diff is hard to review.
- If AI assistance wrote a meaningful part of the change, add a `Co-Authored-By:` trailer.
  This repo is explicit about that, and the history is more useful when the record is honest.
- By contributing you agree your work is licensed under [GPL-3.0-only](LICENSE).
