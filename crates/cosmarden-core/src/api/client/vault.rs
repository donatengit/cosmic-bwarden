use crate::api::client::Client;
use crate::api::models::*;
use crate::error::{Error, Result};
use crate::json::DeserializeJsonWithPathAsync as _;

/// The parts of a sync response the agent persists or caches. `user_id` is the
/// account's UUID, cached because Vaultwarden 1.37+ requires it as
/// `encryptedFor` on every cipher create/update.
pub struct SyncResult {
    pub key: String,
    pub protected_private_key: Option<String>,
    pub org_keys: std::collections::HashMap<String, String>,
    pub user_id: String,
    pub entries: Vec<crate::db::Entry>,
}

impl Client {
    pub async fn sync(&self, access_token: &str) -> Result<SyncResult> {
        let client = self.reqwest_client().await?;
        let res = client
            .get(self.api_url("/sync"))
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;

        match res.status() {
            reqwest::StatusCode::OK => {
                let sync_res: SyncRes = res.json_with_path().await?;
                let ciphers = sync_res
                    .ciphers
                    .iter()
                    .filter_map(|c| c.to_entry(&sync_res.folders))
                    .collect();
                let org_keys = sync_res
                    .profile
                    .organizations
                    .iter()
                    .map(|org| (org.id.clone(), org.key.clone()))
                    .collect();
                Ok(SyncResult {
                    key: sync_res.profile.key,
                    protected_private_key: sync_res.profile.protected_private_key,
                    org_keys,
                    user_id: sync_res.profile.id,
                    entries: ciphers,
                })
            }
            reqwest::StatusCode::UNAUTHORIZED => Err(Error::RequestUnauthorized),
            _ => Err(Self::request_failed("GET", res).await),
        }
    }
}
