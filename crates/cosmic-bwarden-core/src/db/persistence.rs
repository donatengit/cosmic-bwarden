use std::collections::HashMap;
use std::io::{Read as _, Write as _};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use crate::api;
use crate::error::{Error, Result};
use crate::db::models::*;

#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
pub struct Db {
    #[serde(skip)]
    pub access_token: Option<Secret>,
    #[serde(skip)]
    pub refresh_token: Option<Secret>,

    pub kdf: Option<api::KdfType>,
    pub iterations: Option<u32>,
    pub memory: Option<u32>,
    pub parallelism: Option<u32>,
    pub protected_key: Option<Secret>,
    pub protected_private_key: Option<Secret>,
    pub protected_org_keys: HashMap<String, Secret>,

    pub entries: Vec<Entry>,
}

impl Db {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(server: &str, email: &str) -> Result<Self> {
        let file = crate::dirs::db_file(server, email);
        let mut fh = std::fs::File::open(&file)?;
        let mut json = String::new();
        fh.read_to_string(&mut json)?;
        use crate::json::DeserializeJsonWithPath as _;
        let slf: Self = json.json_with_path()?;
        Ok(slf)
    }

    pub async fn load_async(server: &str, email: &str) -> Result<Self> {
        let file = crate::dirs::db_file(server, email);
        let mut fh = tokio::fs::File::open(&file).await?;
        let mut json = String::new();
        fh.read_to_string(&mut json).await?;
        use crate::json::DeserializeJsonWithPath as _;
        let slf: Self = json.json_with_path()?;
        Ok(slf)
    }

    pub fn save(&self, server: &str, email: &str) -> Result<()> {
        let file = crate::dirs::db_file(server, email);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut fh = std::fs::File::create(&file)?;
        fh.write_all(
            serde_json::to_string(self)
                .map_err(|source| Error::Other(source.to_string()))?
                .as_bytes(),
        )?;
        Ok(())
    }

    pub async fn save_async(&self, server: &str, email: &str) -> Result<()> {
        let file = crate::dirs::db_file(server, email);
        if let Some(parent) = file.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut fh = tokio::fs::File::create(&file).await?;
        fh.write_all(
            serde_json::to_string(self)
                .map_err(|source| Error::Other(source.to_string()))?
                .as_bytes(),
        )
        .await?;
        Ok(())
    }

    pub fn remove(server: &str, email: &str) -> Result<()> {
        let file = crate::dirs::db_file(server, email);
        let res = std::fs::remove_file(&file);
        if let Err(e) = &res {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Ok(());
            }
        }
        res.map_err(Into::into)
    }

    pub fn has_account(&self) -> bool {
        self.iterations.is_some() && self.kdf.is_some() && self.protected_key.is_some()
    }

    pub fn needs_login(&self) -> bool {
        !self.has_account()
            || self.access_token.is_none()
            || self.refresh_token.is_none()
    }
}
