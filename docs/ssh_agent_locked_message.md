# Locked SSH agent: list real keys, wait to sign

Coding agents (and humans running `ssh-add -l` / `git push`) treat an
empty identity list as “SSH is not set up” and then generate keys, rewrite
remotes to HTTPS, or point `SSH_AUTH_SOCK` at a different agent. This
document is the decision record for how Cosmarden’s ssh-agent
behaves while the vault is **locked**.

User-facing protocol behaviour also lives in [`ssh-agent.md`](ssh-agent.md).
The UI subscriber primes the existing unlock form and sends a desktop
`org.freedesktop.Notifications` `Notify` (“Unlock is requested”, symbolic
app icon). It does **not** auto-open the applet popup.

## Decision

| Topic | Choice |
|---|---|
| Identities while locked | List the **real** cached public-key blobs |
| Comment while locked | Append the token `[cosmarden:locked]` to the entry name |
| Comment while unlocked | Entry name only — no token |
| Fake / sentinel identity | **Rejected** |
| Sign while locked | **Block** until unlock in the same process, or fail after **90s** |
| Public-key cache | **RAM only**, filled on unlock / vault mutation, **survives `lock()`**, **cleared on logout** |
| Cold start already locked | Empty list (no on-disk pubkey cache) |
| Extra applet/app unlock UI | Desktop `Notify` from the UI on `UnlockRequested` / `PinRequested` (no auto-popup) |

## Why not a fake key

A dummy identity whose comment is a human sentence (“unlock the applet…”)
is only visible to something that runs `ssh-add -l`. `ssh` / `git` offer
that blob to the server; the server has never seen it, so **no sign
request is issued**, the command fails immediately with `Permission
denied (publickey)`, and a coding agent still “fixes SSH”.

It also burns a `MaxAuthTries` slot, makes `ssh-add -l` exit 0 when the
agent cannot actually authenticate, and cannot satisfy
`IdentitiesOnly yes` + `IdentityFile ~/.ssh/*.pub` (match is by key blob,
not comment).

## Why list the real blobs

OpenSSH only asks the agent to sign keys the agent **listed**. Listing
the real public keys while locked is what lets `git push` reach `sign`,
where we can wait for unlock. 1Password (on-disk public keys) and
Bitwarden desktop (list supported while locked) already do this.

Public keys are public. Caching them in process memory across `lock()` is
the same class of data already shown on GitHub / `authorized_keys`.
Entry **names** stay in RAM with the blobs (needed for `ssh-add -l`);
they are not written to disk.

The token is short, stable, and **not localised** so scripts and models
can grep it. OpenSSH includes the comment in
`sign_and_send_pubkey: signing failed for ED25519 "…": agent refused
operation`, so even a fail-fast sign would still name the locked state;
waiting is better because the command can succeed.

## Why wait on sign

Fail-fast `"agent is locked"` usually surfaces as `agent refused
operation` (our string is hidden unless `-vvv`). Coding agents then
still invent a second SSH stack.

If `sign` waits, `git push` hangs, the existing `UnlockRequested` /
`PinRequested` broadcast primes the unlock form and fires a desktop
notification so the user can unlock, and the same request completes. No
retry is required.

List stays instant — `ssh-add -l` and “is the agent alive?” probes must
not block.

## Cache lifetime

```
never unlocked this process  →  empty list
unlocked once                →  cache filled from decrypted SSH items
lock()                       →  private keys dropped; pubkey+comment cache kept
list while locked            →  same blobs, comments suffixed
unlock                       →  comments unadorned; sign uses vault private keys
logout / clear account       →  cache cleared; empty list
unauthorized peer UID        →  empty list (unchanged)
```

Rebuilding the cache is attached to `State::rebuild_sidebar_cache()`
(unlock, login, PIN unlock, sync, vault mutation). `lock()` does **not**
clear it. There is no on-disk snapshot: an agent that starts already
locked still reports no identities until the first unlock of that
process.

## Sign wait

- Do not hold the `State` mutex while waiting.
- Observe an in-process watch (`VaultSession::{Unlocked, Locked, LoggedOut}`).
- Bound: **90 seconds** (`ssh_sign::DEFAULT_SIGN_WAIT`). Tests inject a
  shorter duration on `SshAgent`.
- Unlock in time → decrypt the matching private key and sign (same path
  as an already-unlocked sign).
- Timeout / logout / key not in cache → no signature from vault key
  material.
- `request_unlock()` still runs on list and on sign, once per lock
  period, so existing subscriber tests keep passing.

## Rejected alternatives

| Option | Why not |
|---|---|
| Empty list (OpenSSH `ssh-add -x` lock) | Current behaviour; coding agents treat it as “no SSH” |
| `SSH_AGENT_FAILURE` on list | Clients report “communication with agent failed” and spawn a new agent |
| Instructional comment *instead of* the real blob | `ssh`/`git` never sign; `IdentitiesOnly` cannot match |
| On-disk public-key cache | Follow-up; extra at-rest plaintext, not needed for same-process lock |
| Indefinite wait | Wedges unattended / CI; 90s is enough for a human at the applet |
| Localising the token | Scripts and models parse `ssh-add -l` |
| Auto-open applet popup | cosmic-panel drops popups unless a panel surface is hovered |

## Tests

Agent-crate tests drive the shipped `SshAgent` `Session` against a real
`State` (no mock session, no reimplemented list/sign):

- unlocked list: real blob, comment without the token
- lock then list: same blob, comment contains `[cosmarden:locked]`
- unlock then list: token gone
- logout / never-unlocked / unauthorized: empty list
- sign while locked does not complete before unlock; after unlock it
  yields a signature that verifies with the cached public key
- sign that outlives the injected wait without unlock fails

E2E (`vault::ssh_agent_lifecycle`): locked `ssh-add -l` must still show
the real key plus the token; a live `ssh` login must **not** succeed
while the vault stays locked (the client is time-bounded so the suite
does not sit on the 90s wait).
