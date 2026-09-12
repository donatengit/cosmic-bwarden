# SSH Agent

`cosmarden-agent` implements the `ssh-agent` protocol and serves SSH
identities directly from your unlocked Bitwarden/Vaultwarden vault — no
separate `ssh-agent`, `ssh-add`, or on-disk private key files needed.

## How it works

- The agent exposes a Unix socket at:
  ```
  $XDG_RUNTIME_DIR/cosmarden/ssh-agent-socket
  ```
  (or `$XDG_RUNTIME_DIR/cosmarden-<PROFILE>/ssh-agent-socket` if
  `COSMARDEN_PROFILE` is set — used for test isolation, not normal use).
  If `XDG_RUNTIME_DIR` isn't set (rare — non-systemd sessions), it falls back
  to `/tmp/cosmarden-<uid>/ssh-agent-socket`.
- Every vault item of type **SSH Key** with a public key becomes an identity
  returned by `ssh-add -l` / `SSH2_AGENTC_REQUEST_IDENTITIES`.
- Signing (`SSH2_AGENTC_SIGN_REQUEST`) decrypts the matching private key
  in-memory, signs the challenge, and discards it — the private key is never
  written to disk.
- **Listing while locked**: after the vault has been unlocked once in this
  agent process, `ssh-add -l` still returns the same public keys. Each
  comment is the entry name plus `[cosmarden:locked]`. A process that
  has never been unlocked, or that has been logged out, still reports no
  identities.
- **Signing while locked**: the agent does not sign with vault private-key
  material until you unlock. The sign request waits (up to 90 seconds) for
  an unlock in the same process, then completes; if nobody unlocks, it
  fails. See [`ssh_agent_locked_message.md`](ssh_agent_locked_message.md)
  for the rationale.

### Socket permissions

On startup the agent creates its runtime directory with mode `0700` and the
`ssh-agent-socket` file with mode `0600` — the same model a real `ssh-agent`
uses, so only your user (and root) can connect. Unlike the main IPC socket
(`socket`, used by cosmarden's own UI/CLI), the ssh-agent socket does
*not* enforce a peer-UID check: `SSH_AUTH_SOCK` is conventionally shared with
`sudo`-elevated processes and containers/sandboxes that bind-mount it, and a
strict UID match would break those workflows. Filesystem permissions are the
intended security boundary here, matching upstream OpenSSH `ssh-agent`.

### Supported key types

| Key type | `request_identities` | `sign` |
|---|---|---|
| Ed25519 | ✅ | ✅ |
| RSA | ✅ | ✅ — negotiates `rsa-sha2-512`, `rsa-sha2-256`, or legacy `ssh-rsa` based on the client's requested flags |
| ECDSA / others | listed if the stored public key parses | ❌ fails with "unsupported key type" |

## 1. Storing an SSH key in the vault

If you're self-hosting Vaultwarden, enable the SSH key item type first:

```
EXPERIMENTAL_CLIENT_FEATURE_FLAGS=ssh-key-vault-item,ssh-agent
```

Add a key via the CLI (the private key is read from stdin/prompt if
`private_key=` is omitted):

```sh
cosmarden-cli sshkey add "My Work Key" \
  private_key="$(cat ~/.ssh/id_ed25519)" \
  public_key="$(cat ~/.ssh/id_ed25519.pub)"
```

Generated keys can be created with `ssh-keygen -t ed25519` as usual — just
import the resulting private/public key pair instead of leaving the files on
disk, then delete the local copies if you want the vault to be the sole
source of truth.

## 2. Pointing clients at the agent

With the agent running and the vault **unlocked**:

```sh
export SSH_AUTH_SOCK="$XDG_RUNTIME_DIR/cosmarden/ssh-agent-socket"
```

Add this to your shell profile (`~/.bashrc`, `~/.zshrc`, etc.) so every new
shell picks it up. If another `ssh-agent` (e.g. GNOME Keyring, a manually
started `ssh-agent`) already owns `SSH_AUTH_SOCK`, this will override it for
that shell — cosmarden does not chain to other agents.

## 3. Verifying it works

List identities:

```sh
ssh-add -l
```

You should see one line per SSH-key vault entry, with the comment set to the
entry's name. After a lock in the same agent process the same keys are still
listed, with `[cosmarden:locked]` on each comment; `The agent has no
identities.` means this process has not unlocked (or has logged out), not
that the socket is broken.

Test a real connection:

```sh
ssh -o IdentitiesOnly=no user@host
```

`ssh` will offer every identity from `SSH_AUTH_SOCK` automatically — no
`-i`/`IdentityFile` flag needed.

## 4. Lock / unlock and login / logout behavior

- **Lock**: public keys stay listed (comments gain `[cosmarden:locked]`);
  in-flight `sign` requests wait for unlock in this process (90s bound)
  rather than failing immediately. Unlocking restores signing of the same
  keys without any re-import.
- **SSH request while locked**: the first `ssh-add -l`/`sign` request after a
  lock broadcasts `UnlockRequested` (or `PinRequested` when a TPM PIN is
  configured), logged at `warn` and sent at most once per lock period. The
  UI primes its unlock form and sends a desktop notification (“Unlock is
  requested”, symbolic app icon). It does **not** auto-open the applet
  popup — click the panel icon (or the notification) to type PIN/password.
  Listing returns immediately; signing waits for that unlock.
- **Logout**: identities disappear until you log back in and sync (the
  in-memory public-key cache is cleared). After re-login + sync, previously
  stored SSH keys are available again (they're fetched from the server, not
  just the local cache).

## Troubleshooting

- **`ssh-add -l` says "no identities" but the vault has an SSH key entry**:
  this agent process has not unlocked since start (or you logged out).
  Unlock (`cosmarden-cli unlocked` / the applet), then list again. If
  it's unlocked and the key still doesn't show up, check
  `cosmarden-cli get "My Work Key"` returns a populated `public_key` —
  if it doesn't, the entry itself is missing key data (re-add it).
- **`ssh-add -l` shows `[cosmarden:locked]`**: the vault is locked in
  this process; unlock the applet or app and retry the SSH/git command
  (a sign already in flight will complete on unlock).
- **`sign` fails with "no matching key found"**: the public key offered by
  the SSH server during negotiation doesn't match any vault entry's stored
  public key byte-for-byte. Re-check the `public_key=` value stored in the
  vault matches the actual keypair.
- **`sign` fails with "unsupported key type"**: only Ed25519 and RSA private
  keys can be used for signing; other key types (e.g. ECDSA) can be listed
  but not used.
- **Socket doesn't exist**: confirm the agent process is running and check
  its logs (`RUST_LOG=debug`) for `ssh-agent-socket` bind errors — usually a
  stale socket file or permissions issue in `$XDG_RUNTIME_DIR`.

## End-to-end test coverage

`crates/cosmarden-tests/src/vault/ssh_agent.rs` and
`vault/ssh_agent_lifecycle.rs` exercise this entire flow against a real
`sshd` container using real `ssh`/`ssh-add` commands (Ed25519 + RSA, plus
lock/unlock and logout/login cycles). Run with:

```sh
cargo test -p cosmarden-tests vault::ssh_agent -- --test-threads=1
```

(requires Docker or Podman).
