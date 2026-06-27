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
