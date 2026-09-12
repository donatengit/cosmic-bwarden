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

/// `[P1-1]`: refuse any server URL whose transport would carry the
/// master-password hash and bearer tokens in cleartext. `https://` always
/// passes; `http://` passes only for loopback hosts (local Vaultwarden dev
/// instances); anything else — including a missing scheme — is an error.
fn ensure_transport_security(url_str: &str) -> Result<()> {
    let parsed = url::Url::parse(url_str).map_err(|source| Error::InvalidServerUrl {
        url: url_str.to_string(),
        source,
    })?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if parsed.host().is_some_and(host_is_loopback) => {
            // Once per process, not per request — this fires on every API call.
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                log::warn!(
                    "using cleartext http for loopback server {url_str} — \
                     acceptable only because traffic never leaves this machine"
                );
            });
            Ok(())
        }
        _ => Err(Error::InsecureServerUrl {
            url: url_str.to_string(),
        }),
    }
}

fn host_is_loopback(host: url::Host<&str>) -> bool {
    match host {
        // `.localhost` names are reserved loopback per RFC 6761.
        url::Host::Domain(d) => {
            d.eq_ignore_ascii_case("localhost") || d.to_ascii_lowercase().ends_with(".localhost")
        }
        url::Host::Ipv4(ip) => ip.is_loopback(),
        url::Host::Ipv6(ip) => ip.is_loopback(),
    }
}

impl Client {
    pub fn new(base_url: &str, identity_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            identity_url: identity_url.to_string(),
        }
    }

    pub(crate) async fn reqwest_client(&self) -> Result<reqwest::Client> {
        // Every request funnels through here, making this the single
        // transport-security enforcement point (`[P1-1]`) — no per-call-site
        // check to forget.
        ensure_transport_security(&self.base_url)?;
        ensure_transport_security(&self.identity_url)?;

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
        reqwest::Client::builder()
            .user_agent(user_agent)
            .default_headers(default_headers)
            .build()
            .map_err(|e| Error::CreateReqwestClient { source: e })
    }

    pub(crate) fn identity_url(&self, path: &str) -> String {
        format!("{}{}", self.identity_url, path)
    }

    pub(crate) fn api_url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Consume an unsuccessful HTTP response into `Error::RequestFailed`,
    /// logging method, URL, status, and (truncated) body at `error` level.
    /// Every non-2xx API response must go through here — the "no silent
    /// failures" invariant: operators must see failed server calls in the
    /// journal. Server error bodies are diagnostic messages, never secrets.
    pub(crate) async fn request_failed(method: &str, res: reqwest::Response) -> Error {
        let status = res.status().as_u16();
        let url = res.url().clone();
        let body: String = res
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(300)
            .collect();
        log::error!("API request failed: {method} {url} -> HTTP {status}: {body}");
        Error::RequestFailed { status }
    }
}

#[cfg(test)]
mod transport_security_tests {
    use super::ensure_transport_security;
    use crate::error::Error;

    #[test]
    fn https_is_always_allowed() {
        assert!(ensure_transport_security("https://vault.example.com/api").is_ok());
        assert!(ensure_transport_security("https://api.bitwarden.com").is_ok());
    }

    #[test]
    fn http_is_allowed_for_loopback_hosts_only() {
        assert!(ensure_transport_security("http://localhost:8080/api").is_ok());
        assert!(ensure_transport_security("http://LOCALHOST/api").is_ok());
        assert!(ensure_transport_security("http://vaultwarden.localhost/api").is_ok());
        assert!(ensure_transport_security("http://127.0.0.1:8080").is_ok());
        // The whole 127.0.0.0/8 block is loopback, not just .1.
        assert!(ensure_transport_security("http://127.5.4.3").is_ok());
        assert!(ensure_transport_security("http://[::1]:8080").is_ok());
    }

    #[test]
    fn http_to_a_public_host_is_refused() {
        for url in [
            "http://vault.example.com/api",
            "http://192.168.1.10:8080", // LAN is still a network — not exempt
            "http://notlocalhost.example.com",
            "http://localhost.example.com", // suffix-matching must not be fooled
        ] {
            assert!(
                matches!(
                    ensure_transport_security(url),
                    Err(Error::InsecureServerUrl { .. })
                ),
                "{url} must be refused"
            );
        }
    }

    #[test]
    fn unparseable_or_schemeless_urls_are_refused() {
        // No scheme: url::Url::parse has no base to resolve against.
        assert!(matches!(
            ensure_transport_security("vault.example.com"),
            Err(Error::InvalidServerUrl { .. })
        ));
    }

    #[test]
    fn non_http_schemes_are_refused() {
        assert!(matches!(
            ensure_transport_security("ftp://vault.example.com"),
            Err(Error::InsecureServerUrl { .. })
        ));
    }
}
