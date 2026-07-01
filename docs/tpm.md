# TPM PIN Unlock

`cosmic-bwarden-agent` can seal your vault keys inside a TPM 2.0 chip so
unlocking requires only a short PIN rather than your full master password.
An optional second blob stores a sealed copy of your master password hash,
enabling silent Bitwarden server re-authentication (for sync) after a PIN unlock.

## How it works

### PIN unlock (vault keys)

Your vault is encrypted with a pair of keys (`enc_key ‖ mac_key`). Normally
these keys are re-derived from your master password every time you unlock. TPM
PIN unlock seals a copy of those keys inside the hardware chip so a short PIN
suffices instead.

1. During setup (`cosmic-bwarden-cli tpm setup`) the agent derives your vault's
   encryption keys from your master password, then asks for a 6-character-minimum
   PIN.
2. A symmetric primary key is created deterministically from the TPM's owner
   hierarchy seed using an **AES-128-CFB** template — the primary key is never
   stored anywhere; the TPM recreates it on demand.
3. The 64-byte vault key material is sealed into a **KeyedHash/Null** object
   protected by the PIN. Dictionary-attack (DA) lockout is enabled: too many
   wrong PINs trigger a TPM-enforced delay counted in hardware.
4. The sealed blob is written to:
   ```
   ~/.local/share/cosmic-bwarden/tpm_sealed_<hex16>.bin
   ```
   where `<hex16>` is the first 16 hex characters of SHA-256(`server + "\0" + email`),
   scoped so one blob exists per account.
5. On subsequent unlocks the agent unseals the blob with your PIN — no network
   call needed. If the TPM is not available (e.g. wrong machine, damaged chip),
   the agent falls back to the normal master password prompt.

**Trade-off**: anyone who knows your PIN *and* has physical access to this
device can decrypt your vault contents without knowing your master password.
Protect your PIN like a password.

### Stored hashed password (optional)

PIN unlock only restores the local vault decryption keys. It does **not**
restore a session with the Bitwarden server. This means that after a pure PIN
unlock, sync and other server operations will fail until you authenticate with
your master password once. The "Store hashed password" option below removes
this limitation.

When "Store hashed password in this device's TPM chip" is enabled in Settings
(disabled by default), a second sealed blob is created:

```
~/.local/share/cosmic-bwarden/tpm_sealed_hash_<hex16>.bin
```

This blob contains the master password hash used for Bitwarden server
authentication. Unlike the vault-key blob it is **not** PIN-protected — the
TPM hardware binding alone restricts access. After a PIN unlock the agent
silently unseals this hash, authenticates to the server, and refreshes the
sync without ever prompting for the master password.

**Trade-off**: Fewer master password prompts, but if your PIN is compromised,
anyone with physical access to this device and its TPM chip can authenticate to
your Bitwarden server and modify your account without knowing your actual master
password.

### TPM context probe order

The agent tries the following to open a TPM context, in order:

1. `TSS2_TCTI` environment variable (explicit TCTI string — useful for tests or
   custom TPM emulators)
2. `/dev/tpmrm0` — the resource-manager device node (preferred; handles concurrent
   access)
3. `/dev/tpm0` — the raw device node (works when no resource manager is running)
4. `tabrmd:` — the userspace TPM Access Broker & Resource Manager daemon

## Hardware and software requirements

- **TPM 2.0 chip** — TPM 1.2 is not supported.
- **User must have access to the TPM device**. One of:
  - Be a member of the `tss` group:
    ```sh
    sudo usermod -aG tss $USER
    ```
    (log out and back in for the group change to take effect)
  - Or install and enable the TPM Access Broker daemon (`tpm2-abrmd`):
    ```sh
    sudo systemctl enable --now tpm2-abrmd
    ```
- **`tss-esapi`** Rust crate (already a dependency when the `tpm` feature is
  compiled in — the default desktop build).

## 1. Setting up PIN unlock

### Via the UI

Open the COSMIC applet → **Settings** → scroll to the **TPM** section.

