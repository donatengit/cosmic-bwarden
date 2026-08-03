# Phase 7 — Packaging, Distribution & Release

Reviewed: 2026-07-04. Scope: versioning/protocol decision (implemented), Flatpak
feasibility analysis, installable artifacts (.deb built; PKGBUILD skeleton), release
automation, extension-store requirements, licensing.

## Verdict

The architecture (host agent + panel applet + systemd user unit + browser native host)
is **native-package territory**; Flatpak is confirmed unsuitable as the primary channel
(analysis below — this resolves the risk flagged in the review plan). A real `.deb` is
produced and structure-verified; AUR skeleton and tag-driven release workflow are in
place. Two hard pre-publish blockers remain: **no LICENSE** and **no public remote**.
**Gate: PASS** (artifact produced), with those blockers owned by the roadmap.

## Versioning & protocol — decided and implemented

- **Problem**: the build version (`YYYY.MM-<seconds-into-month>-<git>`) was also the
  protocol version, and compatibility was string equality — every rebuild was
  "incompatible" (bit us live in Phase 0's E2E baseline).
- **Implemented now**: `core::PROTOCOL_VERSION` ("1") — an independent constant bumped
  only on breaking wire changes. Agent reports it in `Response::Version`; the CLI
  compares constants, not build strings. E2E `version` tests updated and green; the
  stale-binary-skew failure class is structurally dead.
- **Decided (for first release)**: release tags are calendar-based **`vYYYY.MM.PATCH`**
  (fits the existing calendar build string; PATCH for same-month respins). The
  seconds-based build version remains a build-info string only — shown in `--version`
  and the UI, never compared. Package versions derive from the tag, not the build
  string, so package managers get monotonic, comparable versions.

## Flatpak analysis (plan's early-risk item — resolved: not the primary channel)

Three independent blockers, each architectural rather than fixable-with-effort:

1. **Panel applet**: the COSMIC panel spawns applet binaries directly from desktop
   entries; there is no convention for panel-embedding a Flatpak'd applet. Applets are
   host packages across today's COSMIC ecosystem.
2. **Native messaging host**: the browser launches the host binary named in a JSON
   manifest under the *host* filesystem (`~/.mozilla/native-messaging-hosts`). A
   Flatpak'd cosmic-bwarden cannot install that manifest or be exec'd by a host
   browser without a host-side `flatpak run` shim — at which point the "sandbox" is
   ceremony. (The mirror problem — *Flatpak'd browsers* reaching our *host* native
   host — is the user's browser's concern: Firefox ≥127 flatpak supports the
   WebExtensions portal; Chromium flatpaks mostly don't. Worth a docs note for users.)
3. **systemd user service**: Flatpaks can't install user units; the agent is the
   security core and must run as a proper service.

**Decision**: native packages are the distribution channel — `.deb` (Pop!_OS/Ubuntu
COSMIC), AUR (Arch/CachyOS), COPR later (Fedora COSMIC spin). No Flatpak manifest will
be maintained; revisit only if COSMIC grows a portal-based applet story.

## Artifacts produced this phase

| Artifact | Status |
|---|---|
| `cargo deb -p cosmic-bwarden-ui --no-build` → `target/debian/cosmic-bwarden_*.deb` | **built & content-verified** (3 binaries, desktop entry, applet .ron, hardened user unit, SVG icons). `depends` is a conservative manual list — `$auto` needs dpkg tooling; finalize with `dpkg-shlibdeps` on a Debian host before publishing. |
| `packaging/PKGBUILD` | skeleton; blocked on remote URL + license, then `makepkg` in a clean chroot |
| `.github/workflows/release.yml` | tag `v*` → build .deb + extension zip → draft GitHub release |
| `packaging/cosmic-bwarden-agent.deb.service`, `packaging/com.system76.CosmicBWarden.ron` | static copies for packages (justfile keeps rendering its own; noted for later dedupe) |

## Extension shipping notes (AMO / Chrome Web Store)

- ID `cosmic-bwarden@enikeev.com` is pinned in the native-host manifest; AMO signing keys
  that ID on first submission — submit before advertising, since squatters can't be
  evicted.
- Firefox: the known MV3 `service_worker` limitation (test-suite notes) means AMO
  submission should use `background.scripts` (event page) in a Firefox-flavoured
  manifest; Chrome keeps `service_worker`. Two-manifest build in `pack-extension` when
  store submission becomes real.
- Both stores require a privacy policy and (AMO) source-review notes for the native
  messaging permission. Cheap to write; do it with Phase 8 docs.

## Blockers → roadmap (pre-publish section)

1. **No LICENSE file / no `license` fields anywhere.** Legally "all rights reserved" —
   blocks AUR (`license=()`), deb copyright, AMO review, and any contribution. Owner
   decision needed (GPL-3.0 fits the COSMIC ecosystem norm).
2. **No public remote** — blocks PKGBUILD source URL, CI activation (Phase 5), release
   workflow, store links.
3. App-ID rename (`com.system76.*` → own namespace, Phase 4 U4-1) should land **before**
   the first published artifact so no shipped package ever carries the wrong ID.

## Gate assessment

Installable artifact exists and is verified; versioning implemented; Flatpak question
closed with a documented decision. **Phase 7 gate: PASS** — proceed to Phase 8
(documentation & first-run experience).
