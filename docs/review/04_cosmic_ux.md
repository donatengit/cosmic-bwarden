# Phase 4 — COSMIC-Native Integration & UX

Reviewed: 2026-07-04. Scope: applet/panel registration, desktop metadata, systemd unit,
session integration (logind), theming, i18n, keyboard/a11y, and the SOTA feature-parity
ranking. Static + runtime-under-systemd review; items needing an interactive COSMIC
session (visual light/dark pass, panel embedding) are listed as manual follow-ups.

Severity: S2 must-fix-before-release · S3 polish. (No S0/S1 in this phase's scope.)

## Verdict

Integration is genuinely COSMIC-native: correct applet markers in the desktop entry,
valid Cosmic-theme icon, proper `.ron` metadata, one-binary/two-modes applet↔window
design, and a solid logind auto-lock. The systemd unit was bare — now hardened and
runtime-validated. The biggest pre-release item is the app-ID namespace. **Gate: PASS**;
UX backlog ranked below.

## Fixed this phase

- **U4-2 (S2) — systemd unit had zero hardening.** Added `NoNewPrivileges`,
  `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6`, `MemoryDenyWriteExecute`,
  `ProtectKernel{Tunables,Modules}`, `ProtectControlGroups`, `RestrictNamespaces`,
  `RestrictRealtime`, `LockPersonality`, `SystemCallArchitectures=native`, `UMask=0077`;
  dropped the meaningless (user-manager) `After=network.target`.
  **Validated at runtime**: agent launched via `systemd-run --user` with exactly these
  properties on an isolated profile — binds both sockets, answers IPC (CLI `version`
  round-trip), connects to the system bus for logind; no denials in the journal.
  `ProtectHome`/`ProtectSystem=strict`/`SystemCallFilter` deliberately deferred until
  tested against keyring+TPM+network paths (roadmap).
- **U4-3 (S3) — `docs/cosmic_integration.md` was stale** (described an
  `org.cosmic-bwarden.applet` ID and a `cosmic-bwarden-ui` binary that don't exist).
  Rewritten to match the justfile's actual install set.
- **U4-9 (S3) — hardcoded red error text** in `view/settings.rs` replaced with the
  theme's `destructive_text_color()`, so it tracks light/dark/high-contrast.

## Verified good

- **Desktop entry** (`com.system76.CosmicBWarden.desktop`): `X-CosmicApplet=true`,
  `X-CosmicHoverPopup=Auto`, `OnlyShowIn=COSMIC;`, sane categories/keywords.
- **Panel icon**: `password-manager-symbolic` exists in the installed Cosmic icon theme
  (`/usr/share/icons/Cosmic/scalable/apps/`).
- **Session integration** (`logind.rs`): locks on `Session.Lock`, `PrepareForShutdown`,
  `PrepareForSleep`; match rules registered explicitly; stream loss logs `error!` loudly.
- **i18n discipline**: `fl!` coverage is complete — the only bare widget strings are
  exempt glyphs; keys are verified at compile time against the fallback `.ftl`.
- **Theming**: no hardcoded colors remain (post-U4-9); widgets use theme classes.
- **MVU/applet conventions** match the `cosmic-applets` examples (popup autosize,
  context-menu structure, activation-token flow for spawning the vault window).

## Open findings

**U4-1 (S2) — app ID squats `com.system76.*`.** Everything (desktop file, `.ron`,
`StartupWMClass`, `CONFIG_ID`) uses `com.system76.CosmicBWarden`, but this is not a
System76 project. Blocks store/Flatpak publishing (ID must be a namespace we control,
e.g. `io.github.<owner>.CosmicBWarden`) and needs a config-migration step for
`CONFIG_ID`. → roadmap, pre-publish.

**U4-4 (S3) — emoji as button icons** (`view/applet/search.rs`: 📂 🔗 🔑). Render
inconsistently across icon themes, don't recolor, and carry no accessible name. Replace
with symbolic icons + tooltips.

**U4-5 (S3) — no keyboard handling.** No `Escape` (dismiss popup/dialogs), no arrow-key
navigation in result lists; only `Enter` via `on_submit` works. Keyboard-first operation
is a SOTA bar for a password manager. Also no global "quick access" shortcut story yet.

**U4-6 (S3) — single locale.** Only `en` exists, so the Fluent pipeline has never been
proven with a real translation. Add one full second locale (`ru` would suit the
maintainer) as the pipeline proof.

**U4-7 (S3) — unbranded panel icon.** `icons/black.svg`/`white.svg` exist but aren't
installed as themed symbolic icons; the applet TODO in `view/applet/mod.rs` tracks it.

**U4-8 (S3) — `PrepareForSleep` not filtered by its bool argument** — also fires on
resume; the extra `lock()` is a no-op today, but reading the body would make the intent
explicit and log messages accurate.

## Manual follow-ups (need an interactive COSMIC session)

- Visual pass: light/dark/high-contrast on applet popup, vault window, dialogs.
- Panel embedding: icon at 16/24 px scales, hover popup behaviour, multi-monitor.
- First-run flow walk-through (feeds Phase 8).

## SOTA feature-parity backlog (ranked, not built)

| # | Feature | Effort | Notes |
|---|---------|--------|-------|
| 1 | **Clipboard auto-clear** (P1-9) | S | UI + applet + extension; 20–30 s timer; the one glaring parity gap with security weight |
| 2 | **TOTP display/copy in UI+applet** | S | agent already serves `GetTotp` (fixed in Phase 2); pure UI work |
| 3 | **Password generator** | M | applet quick-gen + edit-form integration |
| 4 | **Fill-time domain re-check** (P1-8) | S | extension-only; blocks look-alike-tab mis-fill |
| 5 | **Folder/collection navigation** | M | data already synced; sidebar tree UI |
| 6 | **Second locale** (U4-6) | S | pipeline proof |
| 7 | **Attachments (view/download)** | M/L | needs API + crypto for attachment keys |
| 8 | **Autotype on Wayland** | L | research: virtual-keyboard protocol / portals; COSMIC support unclear |
| 9 | **Passkeys/FIDO2** | XL | webauthn UX + storage; track upstream Bitwarden client behaviour |

## Gate assessment

Systemd hardening landed and runtime-validated; integration metadata verified; UX backlog
ranked into the roadmap. **Phase 4 gate: PASS** — proceed to Phase 5 (testing & CI).
