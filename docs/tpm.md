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
   whose auth policy is `PolicyPCR(SHA-256, {0,7}) ∧ PolicyAuthValue`. Unsealing
   therefore requires **both** the PIN (auth value) **and** a matching boot state:
   PCR 0 (firmware/UEFI code) and PCR 7 (Secure Boot state). `userWithAuth` is
   false, so the PIN is only usable through this policy — it cannot be used to
   authorize the object under a different boot state. Dictionary-attack (DA)
   lockout is enabled: too many wrong PINs trigger a TPM-enforced delay counted in
   hardware. The unseal uses a session with parameter encryption so the recovered
   key is not exposed in the clear on the TPM bus.

   **Consequence — firmware/Secure-Boot changes invalidate the blob.** A BIOS
   update, enabling/disabling Secure Boot, or booting different firmware changes
   PCR 0/7 so the policy no longer satisfies and the blob will not unseal. This is
   the intended anti-evil-maid property. When it happens the agent falls back to
   the master-password prompt; simply re-run PIN setup to reseal against the new
   boot state. Sealed blobs are versioned (`v2`); blobs from before PCR binding
   cannot be unsealed and must be re-created via PIN setup.
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

### Restoring the server session

PIN unlock restores the local vault decryption keys, and with them the server
session: the vault keys decrypt the **session envelope**
(`~/.local/share/cosmic-bwarden/session_<hex16>.enc`), which holds the server
refresh token under XChaCha20-Poly1305. The agent exchanges that token for a
fresh access token and syncs, with no master-password prompt.

The refresh token is a deliberately weak credential — revocable from the
server's device list, self-expiring (30 days on Vaultwarden for a desktop
device), and unable to change the account. It is rewritten on every successful
refresh, so ordinary use keeps rolling the window forward.

When it cannot be used — the device was offline past the expiry window, the
session was revoked, the account requires two-factor sign-in, or the TPM state
changed — the vault still unlocks and works offline, and you are asked for your
master password once to restore syncing.

> **No master-password hash is stored on this device.** An earlier version
> sealed one in a second TPM blob to avoid that prompt. It was removed: it kept
> the strongest credential in the system to save a prompt that fires at most
> once per refresh-token lifetime, and it could not satisfy two-factor sign-in
> anyway. Agents delete any leftover `tpm_sealed_hash_*.bin` at startup.

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

## 2. Removing TPM unlock

To stop using PIN unlock and return to master-password-only unlocking:

### Via the UI

Settings → TPM section → **Remove PIN unlock**.

### Via the CLI

```sh
cosmic-bwarden-cli tpm remove
```

This deletes the sealed blob for the current account. The TPM primary key is
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
- **Boot-state binding (PCR 0/7)**: the blob is bound to the firmware and Secure
  Boot state at seal time. Booting other firmware/OS (e.g. from USB) to lift the
  blob off disk will not unseal it, and a firmware/Secure-Boot change invalidates
  the blob until you re-run PIN setup (see the sealing steps above).
- **PIN policy**: the agent enforces a minimum PIN length (empty/too-short PINs
  are rejected server-side, not just in the UI), since the sealed blob lives on
  disk and a weak PIN is bounded only by DA lockout.
- **Backup**: the blob file can be backed up but is useless without the original
  TPM hardware. Losing the blob (or the machine) requires re-running `tpm setup`
  on the same hardware after unlocking with the master password.
- **No stored master password**: the master-password hash is derived per unlock,
  sent to the server, and dropped — it is never written to a TPM blob, the
  session envelope, or agent state. The session envelope holds only the refresh
  token, and an expired one costs a master-password prompt by design.
- **DA lockout**: the default TPM DA lockout policy applies. On most TPM 2.0
  firmware this is 32 failures before a recovery lockout of ~24 hours. The raw
  lockout interval depends on the platform — check `tpm2_getcap properties-variable`
  for `TPM_PT_LOCKOUT_INTERVAL`.

## End-to-end test coverage

`crates/cosmic-bwarden-tests/src/vault/tpm.rs` and `src/tpm_lifecycle/`
exercise the full flow against a live Vaultwarden container. They require a
software TPM emulator (`swtpm`) in the test environment. Run with:

```sh
cargo test -p cosmic-bwarden-tests --features tpm-smoke -- tpm_lifecycle --test-threads=1
```

(requires Docker or Podman, `swtpm` + `swtpm_setup` in PATH, and a
`target/debug/cosmic-bwarden-agent-tpm` built with `--features tpm`.)
