# Phase 8 — Documentation & First-Run Experience

Reviewed: 2026-07-04. Final phase of the review plan
(`docs/fable_5_project_review_plan.md`).

## Verdict

The doc tree was excellent for AI agents and thin for humans; that gap is now closed
with a root `README.md`. Licensing landed (owner decision: **GPL-3.0-only**). The
first-run flow is documented at 3 steps post-install; making it zero-terminal is a
packaging outcome (Phase 7 blockers), not a docs problem. **Gate: PASS — review
complete.**

## Done this phase

- **`README.md`** (was: none) — human-facing: pitch, accurate feature list, install +
  first-run (3 steps after `just user-install`), an *honest* security-model summary
  (including the same-UID trust boundary and the swap caveat — trust is won by stating
  limits, not hiding them), development pointers, status, license. States
  non-affiliation with Bitwarden Inc. and System76.
- **License** — canonical GPL-3.0 text at `LICENSE` (verified against the FSF sha256),
  `license = "GPL-3.0-only"` in all five crates (flows into `cargo deb` copyright),
  PKGBUILD `license=(GPL-3.0-only)`. Roadmap blocker closed.
- **`docs/summary.md`** — fixed a stale claim (applet described as "top-5 most
  frequently used"; actual behaviour is favourites-first search capped at 10).
- Doc-tree consolidation was largely done in Phase 0 (archive split, typo rename);
  spot-check this phase found the AGENTS.md document index accurate.

## First-run flow (as documented)

`just user-install && just enable-agent && just restart-panel`, then: add applet in
COSMIC Settings → open vault window from the applet → log in. Three GUI steps, one
credential entry, no terminal after install. **Terminal-free install** requires the
published `.deb`/AUR artifacts (Phase 7 pre-publish blockers) — by design the next
milestone, not additional docs.

Manual follow-up (needs a live session, same list as Phase 4): walk the flow on a
fresh profile and screenshot for the README (`docs/` has no screenshots yet — worth
adding once the UI is visually final).

## Review-wide loose ends (owner decisions, not review work)

1. **Reference submodules** (Phase 0 F1, ~280 MB): review complete, so the stated
   precondition for removal is met. Removal stays the recommendation for clone/CI
   ergonomics — but they are the owner's working reference material, so this is left
   as a deliberate owner choice, not applied unilaterally. `docs/references.md` with
   pinned URLs is the replacement if removed.
2. **First push to a forge** — activates CI (Phase 5), release automation and
   PKGBUILD URL (Phase 7), and the history-scrub item already in the roadmap.
3. **App-ID rename** before the first published artifact (Phase 4 U4-1).

## Gate assessment

README shippable, license resolved, doc tree consistent. **Phase 8 gate: PASS.**

---

## Review closing summary (Phases 0–8)

| Phase | Gate | Headline |
|---|---|---|
| 0 Ground truth | PASS | reproducible build; hygiene fixed; flake baseline |
| 1 Security | PASS | no S0/S1; 14 prior fixes verified real; 2 hardening fixes |
| 2 Data integrity | PASS | **2 S1 bugs fixed**: dead token refresh; wrong TOTP derivation |
| 3 Architecture | PASS | mlock panic fixed; clippy adopted workspace-wide |
| 4 COSMIC UX | PASS | systemd unit hardened (runtime-validated); UX backlog ranked |
| 5 Testing & CI | PASS | flake roots fixed; invariant tests added; CI authored |
| 6 Performance | PASS | all budgets met 7–11×; two concerns closed with data |
| 7 Packaging | PASS | protocol version decoupled; .deb built; Flatpak ruled out |
| 8 Docs | PASS | README + GPL-3.0; first-run at 3 steps |

Standing work lives in `docs/roadmap.md` (pre-publish blockers, security hardening
backlog, UX backlog). The repo is in materially better shape than at `c037f37`:
every claim above is backed by a committed test, measurement, or artifact.
