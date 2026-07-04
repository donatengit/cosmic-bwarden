pub mod agent_client;
pub mod api;
pub mod base64;
pub mod cipherstring;
pub mod config;
pub mod db;
pub mod dirs;
pub mod error;
pub mod identity;
pub mod json;
pub mod locked;
mod perf;
pub mod protocol;
pub mod tests;
pub mod vault;

pub fn version() -> &'static str {
    env!("COSMIC_BWARDEN_VERSION")
}

/// IPC protocol version, independent of the build version. Bump ONLY on a
/// breaking change to the wire protocol (`protocol::Action`/`Response`
/// semantics or encoding). The build version embeds seconds-since-month-start
/// plus a git id, so comparing build versions declared every rebuild
/// "incompatible" (observed as E2E failures from stale-binary skew —
/// docs/review/00_ground_truth.md F9, decision in 07_packaging.md).
pub const PROTOCOL_VERSION: &str = "1";

/// Minimum length for a TPM-unlock PIN. Single source of truth for the agent
/// (authoritative validation), the UI (captions and submit validation), and
/// the CLI (prompt text). Short/empty PINs offer negligible protection: the
/// sealed blob is on disk, so brute force is bounded only by TPM
/// dictionary-attack lockout.
pub const MIN_PIN_LEN: usize = 6;
