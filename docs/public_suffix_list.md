# Domain Matching and the Public Suffix List

How cosmic-bwarden decides whether a vault entry belongs to the web page the
browser is on — the extension popup's suggestions, the toolbar badge count,
and the save/update prompt (`CheckLoginMatch`) all use the same rules.

## Why naive approaches fail

Two obvious strategies were tried and rejected:

- **Strip the host to its last two labels** (`account.facebook.com` →
  `facebook.com`) and search with the remainder. Multi-label public suffixes
  break this: `victim.co.uk` collapses to `co.uk`, and a substring search then
  surfaces every unrelated `.co.uk` entry — flagged in the 2026-07 security
  review. The suggestion list is an anti-phishing signal ("my password manager
  doesn't recognize this site"); over-broad results erode exactly that.
- **Match the full host exactly.** Safe, but visiting `account.facebook.com`
  showed nothing for an entry saved as `facebook.com` — unusable without
  autofill.

Progressive label-stripping ("try exact, drop the leftmost label, repeat,
stop at the first hit") was also considered and rejected: its no-match
fallback step fires precisely in the phishing scenario (unknown page, walk to
the bare suffix, suggest the user's real bank), and behavior becomes
dependent on vault contents.

## The rules

`cosmic_bwarden_core::domain::hosts_match(a, b)` — applied between the tab's
full host and each host stored on an entry (URI hosts; hostname-shaped name
as fallback for legacy URI-less entries). In order:

1. **Exact equality.**
2. **Label-boundary subdomain, both directions** — one host ends with
   `.` + the other. The boundary dot means `notfacebook.com` never matches
   `facebook.com`. Directions: page `account.facebook.com` matches stored
   `facebook.com` (popup suggestions); stored `login.facebook.com` matches
   page `facebook.com` (save-prompt "update, don't duplicate").
3. **Same registrable domain (eTLD+1)** — only with the `public_suffix_list`
   feature. Catches sibling subdomains: stored `accounts.google.com` matches
   page `mail.google.com` because both reduce to `google.com`.

Rules 1–2 need no data and cannot cross a public-suffix boundary: nothing is
ever *derived* from the page host, it is only compared against hosts the user
deliberately stored. `evil.co.uk` vs `mybank.co.uk` fails both directions of
rule 2. The only way to opt into promiscuous matching is storing a URI that
*is* a bare suffix (`co.uk`) — self-inflicted, and identical to Bitwarden's
exposure.

Rule 3 is the only place a registrable domain is computed, and the PSL knows
`co.uk` is a suffix (`psl::domain_str("evil.co.uk")` = `evil.co.uk`, not
`co.uk`), so the collapse that broke the naive stripper can't happen.

**Special hosts fail closed**: IPs, `localhost`, and dotless intranet names
match exactly only (no hierarchy, no eTLD+1).

**`match_type` handling**: URIs with Bitwarden match type `Never` are
excluded when the host cache is built; all other match types are treated as
domain matching (v1 simplification, same as `CheckLoginMatch`).

## The `public_suffix_list` feature

- **Crates**: declared in `cosmic-bwarden-core` (`dep:psl`), forwarded by
  `cosmic-bwarden-agent` and `cosmic-bwarden-ui`. **Enabled by default.**
- **What the `psl` crate does**: a codegen step converts Mozilla's Public
  Suffix List into a ~2.5 MB generated Rust match-tree compiled into `.text`.
  No heap allocation, no startup parsing, no runtime fetch (a runtime fetch
  would be its own attack surface). Measured cost: ~+790 KB binary,
  ≤~650 KB RSS worst case (read-only mmapped pages, demand-paged), ~10 s
  one-time compile of the dependency.
- **Without the feature** (`--no-default-features`): rule 3 is skipped —
  `registrable_domain()` returns `None`, callers treat that as "rule
  unavailable", never as a match. Everything degrades gracefully: exact and
  boundary-subdomain matching still work; only sibling-subdomain matching is
  lost. The applet's row label (`extract_domain_label`) likewise falls back
  to the full host — never the old last-two-labels cut that displayed
  `example.co.uk` as `co.uk`.
- **List freshness**: the PSL is frozen at the built `psl` crate version.
  A stale list can only make rule 3 slightly over-broad for *new* public
  suffixes (never breaks rules 1–2). Routine `cargo update` covers this.

## Where matching runs (performance)

Matching is agent-side only; the extension sends the tab's full host
(lowercased, `www.` stripped — see `extractDomain` in `popup.js` /
`background.js`) in the `domain` field of `GetSidebarEntries` and never
collapses labels itself.

`domain` is a separate field from `query` deliberately: `query` is a
free-text substring search over names/usernames, `domain` is host matching
with security semantics. Overloading `query` was the original bug — the agent
can't distinguish "user typed co.uk" from "tab is on victim.co.uk". A set
`query` wins over `domain` (typed search overrides the tab filter).

Per-entry hosts are decrypted once per unlock/sync into the agent's sidebar
cache (`CachedSidebarEntry.hosts`), not per keystroke; a query is then a
linear scan of string comparisons — ~50 µs for 5k entries. The plaintext
hosts sit in unlocked-agent memory alongside the plaintext names/usernames
already cached there (same sensitivity class; noted in `CONTEXT.md`).

## Verification

- Rule unit tests (both feature states): `cargo test -p cosmic-bwarden-core
  --features public_suffix_list domain::` and without the flag.
- Agent-path E2E (encrypted URIs → host cache → filter):
  `cargo test -p cosmic-bwarden-tests -- vault::domain_matching --test-threads=1`.
- Extension flow: `just test-extension-e2e` (`popup.spec.js`,
  "domain-based filtering").
