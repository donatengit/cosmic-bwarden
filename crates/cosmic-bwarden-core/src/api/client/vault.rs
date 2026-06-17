use crate::error::{Error, Result};
use crate::json::{DeserializeJsonWithPathAsync as _};
use crate::api::models::*;
use crate::api::client::Client;

impl Client {
    pub async fn sync(
        &self,
        access_token: &str,
    ) -> Result<(String, Option<String>, std::collections::HashMap<String, String>, Vec<crate::db::Entry>)> {
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
                Ok((
                    sync_res.profile.key,
                    sync_res.profile.protected_private_key,
                    org_keys,
                    ciphers,
                ))
            }
            reqwest::StatusCode::UNAUTHORIZED => Err(Error::RequestUnauthorized),
            _ => Err(Error::RequestFailed {
                status: res.status().as_u16(),
            }),
        }
    }
}
