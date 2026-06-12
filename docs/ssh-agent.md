# SSH Agent

`cosmic-bwarden-agent` implements the `ssh-agent` protocol and serves SSH
identities directly from your unlocked Bitwarden/Vaultwarden vault — no
separate `ssh-agent`, `ssh-add`, or on-disk private key files needed.

## How it works

- The agent exposes a Unix socket at:
  ```
  $XDG_RUNTIME_DIR/cosmic-bwarden/ssh-agent-socket
  ```
  (or `$XDG_RUNTIME_DIR/cosmic-bwarden-<PROFILE>/ssh-agent-socket` if
  `COSMIC_BWARDEN_PROFILE` is set — used for test isolation, not normal use).
- Every vault item of type **SSH Key** with a public key becomes an identity
  returned by `ssh-add -l` / `SSH2_AGENTC_REQUEST_IDENTITIES`.
- Signing (`SSH2_AGENTC_SIGN_REQUEST`) decrypts the matching private key
  in-memory, signs the challenge, and discards it — the private key is never
  written to disk.
- Both identity listing and signing require the vault to be **unlocked**
  (`state.keys` populated). While locked, the agent reports zero identities
  and refuses to sign.

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
cosmic-bwarden-cli sshkey add "My Work Key" \
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
export SSH_AUTH_SOCK="$XDG_RUNTIME_DIR/cosmic-bwarden/ssh-agent-socket"
```

Add this to your shell profile (`~/.bashrc`, `~/.zshrc`, etc.) so every new
shell picks it up. If another `ssh-agent` (e.g. GNOME Keyring, a manually
started `ssh-agent`) already owns `SSH_AUTH_SOCK`, this will override it for
that shell — cosmic-bwarden does not chain to other agents.

## 3. Verifying it works

List identities (requires the vault to be unlocked):

```sh
ssh-add -l
```

You should see one line per SSH-key vault entry, with the comment set to the
entry's name. `The agent has no identities.` while locked is expected
behavior, not an error.

Test a real connection:

```sh
ssh -o IdentitiesOnly=no user@host
```

`ssh` will offer every identity from `SSH_AUTH_SOCK` automatically — no
`-i`/`IdentityFile` flag needed.

## 4. Lock / unlock and login / logout behavior

- **Lock**: identities disappear immediately; in-flight `sign` requests fail
  with "agent is locked". Unlocking restores access to the same keys without
  any re-import.
- **Logout**: same as lock — no identities until you log back in and sync.
  After re-login + sync, previously stored SSH keys are available again
  (they're fetched from the server, not just the local cache).

## Troubleshooting

- **`ssh-add -l` says "no identities" but the vault has an SSH key entry**:
  make sure the vault is unlocked (`cosmic-bwarden-cli unlocked`). If it's
  unlocked and the key still doesn't show up, check
  `cosmic-bwarden-cli get "My Work Key"` returns a populated `public_key` —
  if it doesn't, the entry itself is missing key data (re-add it).
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

`crates/cosmic-bwarden-tests/src/vault/ssh_agent.rs` and
`vault/ssh_agent_lifecycle.rs` exercise this entire flow against a real
`sshd` container using real `ssh`/`ssh-add` commands (Ed25519 + RSA, plus
lock/unlock and logout/login cycles). Run with:

```sh
cargo test -p cosmic-bwarden-tests vault::ssh_agent -- --test-threads=1
```

(requires Docker or Podman).
