pub mod agent_client;
pub mod api;
pub mod base64;
pub mod cipherstring;
pub mod config;
pub mod db;
pub mod dirs;
pub mod domain;
pub mod error;
pub mod generator_settings;
pub mod identity;
pub mod json;
pub mod locked;
mod perf;
pub mod protocol;
pub mod session_envelope;
#[cfg(test)]
mod tests;
pub mod vault;

pub fn version() -> &'static str {
    env!("COSMARDEN_VERSION")
}

/// Canonical project URL. Single source for every user-facing surface that
/// points people back at the source (CLI `--help`, the Settings footer, the
/// AppStream metainfo, packaging metadata) so a repo move needs one edit here
/// plus the non-Rust manifests, not a grep across the tree.
pub const HOMEPAGE: &str = "https://github.com/donatengit/cosmic-bwarden";

/// Trailing line for every binary's `--help`. A function rather than a `const`
/// because `concat!` only takes literals, and duplicating the URL into a second
/// const is exactly the drift [`HOMEPAGE`] exists to prevent.
pub fn help_footer() -> String {
    format!("Source, docs, and bug reports: {HOMEPAGE}")
}

/// IPC protocol version, independent of the build version. Bump ONLY on a
/// breaking change to the wire protocol (`protocol::Action`/`Response`
/// semantics or encoding). The build version embeds seconds-since-month-start
/// plus a git id, so comparing build versions declared every rebuild
/// "incompatible" (observed as E2E failures from stale-binary skew —
/// docs/review/00_ground_truth.md F9, decision in 07_packaging.md).
pub const PROTOCOL_VERSION: &str = "7";

/// Maximum postcard-framed IPC request or response body (bytes). Shared by
/// the agent accept loop and [`agent_client::AgentClient`] so a hostile
/// length prefix cannot force a multi-gigabyte allocation.
pub const MAX_IPC_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// Accept a framed IPC length prefix, or error without allocating the claimed size.
pub fn ipc_frame_len(claimed: u32) -> error::Result<usize> {
    let n = claimed as usize;
    if n > MAX_IPC_FRAME_BYTES {
        Err(error::Error::Other(format!(
            "IPC frame length {n} exceeds cap {MAX_IPC_FRAME_BYTES}"
        )))
    } else {
        Ok(n)
    }
}

/// Constant-time equality for secret byte slices (reprompt hashes, login-match
/// passwords). Length mismatch is `false`.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq as _;
    bool::from(a.ct_eq(b))
}

/// Minimum length for a TPM-unlock PIN. Single source of truth for the agent
/// (authoritative validation), the UI (captions and submit validation), and
/// the CLI (prompt text). Short/empty PINs offer negligible protection: the
/// sealed blob is on disk, so brute force is bounded only by TPM
/// dictionary-attack lockout.
pub const MIN_PIN_LEN: usize = 6;
