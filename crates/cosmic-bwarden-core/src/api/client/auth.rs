use crate::error::{Error, Result};
use crate::json::{DeserializeJsonWithPathAsync as _};
use crate::api::models::*;
use crate::api::client::{Client, DEVICE_TYPE};

impl Client {
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
            _ => Err(Self::request_failed("POST", res).await),
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
                Err(map_connect_error(err))
            }
            _ => Err(Self::request_failed("POST", res).await),
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
            _ => Err(Self::request_failed("POST", res).await),
        }
    }
}

pub(crate) fn map_connect_error(err: ConnectErrorRes) -> Error {
    if err.error == "invalid_grant" {
        if let Some(token) = err.sso_email_2fa_session_token {
            Error::TwoFactorRequired {
                providers: err
                    .two_factor_providers
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| p as u32)
                    .collect(),
                token,
            }
        } else {
            Error::Other("Invalid credentials".to_string())
        }
    } else if err.error == "invalid_token"
        && err.error_description.as_deref() == Some("Device verification required.")
    {
        Error::NewDeviceVerificationRequired
    } else {
        Error::Other(
            err.error_description
                .unwrap_or_else(|| err.error_model.map(|m| m.message).unwrap_or(err.error)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_error(error: &str) -> ConnectErrorRes {
        ConnectErrorRes {
            error: error.to_string(),
            error_description: None,
            error_model: None,
            two_factor_providers: None,
            sso_email_2fa_session_token: None,
        }
    }

    #[test]
    fn invalid_grant_without_2fa_token_is_invalid_credentials() {
        let err = map_connect_error(base_error("invalid_grant"));
        assert!(matches!(err, Error::Other(msg) if msg == "Invalid credentials"));
    }

    #[test]
    fn invalid_grant_with_2fa_token_requires_two_factor() {
        let mut res = base_error("invalid_grant");
        res.sso_email_2fa_session_token = Some("session-token".to_string());
        res.two_factor_providers = Some(vec![TwoFactorProviderType::Authenticator]);

        let err = map_connect_error(res);
        match err {
            Error::TwoFactorRequired { providers, token } => {
                assert_eq!(providers, vec![0]);
                assert_eq!(token, "session-token");
            }
            other => panic!("expected TwoFactorRequired, got {other:?}"),
        }
    }

    #[test]
    fn invalid_token_with_device_verification_message() {
        let mut res = base_error("invalid_token");
        res.error_description = Some("Device verification required.".to_string());

        let err = map_connect_error(res);
        assert!(matches!(err, Error::NewDeviceVerificationRequired));
    }

    #[test]
    fn falls_back_to_error_description_when_present() {
        let mut res = base_error("some_error");
        res.error_description = Some("Human readable description".to_string());
        res.error_model = Some(ConnectErrorResErrorModel {
            message: "Model message".to_string(),
        });

        let err = map_connect_error(res);
        assert!(matches!(err, Error::Other(msg) if msg == "Human readable description"));
    }

    #[test]
    fn falls_back_to_error_model_message_when_no_description() {
        let mut res = base_error("some_error");
        res.error_model = Some(ConnectErrorResErrorModel {
            message: "Model message".to_string(),
        });

        let err = map_connect_error(res);
        assert!(matches!(err, Error::Other(msg) if msg == "Model message"));
    }

    #[test]
    fn falls_back_to_raw_error_code_when_nothing_else_present() {
        let err = map_connect_error(base_error("some_error"));
        assert!(matches!(err, Error::Other(msg) if msg == "some_error"));
    }
}
