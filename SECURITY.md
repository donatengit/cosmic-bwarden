# Security Policy

`cosmic-bwarden` handles password-vault material. Security reports are welcome and will
be taken seriously.

## Reporting a vulnerability

**Do not open a public issue for a security bug.**

- Preferred: GitHub → **Security** → *Report a vulnerability* (private advisory).
- Alternative: email **donat@enikeev.net** with `cosmic-bwarden security` in the subject.

Please include what you'd want to receive: affected component (agent / applet / UI / CLI
/ SSH agent / browser extension / TPM path), version (`cosmic-bwarden-cli --version`),
reproduction steps, and impact as you see it.

Expect an acknowledgement within 7 days and an assessment within 14. There is no bug
bounty — this is an unfunded personal project. Credit in the advisory and the release
notes is offered by default; say so if you'd rather stay anonymous.

I ask for 90 days before public disclosure, or until a fix ships if that comes sooner.
If a report goes unanswered for 30 days, treat that as consent to disclose publicly.

## Supported versions

Pre-release: only the tip of `main` is supported. There are no maintained release
branches, and no backports.

## Scope

In scope — anything that breaks these properties:

- Vault secrets reachable by a process **not** running as the vault owner's UID (socket
  permissions, `SO_PEERCRED` checks, D-Bus surfaces, the native-messaging host).
- Secrets leaving the agent when they shouldn't: bulk reads bypassing the
  master-password reprompt, secrets in logs, secrets persisted to disk in plaintext,
  clipboard contents outliving their auto-clear.
- Cryptographic defects: cipherstring parsing/verification, encrypt-then-MAC ordering,
  KDF parameter handling, TOTP derivation, key zeroization, `mlock` failures.
- TPM path: sealing policy, PCR binding, PIN handling and lockout behaviour.
- Browser extension: a web page reaching vault data it shouldn't, domain-matching
  bypasses (look-alike hosts, public-suffix confusion), secrets held in extension state.
- Protocol handling: malformed or oversized frames on the agent socket.

Known and accepted (documented in [docs/review/01_security.md](docs/review/01_security.md)) —
not vulnerabilities, though better mitigations are welcome as feature proposals:

- A hostile process running **as your user** can ultimately reach the agent. Same-UID is
  the trust boundary on a single-user desktop; reprompts raise cost, they are not a wall.
- Unencrypted swap can page session tokens out to disk.
- An attacker with root, kernel access, or physical DMA is out of scope.
- Vulnerabilities in Bitwarden/Vaultwarden servers themselves belong to those projects.

## What this project has and hasn't had

A self-imposed nine-phase review ([docs/review/](docs/review/)) covering security, data
integrity, architecture, UX, testing, performance, packaging, and docs. It found and
fixed three S0-class issues, all written up publicly.

It has **not** had an independent third-party security audit. Weigh that before trusting
it with a vault you can't afford to lose.
