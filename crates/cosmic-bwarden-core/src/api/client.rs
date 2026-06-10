use crate::error::{Error, Result};
use crate::json::{DeserializeJsonWithPathAsync as _};
use crate::api::models::*;

const BITWARDEN_CLIENT: &str = "cli";
const BITWARDEN_VERSION: &str = "2024.12.0";
const DEVICE_TYPE: u8 = 8;

pub struct Client {
    base_url: String,
    identity_url: String,
}

impl Client {
    pub fn new(base_url: &str, identity_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            identity_url: identity_url.to_string(),
        }
    }

    async fn reqwest_client(&self) -> Result<reqwest::Client> {
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

    fn identity_url(&self, path: &str) -> String {
        format!("{}{}", self.identity_url, path)
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    pub async fn prelogin(&self, email: &str) -> Result<(KdfType, u32, Option<u32>, Option<u32>)> {
        let prelogin = PreloginReq {
            email: email.to_string(),
        };
        let client = self.reqwest_client().await?;
        let res = client
            .post(self.identity_url("/accounts/prelogin"))
            .json(&prelogin)
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;
        let prelogin_res: PreloginRes = res.json_with_path().await?;
        Ok((
            prelogin_res.kdf,
            prelogin_res.kdf_iterations,
            prelogin_res.kdf_memory,
            prelogin_res.kdf_parallelism,
        ))
    }

    pub async fn register(
        &self,
        email: &str,
        name: &str,
        master_password_hash: &str,
        protected_key: &str,
        kdf: KdfType,
        kdf_iterations: u32,
    ) -> Result<()> {
        let req = serde_json::json!({
            "email": email,
            "name": name,
            "masterPasswordHash": master_password_hash,
            "masterPasswordHint": "",
            "key": protected_key,
            "kdf": kdf as u32,
            "kdfIterations": kdf_iterations,
        });
        let client = self.reqwest_client().await?;
        let res = client
            .post(self.identity_url("/accounts/register"))
            .json(&req)
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;
        
        let res = if res.status() == reqwest::StatusCode::NOT_FOUND {
            client
                .post(self.api_url("/accounts/register"))
                .json(&req)
                .send()
                .await
                .map_err(|source| Error::Reqwest { source })?
        } else {
            res
        };

        match res.status() {
            reqwest::StatusCode::OK | reqwest::StatusCode::NO_CONTENT => Ok(()),
            _ => Err(Error::RequestFailed {
                status: res.status().as_u16(),
            }),
        }
    }

    pub async fn login(
        &self,
        email: &str,
        device_id: &str,
        master_password_hash: &crate::locked::PasswordHash,
        two_factor_token: Option<&str>,
        two_factor_provider: Option<u32>,
        two_factor_code: Option<&str>,
        device_verification_code: Option<&str>,
    ) -> Result<(String, Option<String>, Option<String>)> {
        let mut req = ConnectTokenReq {
            grant_type: "password".to_string(),
            scope: "api offline_access".to_string(),
            client_id: "browser".to_string(),
            device_type: u32::from(DEVICE_TYPE),
            device_identifier: device_id.to_string(),
            device_name: "cosmic-bwarden".to_string(),
            device_push_token: String::new(),
            two_factor_token: two_factor_token.map(String::from),
            two_factor_provider,
            device_verification_code: device_verification_code.map(String::from),
            auth: ConnectTokenAuth::Password(ConnectTokenPassword {
                username: email.to_string(),
                password: base64::Engine::encode(
                    &base64::prelude::BASE64_STANDARD,
                    master_password_hash.hash(),
                ),
            }),
        };

        if let Some(code) = two_factor_code {
            match &mut req.auth {
                ConnectTokenAuth::Password(p) => {
                    p.password = format!("{}:{}", p.password, code);
                }
            }
        }

        let client = self.reqwest_client().await?;
        let res = client
            .post(self.identity_url("/connect/token"))
            .form(&req)
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;

        match res.status() {
            reqwest::StatusCode::OK => {
                let login_res: ConnectTokenRes = res.json_with_path().await?;
                Ok((
                    login_res.access_token,
                    login_res.refresh_token,
                    login_res.key,
                ))
            }
            reqwest::StatusCode::BAD_REQUEST => {
                let err: ConnectErrorRes = res.json_with_path().await?;
                if err.error == "invalid_grant" {
                    if let Some(token) = err.sso_email_2fa_session_token {
                        Err(Error::TwoFactorRequired {
                            providers: err
                                .two_factor_providers
                                .unwrap_or_default()
                                .into_iter()
                                .map(|p| p as u32)
                                .collect(),
                            token,
                        })
                    } else {
                        Err(Error::Other("Invalid credentials".to_string()))
                    }
                } else if err.error == "invalid_token"
                    && err.error_description.as_deref()
                        == Some("Device verification required.")
                {
                    Err(Error::NewDeviceVerificationRequired)
                } else {
                    Err(Error::Other(err.error_description.unwrap_or(err.error)))
                }
            }
            _ => Err(Error::RequestFailed {
                status: res.status().as_u16(),
            }),
        }
    }

    pub async fn exchange_refresh_token(
        &self,
        refresh_token: &str,
    ) -> Result<(String, Option<String>, Option<String>)> {
        let req = ConnectRefreshTokenReq {
            grant_type: "refresh_token".to_string(),
            refresh_token: refresh_token.to_string(),
            client_id: "browser".to_string(),
        };
        let client = self.reqwest_client().await?;
        let res = client
            .post(self.identity_url("/connect/token"))
            .form(&req)
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;

        match res.status() {
            reqwest::StatusCode::OK => {
                let login_res: ConnectTokenRes = res.json_with_path().await?;
                Ok((
                    login_res.access_token,
                    login_res.refresh_token,
                    login_res.key,
                ))
            }
            _ => Err(Error::RequestFailed {
                status: res.status().as_u16(),
            }),
        }
    }

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

    pub async fn add_cipher(
        &self,
        access_token: &str,
        ty: u32,
        name: &str,
        favorite: bool,
        username: Option<&str>,
        password: Option<&str>,
        notes: Option<&str>,
        fields: Option<Vec<CipherField>>,
    ) -> Result<()> {
        let login = if ty == 1 {
            Some(serde_json::json!({
                "username": username,
                "password": password,
                "totp": null,
                "uris": null,
            }))
        } else {
            None
        };

        let req = CiphersPostReq {
            ty,
            folder_id: None,
            favorite,
            name: name.to_string(),
            notes: notes.map(String::from),
            login,
            card: None,
            identity: None,
            secure_note: if ty == 2 {
                Some(CipherSecureNote {})
            } else {
                None
            },
            ssh_key: None,
            fields: fields.unwrap_or_default(),
            reprompt: CipherRepromptType::None,
        };

        let client = self.reqwest_client().await?;
        let res = client
            .post(self.api_url("/ciphers"))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&req)
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;

        match res.status() {
            reqwest::StatusCode::OK => Ok(()),
            reqwest::StatusCode::UNAUTHORIZED => Err(Error::RequestUnauthorized),
            _ => Err(Error::RequestFailed {
                status: res.status().as_u16(),
            }),
        }
    }

    pub async fn add_ssh_key(
        &self,
        access_token: &str,
        name: &str,
        favorite: bool,
        private_key: &str,
        public_key: Option<&str>,
        notes: Option<&str>,
        fields: Option<Vec<CipherField>>,
    ) -> Result<()> {
        let req = CiphersPostReq {
            ty: 5,
            folder_id: None,
            favorite,
            name: name.to_string(),
            notes: notes.map(String::from),
            login: None,
            card: None,
            identity: None,
            secure_note: None,
            ssh_key: Some(CipherSshKey {
                private_key: Some(private_key.to_string()),
                public_key: public_key.map(String::from),
                fingerprint: None,
            }),
            fields: fields.unwrap_or_default(),
            reprompt: CipherRepromptType::None,
        };

        let client = self.reqwest_client().await?;
        let res = client
            .post(self.api_url("/ciphers"))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&req)
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;

        match res.status() {
            reqwest::StatusCode::OK => Ok(()),
            reqwest::StatusCode::UNAUTHORIZED => Err(Error::RequestUnauthorized),
            _ => Err(Error::RequestFailed {
                status: res.status().as_u16(),
            }),
        }
    }

    pub async fn add_card(
        &self,
        access_token: &str,
        name: &str,
        favorite: bool,
        cardholder_name: Option<&str>,
        brand: Option<&str>,
        number: Option<&str>,
        exp_month: Option<&str>,
        exp_year: Option<&str>,
        code: Option<&str>,
        notes: Option<&str>,
        fields: Option<Vec<CipherField>>,
    ) -> Result<()> {
        let req = CiphersPostReq {
            ty: 3,
            folder_id: None,
            favorite,
            name: name.to_string(),
            notes: notes.map(String::from),
            login: None,
            card: Some(CipherCard {
                cardholder_name: cardholder_name.map(String::from),
                brand: brand.map(String::from),
                number: number.map(String::from),
                exp_month: exp_month.map(String::from),
                exp_year: exp_year.map(String::from),
                code: code.map(String::from),
            }),
            identity: None,
            secure_note: None,
            ssh_key: None,
            fields: fields.unwrap_or_default(),
            reprompt: CipherRepromptType::None,
        };

        let client = self.reqwest_client().await?;
        let res = client
            .post(self.api_url("/ciphers"))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&req)
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;

        match res.status() {
            reqwest::StatusCode::OK => Ok(()),
            _ => Err(Error::RequestFailed {
                status: res.status().as_u16(),
            }),
        }
    }

    pub async fn add_identity(
        &self,
        access_token: &str,
        name: &str,
        favorite: bool,
        first_name: Option<&str>,
        last_name: Option<&str>,
        address1: Option<&str>,
        city: Option<&str>,
        state: Option<&str>,
        postal_code: Option<&str>,
        country: Option<&str>,
        email: Option<&str>,
        phone: Option<&str>,
        notes: Option<&str>,
        fields: Option<Vec<CipherField>>,
    ) -> Result<()> {
        let req = CiphersPostReq {
            ty: 4,
            folder_id: None,
            favorite,
            name: name.to_string(),
            notes: notes.map(String::from),
            login: None,
            card: None,
            identity: Some(CipherIdentity {
                first_name: first_name.map(String::from),
                last_name: last_name.map(String::from),
                address1: address1.map(String::from),
                city: city.map(String::from),
                state: state.map(String::from),
                postal_code: postal_code.map(String::from),
                country: country.map(String::from),
                email: email.map(String::from),
                phone: phone.map(String::from),
                title: None,
                middle_name: None,
                address2: None,
                address3: None,
                ssn: None,
                license_number: None,
                passport_number: None,
                username: None,
            }),
            secure_note: None,
            ssh_key: None,
            fields: fields.unwrap_or_default(),
            reprompt: CipherRepromptType::None,
        };

        let client = self.reqwest_client().await?;
        let res = client
            .post(self.api_url("/ciphers"))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&req)
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;

        match res.status() {
            reqwest::StatusCode::OK => Ok(()),
            _ => Err(Error::RequestFailed {
                status: res.status().as_u16(),
            }),
        }
    }

    pub async fn update_cipher(
        &self,
        access_token: &str,
        id: &str,
        name: &str,
        favorite: bool,
        ty: u32,
        login: Option<serde_json::Value>,
        ssh_key: Option<CipherSshKey>,
        card: Option<CipherCard>,
        identity: Option<CipherIdentity>,
        notes: Option<&str>,
        reprompt: Option<CipherRepromptType>,
        fields: Option<Vec<CipherField>>,
    ) -> Result<()> {
        let req = CiphersPutReq {
            ty,
            folder_id: None,
            organization_id: None,
            favorite,
            name: name.to_string(),
            notes: notes.map(String::from),
            login,
            card,
            identity,
            fields: fields.unwrap_or_default(),
            secure_note: if ty == 2 {
                Some(CipherSecureNote {})
            } else {
                None
            },
            ssh_key,
            password_history: Vec::new(),
            reprompt: reprompt.unwrap_or(CipherRepromptType::None),
        };

        let client = self.reqwest_client().await?;
        let res = client
            .put(self.api_url(&format!("/ciphers/{id}")))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&req)
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;

        match res.status() {
            reqwest::StatusCode::OK => Ok(()),
            _ => Err(Error::RequestFailed {
                status: res.status().as_u16(),
            }),
        }
    }

    pub async fn update_favorite(
        &self,
        access_token: &str,
        id: &str,
        favorite: bool,
    ) -> Result<()> {
        let req = serde_json::json!({
            "Favorite": favorite,
        });

        let client = self.reqwest_client().await?;
        let res = client
            .put(self.api_url(&format!("/ciphers/{id}/favorite")))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&req)
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;

        match res.status() {
            reqwest::StatusCode::OK | reqwest::StatusCode::NO_CONTENT => Ok(()),
            _ => Err(Error::RequestFailed {
                status: res.status().as_u16(),
            }),
        }
    }

    pub async fn delete_cipher(&self, access_token: &str, id: &str) -> Result<()> {
        let client = self.reqwest_client().await?;
        let res = client
            .delete(self.api_url(&format!("/ciphers/{id}")))
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;

        match res.status() {
            reqwest::StatusCode::OK | reqwest::StatusCode::NO_CONTENT => Ok(()),
            _ => Err(Error::RequestFailed {
                status: res.status().as_u16(),
            }),
        }
    }
}
