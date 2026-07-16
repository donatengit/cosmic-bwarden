//! Persistence for the password generator's "last used settings" — device-global
//! (not per-account), non-secret (just 4 booleans + a length), so it's a plain
//! JSON sibling of `config.rs`'s `load_legacy`/`save_legacy`, not folded into
//! `CosmicBWardenConfig` (which is account-shaped and unavailable pre-login).

use std::io::{Read as _, Write as _};

use crate::protocol::GeneratorSettings;

impl GeneratorSettings {
    /// Load the persisted settings, or `GeneratorSettings::default()` if none
    /// have been saved yet (first run).
    pub fn load() -> crate::error::Result<Self> {
        let file = crate::dirs::generator_settings_file();
        let mut fh = match std::fs::File::open(&file) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        let mut json = String::new();
        fh.read_to_string(&mut json)?;
        use crate::json::DeserializeJsonWithPath as _;
        json.json_with_path()
    }

    pub fn save(&self) -> crate::error::Result<()> {
        let file = crate::dirs::generator_settings_file();
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut fh = std::fs::File::create(&file)?;
        fh.write_all(
            serde_json::to_string(self)
                .map_err(|source| crate::error::Error::Other(source.to_string()))?
                .as_bytes(),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let s = GeneratorSettings::default();
        assert!(s.uppercase && s.lowercase && s.numbers && s.special);
        assert_eq!(s.length, 14);
    }
}
