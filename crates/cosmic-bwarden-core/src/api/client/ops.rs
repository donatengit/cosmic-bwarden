use crate::error::{Error, Result};
use crate::api::models::*;
use crate::api::client::Client;

impl Client {
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