If the TPM section shows diagnostic errors, resolve the hardware/permissions
issue first (see [Diagnostics](#diagnostics)).

1. Click **Set up PIN unlock**.
2. Enter your current master password (to verify identity and derive vault keys).
3. Choose a PIN of at least 6 characters. A longer PIN provides more entropy
   against the DA lockout limit.
4. Click **Confirm**.

The blob is written immediately; subsequent unlocks will show a PIN prompt
instead of a master password prompt.

### Via the CLI

```sh
cosmic-bwarden-cli tpm setup
```

The CLI prompts for your master password (to verify and load vault keys) then
for a PIN. The same 6-character minimum applies.

## 2. Enabling the stored hashed password

This setting appears only after PIN unlock is configured.

### Via the UI

Settings → TPM section → toggle **Store hashed password in this device's TPM
chip** to on.

### Via the CLI

```sh
cosmic-bwarden-cli tpm enable-server-credentials
```

Or disable it:

```sh
cosmic-bwarden-cli tpm disable-server-credentials
```

Note: enabling this requires the vault to be unlocked with the **master
password** (not just the PIN), because the hash is only available after a
full password-based login. If you've only done a PIN unlock, lock the vault,
unlock it with the master password, and retry.

## 3. Removing TPM unlock

To stop using PIN unlock and return to master-password-only unlocking:

### Via the UI

Settings → TPM section → **Remove PIN unlock**.

### Via the CLI

```sh
cosmic-bwarden-cli tpm remove
```

This deletes both blob files for the current account. The TPM primary key is
also destroyed (it is re-derived from the hardware seed on demand, so there is
nothing else to clean up).

## Diagnostics

When the TPM is not available or has a configuration problem, Settings shows
a diagnostic panel with four checks:

| Check | What it means |
|---|---|
| `/dev/tpmrm0` exists | The resource-manager device node is present |
| `/dev/tpm0` exists | The raw device node is present (fallback) |
| Can open `/dev/tpmrm0` | Your user has read/write permission |
| TPM 2.0 context opens | A full `tss-esapi` session can be established |

If the first two checks fail: the system has no TPM 2.0 chip (or the kernel
module is not loaded — try `sudo modprobe tpm_tis` for older machines).

If the third check fails: your user is not in the `tss` group and `tpm2-abrmd`
is not running (see requirements above).

If only the fourth check fails with the first three passing: a `tss-esapi`
version mismatch or a broken TCTI configuration. Set `TSS2_TCTI` explicitly
to override the probe order.

You can also run:

```sh
cosmic-bwarden-cli tpm diagnostics
```

to see the same four-item report in the terminal.

## Security notes

- **PIN vs master password**: the PIN is used only to unseal the TPM blob; it
  does not protect your Bitwarden account directly. Changing your Bitwarden
  master password does not automatically rotate the TPM blob — re-run `tpm setup`
  after a master password change.
- **Machine binding**: the sealed blob is cryptographically tied to the specific
  TPM chip. Moving the blob file to another machine will not unseal it.
- **Backup**: the blob file can be backed up but is useless without the original
  TPM hardware. Losing the blob (or the machine) requires re-running `tpm setup`
  on the same hardware after unlocking with the master password.
- **DA lockout**: the default TPM DA lockout policy applies. On most TPM 2.0
  firmware this is 32 failures before a recovery lockout of ~24 hours. The raw
  lockout interval depends on the platform — check `tpm2_getcap properties-variable`
  for `TPM_PT_LOCKOUT_INTERVAL`.

## End-to-end test coverage

`crates/cosmic-bwarden-tests/src/vault/tpm.rs` and `vault/tpm_lifecycle.rs`
exercise the full flow against a live Vaultwarden container. They require a
software TPM emulator (`swtpm`) in the test environment. Run with:

```sh
cargo test -p cosmic-bwarden-tests vault::tpm -- --test-threads=1
```

(requires Docker or Podman, and `swtpm` installed).
