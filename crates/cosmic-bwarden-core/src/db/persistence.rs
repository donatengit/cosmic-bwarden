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
        use std::os::unix::fs::OpenOptionsExt as _;
        let file = crate::dirs::db_file(server, email);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string(self)
            .map_err(|source| Error::Other(source.to_string()))?;
        // Write to a temp file with 0600 perms, then rename over the target so a
        // crash mid-write can't corrupt or truncate the vault cache (which holds
        // `protected_key`). The rename is atomic within the same directory.
        let tmp = file.with_extension("json.tmp");
        {
            let mut fh = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)?;
            fh.write_all(json.as_bytes())?;
            fh.sync_all()?;
        }
        std::fs::rename(&tmp, &file)?;
        Ok(())
    }

    pub async fn save_async(&self, server: &str, email: &str) -> Result<()> {
        let file = crate::dirs::db_file(server, email);
        if let Some(parent) = file.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_string(self)
            .map_err(|source| Error::Other(source.to_string()))?;
        // Temp-file + rename with 0600 perms (see `save`).
        let tmp = file.with_extension("json.tmp");
        {
            let mut fh = tokio::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)
                .await?;
            fh.write_all(json.as_bytes()).await?;
            fh.sync_all().await?;
        }
        tokio::fs::rename(&tmp, &file).await?;
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

    /// Whether a full login (master password + server auth) is required.
    /// Deliberately independent of session-token presence: tokens are zeroized
    /// on lock and lost on agent restart, but unlock silently re-authenticates
    /// with the derived master-password hash — so as long as account material
    /// exists, the user only needs to unlock, never re-login.
    pub fn needs_login(&self) -> bool {
        !self.has_account()
    }
}
