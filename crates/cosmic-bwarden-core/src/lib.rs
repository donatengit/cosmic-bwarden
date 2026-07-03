pub mod api;
pub mod agent_client;
pub mod base64;
pub mod cipherstring;
pub mod config;
pub mod db;
pub mod dirs;
pub mod error;
pub mod identity;
pub mod json;
pub mod locked;
pub mod protocol;
pub mod vault;
pub mod tests;

pub fn version() -> &'static str {
    env!("COSMIC_BWARDEN_VERSION")
}

/// Minimum length for a TPM-unlock PIN. Single source of truth for the agent
/// (authoritative validation), the UI (captions and submit validation), and
/// the CLI (prompt text). Short/empty PINs offer negligible protection: the
/// sealed blob is on disk, so brute force is bounded only by TPM
/// dictionary-attack lockout.
pub const MIN_PIN_LEN: usize = 6;
