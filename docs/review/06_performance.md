# Phase 6 — Performance & Footprint

Measured: 2026-07-04 on the dev machine (CachyOS, `-C target-cpu=native`, release
profile as shipped: `opt-level="z"`, fat LTO, `codegen-units=1`). Method: `#[ignore]`d
measurement tests in `core/src/perf.rs` (rerunnable — command in the file header),
`/proc` sampling for the daemon, `cargo bloat` for size.

## Numbers

| Metric | Value | Budget / verdict |
|---|---|---|
| KDF, PBKDF2-SHA256 600k (Vaultwarden default) | **87.6 ms** median | ≤1 s unlock budget → 11× headroom |
| KDF, Argon2id t=3 m=64 MiB p=4 (Bitwarden default) | **130.7 ms** median | ≤1 s → 7× headroom |
| Decrypt 5 000 entries (name+username, sidebar hot path) | **7.7 ms** total (1.5 µs/entry) | instant; cache rebuild is a non-event |
| Search pass over 5 000 decrypted entries | **0.32 ms** | instant |
| Agent idle RSS (release) | **5.7–5.9 MB** | excellent for a 24/7 daemon |
| Agent wakeups over 20 s idle | **0** voluntary ctx switches | truly idle (autolock polls 5-min) |
| Agent binary (release, stripped) | **4.8 MB** (CLI 1.0 MB) | fine |
| Agent time-to-socket-ready | < 2 s (log-timestamped, effectively immediate) | fine |

`.text` breakdown (cargo bloat, top): agent code 541 KB, std 530 KB, zbus 494 KB
(already analysed and justified in `logind.rs`'s comment), rustls+ring+webpki ≈ 504 KB,
reqwest 161 KB, core 152 KB, clap 129 KB. No anomalies.

## Decisions from the numbers

1. **The `-Oz` vs `-O3` crypto question is moot.** The review plan asked whether
   size-optimized crypto hurts unlock latency. At 88–131 ms — 7–11× inside the 1 s
   budget — even a hypothetical 3× win from per-crate `opt-level=3` overrides would be
   imperceptible. Keeping `opt-level="z"`; not adding override complexity for an
   invisible gain.
2. **A3-1 (KDF blocks the current-thread runtime) downgraded to "no action".** The
   Phase 3 concern was a multi-second stall; measured worst case is ~131 ms once per
   unlock/reprompt. An SSH-agent request queued behind that is fine. `spawn_blocking`
   remains a nicety if the runtime ever goes multi-thread for other reasons; removed as
   a standing roadmap task (noted inline there).
3. **Decryption-cache strategy validated.** The "instant search in huge vaults" claim in
   CONTEXT.md holds: full 5 k rebuild costs 7.7 ms, so rebuilding on every mutation (the
   current design) is entirely reasonable; no incremental-update machinery warranted.
   Cache memory at 5 k entries is well under 1 MB; cleared on lock (F2-5's
   zeroize note stands, severity unchanged).
4. **Idle footprint needs no work.** ~6 MB RSS / zero wakeups is better than most
   session daemons; nothing to do.

## Not measured here (needs interactive COSMIC session)

- Cold-start to *usable popup* (applet paint + first GetSidebarEntries round-trip).
  Expected dominated by the panel's applet spawn, not our code paths (all agent-side
  costs above are ms-scale). → Phase 4's manual-visual-pass list.

## Gate assessment

All numbers recorded and comfortably inside budgets; two standing concerns (A3-1, -Oz)
closed with data rather than code. **Phase 6 gate: PASS** — proceed to Phase 7
(packaging & distribution).
