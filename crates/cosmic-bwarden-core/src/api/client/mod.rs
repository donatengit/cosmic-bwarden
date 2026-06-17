pub mod auth;
pub mod ops;
pub mod vault;

use crate::error::{Error, Result};

pub(crate) const BITWARDEN_CLIENT: &str = "cli";
pub(crate) const BITWARDEN_VERSION: &str = "2024.12.0";
pub(crate) const DEVICE_TYPE: u8 = 8;

pub struct Client {
    pub(crate) base_url: String,
    pub(crate) identity_url: String,
}

impl Client {
    pub fn new(base_url: &str, identity_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            identity_url: identity_url.to_string(),
        }
    }

    pub(crate) async fn reqwest_client(&self) -> Result<reqwest::Client> {
        let mut default_headers = reqwest::header::HeaderMap::new();
        default_headers.insert(
            "Bitwarden-Client-Name",
            reqwest::header::HeaderValue::from_static(BITWARDEN_CLIENT),
        );
        default_headers.insert(
            "Bitwarden-Client-Version",
            reqwest::header::HeaderValue::from_static(BITWARDEN_VERSION),
        );
        default_headers.append(
            "Device-Type",
            reqwest::header::HeaderValue::from_str(&DEVICE_TYPE.to_string()).unwrap(),
        );
        let user_agent = format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        Ok(reqwest::Client::builder()
            .user_agent(user_agent)
            .default_headers(default_headers)
            .build()
            .map_err(|e| Error::CreateReqwestClient { source: e })?)
    }

    pub(crate) fn identity_url(&self, path: &str) -> String {
        format!("{}{}", self.identity_url, path)
    }

    pub(crate) fn api_url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}
