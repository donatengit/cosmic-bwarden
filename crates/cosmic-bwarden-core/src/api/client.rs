use crate::error::{Error, Result};
use crate::json::{DeserializeJsonWithPath as _, DeserializeJsonWithPathAsync as _};
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
            // Fallback to /api/accounts/register
            client
                .post(self.api_url("/accounts/register"))
                .json(&req)
                .send()
                .await
                .map_err(|source| Error::Reqwest { source })?
        } else {
            res
        };

        if res.status() == reqwest::StatusCode::OK
            || res.status() == reqwest::StatusCode::NO_CONTENT
        {
            Ok(())
        } else {
            Err(Error::RequestFailed {
                status: res.status().as_u16(),
            })
        }
    }

    pub async fn login(
        &self,
        email: &str,
        device_id: &str,
        password_hash: &crate::locked::PasswordHash,
        two_factor_token: Option<&str>,
        two_factor_provider: Option<u32>,
        two_factor_code: Option<&str>,
        device_verification_code: Option<&str>,
    ) -> Result<(String, String, String)> {
        let mut two_factor_token = two_factor_token.map(String::from);
        if two_factor_token.is_none() {
            two_factor_token = two_factor_code.map(String::from);
        }

        let connect_req = ConnectTokenReq {
            grant_type: "password".to_string(),
            scope: "api offline_access".to_string(),
            client_id: "cli".to_string(),
            device_type: u32::from(DEVICE_TYPE),
            device_identifier: device_id.to_string(),
            device_name: "cosmic-bwarden".to_string(),
            device_push_token: String::new(),
            two_factor_token,
            two_factor_provider,
            device_verification_code: device_verification_code.map(String::from),
            auth: ConnectTokenAuth::Password(ConnectTokenPassword {
                username: email.to_string(),
                password: crate::base64::encode(password_hash.hash()),
            }),
        };

        let client = self.reqwest_client().await?;
        let res = client
            .post(self.identity_url("/connect/token"))
            .form(&connect_req)
            .header("auth-email", crate::base64::encode_url_safe_no_pad(email))
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;

        if res.status() == reqwest::StatusCode::OK {
            let connect_res: ConnectTokenRes = res.json_with_path().await?;
            Ok((
                connect_res.access_token,
                connect_res.refresh_token.unwrap_or_default(),
                connect_res.key.unwrap_or_default(),
            ))
        } else {
            let code = res.status().as_u16();
            match res.text().await {
                Ok(body) => match body.clone().json_with_path() {
                    Ok(json) => Err(classify_login_error(&json, code)),
                    Err(e) => {
                        log::warn!("{e}: {body}");
                        Err(Error::RequestFailed { status: code })
                    }
                },
                Err(e) => {
                    log::warn!("failed to read response body: {e}");
                    Err(Error::RequestFailed { status: code })
                }
            }
        }
    }

    pub async fn sync(
        &self,
        access_token: &str,
    ) -> Result<(
        String,
        Option<String>,
        std::collections::HashMap<String, String>,
        Vec<crate::db::Entry>,
    )> {
        let client = self.reqwest_client().await?;
        let res = client
            .get(self.api_url("/sync"))
            .header("Authorization", format!("Bearer {access_token}"))
            .header("Bitwarden-Client-Version", "2024.12.0")
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;
        match res.status() {
            reqwest::StatusCode::OK => {
                let sync_res: SyncRes = res.json_with_path().await?;
                let folders = sync_res.folders.clone();
                let ciphers = sync_res
                    .ciphers
                    .iter()
                    .filter_map(|cipher| cipher.to_entry(&folders))
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
            folder_id: serde_json::Value::Null,
            favorite: false,
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
        private_key: &str,
        public_key: Option<&str>,
        notes: Option<&str>,
        fields: Option<Vec<CipherField>>,
    ) -> Result<()> {
        let req = CiphersPostReq {
            ty: 5,
            folder_id: serde_json::Value::Null,
            favorite: false,
            name: name.to_string(),
            notes: notes.map(String::from),
            login: None,
            card: None,
            identity: None,
            secure_note: None,
            ssh_key: Some(serde_json::json!({
                "privateKey": private_key,
                "publicKey": public_key,
                "fingerprint": null,
            })),
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
            folder_id: serde_json::Value::Null,
            favorite: false,
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
            folder_id: serde_json::Value::Null,
            favorite: false,
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
        ty: u32,
        login: Option<serde_json::Value>,
        ssh_key: Option<serde_json::Value>,
        notes: Option<&str>,
        reprompt: Option<CipherRepromptType>,
        fields: Option<Vec<CipherField>>,
    ) -> Result<()> {
        let req = CiphersPutReq {
            ty,
            folder_id: serde_json::Value::Null,
            organization_id: None,
            favorite: false,
            name: name.to_string(),
            notes: notes.map(String::from),
            login,
            card: None,
            identity: None,
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

    pub async fn delete_cipher(&self, access_token: &str, id: &str) -> Result<()> {
        let client = self.reqwest_client().await?;
        let res = client
            .delete(self.api_url(&format!("/ciphers/{id}")))
            .header("Authorization", format!("Bearer {access_token}"))
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

    pub async fn folders(&self, access_token: &str) -> Result<Vec<(String, String)>> {
        let client = self.reqwest_client().await?;
        let res = client
            .get(self.api_url("/folders"))
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;
        match res.status() {
            reqwest::StatusCode::OK => {
                let folders_res: FoldersRes = res.json_with_path().await?;
                Ok(folders_res
                    .data
                    .iter()
                    .map(|folder| (folder.id.clone(), folder.name.clone()))
                    .collect())
            }
            reqwest::StatusCode::UNAUTHORIZED => Err(Error::RequestUnauthorized),
            _ => Err(Error::RequestFailed {
                status: res.status().as_u16(),
            }),
        }
    }

    pub async fn create_folder(&self, access_token: &str, name: &str) -> Result<String> {
        let req = FoldersPostReq {
            name: name.to_string(),
        };
        let client = self.reqwest_client().await?;
        let res = client
            .post(self.api_url("/folders"))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&req)
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;
        match res.status() {
            reqwest::StatusCode::OK => {
                let folders_res: FoldersResData = res.json_with_path().await?;
                Ok(folders_res.id)
            }
            reqwest::StatusCode::UNAUTHORIZED => Err(Error::RequestUnauthorized),
            _ => Err(Error::RequestFailed {
                status: res.status().as_u16(),
            }),
        }
    }

    pub async fn exchange_refresh_token(
        &self,
        refresh_token: &str,
    ) -> Result<(String, Option<String>, Option<String>)> {
        let connect_req = ConnectRefreshTokenReq {
            grant_type: "refresh_token".to_string(),
            client_id: "cli".to_string(),
            refresh_token: refresh_token.to_string(),
        };
        let client = self.reqwest_client().await?;
        let res = client
            .post(self.identity_url("/connect/token"))
            .form(&connect_req)
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;
        let connect_res: ConnectRefreshTokenRes = res.json_with_path().await?;
        Ok((
            connect_res.access_token,
            connect_res.refresh_token,
            connect_res.key,
        ))
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn identity_url(&self, path: &str) -> String {
        format!("{}{}", self.identity_url, path)
    }
}

pub(crate) fn classify_login_error(error_res: &ConnectErrorRes, code: u16) -> Error {
    let error_desc = error_res.error_description.clone();
    let error_desc = error_desc.as_deref();
    match error_res.error.as_str() {
        "invalid_grant" => match error_desc {
            Some("invalid_username_or_password") => {
                if let Some(error_model) = error_res.error_model.as_ref() {
                    let message = error_model.message.as_str().to_string();
                    return Error::IncorrectPassword { message };
                }
            }
            Some("Two factor required.") => {
                if let (Some(providers), Some(token)) = (
                    error_res.two_factor_providers.as_ref(),
                    error_res.sso_email_2fa_session_token.as_ref(),
                ) {
                    return Error::TwoFactorRequired {
                        providers: providers.iter().map(|p| *p as u32).collect(),
                        token: token.clone(),
                    };
                }
            }
            Some("Captcha required.") => {
                return Error::RegistrationRequired;
            }
            _ => {}
        },
        "invalid_client" => {
            return Error::IncorrectApiKey;
        }
        "device_error" => {
            return Error::NewDeviceVerificationRequired;
        }
        "" => {
            // bitwarden_rs returns an empty error and error_description for
            // this case, for some reason
            if let Some(error_model) = error_res.error_model.as_ref() {
                if error_desc.is_none() || error_desc == Some("") {
                    let message = error_model.message.as_str().to_string();
                    match message.as_str() {
                        "Username or password is incorrect. Try again"
                        | "TOTP code is not a number" => {
                            return Error::IncorrectPassword { message };
                        }
                        s => {
                            if s.starts_with("Invalid TOTP code! Server time: ") {
                                return Error::IncorrectPassword { message };
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }

    log::warn!("unexpected error received during login: {error_res:?}");
    Error::RequestFailed { status: code }
}
