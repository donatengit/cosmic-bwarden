use std::io::{Read as _, Write as _};
use tokio::io::AsyncReadExt as _;
use cosmic_config::{CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};

pub const CONFIG_ID: &str = "com.system76.CosmicBWarden";

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, CosmicConfigEntry)]
#[version = 1]
pub struct CosmicBWardenConfig {
    pub email: Option<String>,
    pub base_url: Option<String>,
    pub identity_url: Option<String>,
    pub ui_url: Option<String>,
    pub device_id: Option<String>,
    #[serde(default = "default_lock_timeout")]
    pub lock_timeout: u64,
    #[serde(default = "default_top_popular_count")]
    pub top_popular_count: u32,
    #[serde(default = "default_top_popular_days")]
    pub top_popular_days: u32,
    #[serde(default)]
    pub persist_session: bool,
}

impl Default for CosmicBWardenConfig {
    fn default() -> Self {
        Self {
            email: None,
            base_url: None,
            identity_url: None,
            ui_url: None,
            device_id: None,
            lock_timeout: default_lock_timeout(),
            top_popular_count: default_top_popular_count(),
            top_popular_days: default_top_popular_days(),
            persist_session: false,
        }
    }
}

pub fn default_lock_timeout() -> u64 {
    3600
}

pub fn default_top_popular_count() -> u32 {
    5
}

pub fn default_top_popular_days() -> u32 {
    7
}

impl CosmicBWardenConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load config from the legacy JSON file or return default.
    /// This is for migration purposes or standalone CLI usage.
    pub fn load_legacy() -> crate::error::Result<Self> {
        let file = crate::dirs::config_file();
        let mut fh = match std::fs::File::open(&file) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        let mut json = String::new();
        fh.read_to_string(&mut json)?;
        use crate::json::DeserializeJsonWithPath as _;
        let mut slf: Self = json.json_with_path()?;
        if slf.lock_timeout == 0 {
            slf.lock_timeout = default_lock_timeout();
        }
        Ok(slf)
    }

    pub async fn load_legacy_async() -> crate::error::Result<Self> {
        let file = crate::dirs::config_file();
        let mut fh = match tokio::fs::File::open(&file).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        let mut json = String::new();
        fh.read_to_string(&mut json).await?;
        use crate::json::DeserializeJsonWithPath as _;
        let mut slf: Self = json.json_with_path()?;
        if slf.lock_timeout == 0 {
            slf.lock_timeout = default_lock_timeout();
        }
        Ok(slf)
    }

    pub fn save_legacy(&self) -> crate::error::Result<()> {
        let file = crate::dirs::config_file();
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

    pub fn base_url(&self) -> String {
        self.base_url.as_ref().filter(|s| !s.is_empty()).map_or_else(
            || "https://api.bitwarden.com".to_string(),
            |url| {
                let clean_url = url.trim_end_matches('/');
                if clean_url == "https://api.bitwarden.eu" {
                    "https://api.bitwarden.eu".to_string()
                } else {
                    format!("{clean_url}/api")
                }
            },
        )
    }

    pub fn identity_url(&self) -> String {
        self.identity_url.as_ref().filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| {
            self.base_url.as_ref().filter(|s| !s.is_empty()).map_or_else(
                || "https://identity.bitwarden.com".to_string(),
                |url| {
                    let clean_url = url.trim_end_matches('/');
                    if clean_url == "https://api.bitwarden.eu" {
                        "https://identity.bitwarden.eu".to_string()
                    } else {
                        format!("{clean_url}/identity")
                    }
                },
            )
        })
    }

    pub fn ui_url(&self) -> String {
        self.ui_url.as_ref().filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| {
            self.base_url.as_ref().filter(|s| !s.is_empty()).map_or_else(
                || "https://vault.bitwarden.com".to_string(),
                |url| {
                    let clean_url = url.trim_end_matches('/');
                    if clean_url == "https://api.bitwarden.eu" {
                        "https://vault.bitwarden.eu".to_string()
                    } else {
                        clean_url.to_string()
                    }
                },
            )
        })
    }

    pub async fn device_id(&self) -> crate::error::Result<String> {
        let file = crate::dirs::device_id_file();
        if let Ok(mut fh) = tokio::fs::File::open(&file).await {
            let mut s = String::new();
            fh.read_to_string(&mut s).await?;
            Ok(s.trim().to_string())
        } else {
            let id = self.device_id.as_ref().map_or_else(
                || uuid::Uuid::new_v4().hyphenated().to_string(),
                |s| s.clone(),
            );
            if let Some(parent) = file.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let mut fh = tokio::fs::File::create(&file).await?;
            tokio::io::AsyncWriteExt::write_all(&mut fh, id.as_bytes()).await?;
            Ok(id)
        }
    }

    pub fn server_name(&self) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| "default".to_string())
    }
}
