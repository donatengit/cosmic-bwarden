// serde_repr generates some as conversions that we can't seem to silence from
// here, unfortunately
#![allow(clippy::as_conversions)]

use crate::error::{Error, Result};

use crate::json::{DeserializeJsonWithPath as _, DeserializeJsonWithPathAsync as _};

#[derive(
    serde_repr::Serialize_repr, serde_repr::Deserialize_repr, Debug, Copy, Clone, PartialEq, Eq,
)]
#[repr(u8)]
pub enum UriMatchType {
    Domain = 0,
    Host = 1,
    StartsWith = 2,
    Exact = 3,
    RegularExpression = 4,
    Never = 5,
}

impl std::fmt::Display for UriMatchType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use UriMatchType::*;
        let s = match self {
            Domain => "domain",
            Host => "host",
            StartsWith => "starts_with",
            Exact => "exact",
            RegularExpression => "regular_expression",
            Never => "never",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TwoFactorProviderType {
    Authenticator = 0,
    Email = 1,
    Duo = 2,
    Yubikey = 3,
    U2f = 4,
    Remember = 5,
    OrganizationDuo = 6,
    WebAuthn = 7,
}

impl TwoFactorProviderType {
    pub fn message(&self) -> &str {
        match *self {
            Self::Authenticator => {
                "Enter the 6 digit verification code from your authenticator app."
            }
            Self::Yubikey => "Insert your Yubikey and push the button.",
            Self::Email => "Enter the PIN you received via email.",
            _ => "Enter the code.",
        }
    }

    pub fn header(&self) -> &str {
        match *self {
            Self::Authenticator => "Authenticator App",
            Self::Yubikey => "Yubikey",
            Self::Email => "Email Code",
            _ => "Two Factor Authentication",
        }
    }

    pub fn grab(&self) -> bool {
        !matches!(self, Self::Email)
    }
}

impl<'de> serde::Deserialize<'de> for TwoFactorProviderType {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct TwoFactorProviderTypeVisitor;
        impl serde::de::Visitor<'_> for TwoFactorProviderTypeVisitor {
            type Value = TwoFactorProviderType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("two factor provider id")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                value.parse().map_err(serde::de::Error::custom)
            }

            fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                std::convert::TryFrom::try_from(value).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_any(TwoFactorProviderTypeVisitor)
    }
}

impl std::convert::TryFrom<u64> for TwoFactorProviderType {
    type Error = Error;

    fn try_from(ty: u64) -> Result<Self> {
        match ty {
            0 => Ok(Self::Authenticator),
            1 => Ok(Self::Email),
            2 => Ok(Self::Duo),
            3 => Ok(Self::Yubikey),
            4 => Ok(Self::U2f),
            5 => Ok(Self::Remember),
            6 => Ok(Self::OrganizationDuo),
            7 => Ok(Self::WebAuthn),
            _ => Err(Error::InvalidTwoFactorProvider {
                ty: format!("{ty}"),
            }),
        }
    }
}

impl std::str::FromStr for TwoFactorProviderType {
    type Err = Error;

    fn from_str(ty: &str) -> Result<Self> {
        match ty {
            "0" => Ok(Self::Authenticator),
            "1" => Ok(Self::Email),
            "2" => Ok(Self::Duo),
            "3" => Ok(Self::Yubikey),
            "4" => Ok(Self::U2f),
            "5" => Ok(Self::Remember),
            "6" => Ok(Self::OrganizationDuo),
            "7" => Ok(Self::WebAuthn),
            _ => Err(Error::InvalidTwoFactorProvider { ty: ty.to_string() }),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum KdfType {
    Pbkdf2 = 0,
    Argon2id = 1,
}

impl<'de> serde::Deserialize<'de> for KdfType {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct KdfTypeVisitor;
        impl serde::de::Visitor<'_> for KdfTypeVisitor {
            type Value = KdfType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("kdf id")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                value.parse().map_err(serde::de::Error::custom)
            }

            fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                std::convert::TryFrom::try_from(value).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_any(KdfTypeVisitor)
    }
}

impl std::convert::TryFrom<u64> for KdfType {
    type Error = Error;

    fn try_from(ty: u64) -> Result<Self> {
        match ty {
            0 => Ok(Self::Pbkdf2),
            1 => Ok(Self::Argon2id),
            _ => Err(Error::InvalidKdfType {
                ty: format!("{ty}"),
            }),
        }
    }
}

impl std::str::FromStr for KdfType {
    type Err = Error;

    fn from_str(ty: &str) -> Result<Self> {
        match ty {
            "0" => Ok(Self::Pbkdf2),
            "1" => Ok(Self::Argon2id),
            _ => Err(Error::InvalidKdfType { ty: ty.to_string() }),
        }
    }
}

impl serde::Serialize for KdfType {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            Self::Pbkdf2 => "0",
            Self::Argon2id => "1",
        };
        serializer.serialize_str(s)
    }
}

#[derive(
    serde_repr::Serialize_repr, serde_repr::Deserialize_repr, Debug, Copy, Clone, PartialEq, Eq,
)]
#[repr(u8)]
pub enum CipherRepromptType {
    None = 0,
    Password = 1,
}

#[derive(serde::Serialize, Debug)]
struct PreloginReq {
    email: String,
}

#[derive(serde::Deserialize, Debug)]
struct PreloginRes {
    #[serde(rename = "Kdf", alias = "kdf")]
    kdf: KdfType,
    #[serde(rename = "KdfIterations", alias = "kdfIterations")]
    kdf_iterations: u32,
    #[serde(rename = "KdfMemory", alias = "kdfMemory")]
    kdf_memory: Option<u32>,
    #[serde(rename = "KdfParallelism", alias = "kdfParallelism")]
    kdf_parallelism: Option<u32>,
}

#[derive(serde::Serialize, Debug)]
struct ConnectTokenReq {
    grant_type: String,
    scope: String,
    client_id: String,
    #[serde(rename = "deviceType")]
    device_type: u32,
    #[serde(rename = "deviceIdentifier")]
    device_identifier: String,
    #[serde(rename = "deviceName")]
    device_name: String,
    #[serde(rename = "devicePushToken")]
    device_push_token: String,
    #[serde(rename = "twoFactorToken")]
    two_factor_token: Option<String>,
    #[serde(rename = "twoFactorProvider")]
    two_factor_provider: Option<u32>,
    #[serde(rename = "newDeviceOtp", skip_serializing_if = "Option::is_none")]
    device_verification_code: Option<String>,
    #[serde(flatten)]
    auth: ConnectTokenAuth,
}

#[derive(serde::Serialize, Debug)]
#[serde(untagged)]
enum ConnectTokenAuth {
    Password(ConnectTokenPassword),
    AuthCode(ConnectTokenAuthCode),
    ClientCredentials(ConnectTokenClientCredentials),
}

#[derive(serde::Serialize, Debug)]
struct ConnectTokenPassword {
    username: String,
    password: String,
}

#[derive(serde::Serialize, Debug)]
struct ConnectTokenAuthCode {
    code: String,
    code_verifier: String,
    redirect_uri: String,
}

#[derive(serde::Serialize, Debug)]
struct ConnectTokenClientCredentials {
    username: String,
    client_secret: String,
}

#[derive(serde::Deserialize, Debug)]
struct ConnectTokenRes {
    access_token: String,
    refresh_token: Option<String>,
    #[serde(rename = "Key", alias = "key")]
    key: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct ConnectErrorRes {
    error: String,
    error_description: Option<String>,
    #[serde(rename = "ErrorModel", alias = "errorModel")]
    error_model: Option<ConnectErrorResErrorModel>,
    #[serde(rename = "TwoFactorProviders", alias = "twoFactorProviders")]
    two_factor_providers: Option<Vec<TwoFactorProviderType>>,
    #[serde(rename = "SsoEmail2faSessionToken", alias = "ssoEmail2faSessionToken")]
    sso_email_2fa_session_token: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct ConnectErrorResErrorModel {
    #[serde(rename = "Message", alias = "message")]
    message: String,
}

#[derive(serde::Serialize, Debug)]
struct ConnectRefreshTokenReq {
    grant_type: String,
    client_id: String,
    refresh_token: String,
}

#[derive(serde::Deserialize, Debug)]
struct ConnectRefreshTokenRes {
    access_token: String,
    refresh_token: Option<String>,
    #[serde(rename = "Key", alias = "key")]
    key: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct SyncRes {
    #[serde(rename = "Ciphers", alias = "ciphers")]
    ciphers: Vec<SyncResCipher>,
    #[serde(rename = "Profile", alias = "profile")]
    profile: SyncResProfile,
    #[serde(rename = "Folders", alias = "folders")]
    folders: Vec<SyncResFolder>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncResCipher {
    pub id: String,
    pub folder_id: Option<String>,
    pub organization_id: Option<String>,
    pub name: String,
    #[serde(alias = "Login")]
    pub login: Option<CipherLogin>,
    #[serde(alias = "Card")]
    pub card: Option<CipherCard>,
    #[serde(alias = "Identity")]
    pub identity: Option<CipherIdentity>,
    #[serde(alias = "SecureNote")]
    pub secure_note: Option<CipherSecureNote>,
    #[serde(alias = "SshKey")]
    pub ssh_key: Option<CipherSshKey>,
    pub notes: Option<String>,
    pub password_history: Option<Vec<SyncResPasswordHistory>>,
    pub fields: Option<Vec<CipherField>>,
    pub deleted_date: Option<String>,
    #[serde(alias = "Key")]
    pub key: Option<String>,
    pub reprompt: CipherRepromptType,
}

impl SyncResCipher {
    fn to_entry(&self, folders: &[SyncResFolder]) -> Option<crate::db::Entry> {
        if self.deleted_date.is_some() {
            return None;
        }
        let history = self
            .password_history
            .as_ref()
            .map_or_else(Vec::new, |history| {
                history
                    .iter()
                    .filter_map(|entry| {
                        // Gets rid of entries with a non-existent
                        // password
                        entry.password.clone().map(|p| crate::db::HistoryEntry {
                            last_used_date: entry.last_used_date.clone(),
                            password: p.into(),
                        })
                    })
                    .collect()
            });

        let (folder, folder_id) = self.folder_id.as_ref().map_or((None, None), |folder_id| {
            let mut folder_name = None;
            for folder in folders {
                if &folder.id == folder_id {
                    folder_name = Some(folder.name.clone());
                }
            }
            (folder_name, Some(folder_id))
        });
        let data = if let Some(login) = &self.login {
            crate::db::EntryData::Login {
                username: login.username.clone(),
                password: login.password.clone().map(Into::into),
                totp: login.totp.clone().map(Into::into),
                uris: login.uris.as_ref().map_or_else(std::vec::Vec::new, |uris| {
                    uris.iter()
                        .filter_map(|uri| {
                            uri.uri.clone().map(|s| crate::db::Uri {
                                uri: s,
                                match_type: uri.match_type,
                            })
                        })
                        .collect()
                }),
            }
        } else if let Some(card) = &self.card {
            crate::db::EntryData::Card {
                cardholder_name: card.cardholder_name.clone(),
                number: card.number.clone().map(Into::into),
                brand: card.brand.clone(),
                exp_month: card.exp_month.clone(),
                exp_year: card.exp_year.clone(),
                code: card.code.clone().map(Into::into),
            }
        } else if let Some(identity) = &self.identity {
            crate::db::EntryData::Identity {
                title: identity.title.clone(),
                first_name: identity.first_name.clone(),
                middle_name: identity.middle_name.clone(),
                last_name: identity.last_name.clone(),
                address1: identity.address1.clone(),
                address2: identity.address2.clone(),
                address3: identity.address3.clone(),
                city: identity.city.clone(),
                state: identity.state.clone(),
                postal_code: identity.postal_code.clone(),
                country: identity.country.clone(),
                phone: identity.phone.clone(),
                email: identity.email.clone(),
                ssn: identity.ssn.clone(),
                license_number: identity.license_number.clone(),
                passport_number: identity.passport_number.clone(),
                username: identity.username.clone(),
            }
        } else if let Some(_secure_note) = &self.secure_note {
            crate::db::EntryData::SecureNote
        } else if let Some(ssh_key) = &self.ssh_key {
            crate::db::EntryData::SshKey {
                private_key: ssh_key.private_key.clone().map(Into::into),
                public_key: ssh_key.public_key.clone(),
                fingerprint: ssh_key.fingerprint.clone(),
            }
        } else {
            return None;
        };
        let fields = self.fields.as_ref().map_or_else(Vec::new, |fields| {
            fields
                .iter()
                .map(|field| crate::db::Field {
                    ty: field.ty,
                    name: field.name.clone(),
                    value: field.value.clone().map(Into::into),
                    linked_id: field.linked_id,
                })
                .collect()
        });
        Some(crate::db::Entry {
            id: self.id.clone(),
            org_id: self.organization_id.clone(),
            folder,
            folder_id: folder_id.map(std::string::ToString::to_string),
            name: self.name.clone(),
            data,
            fields,
            notes: self.notes.clone().map(Into::into),
            history,
            key: self.key.clone(),
            master_password_reprompt: self.reprompt,
        })
    }
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncResProfile {
    #[serde(alias = "Key")]
    pub key: String,
    pub private_key: Option<String>,
    pub protected_private_key: Option<String>,
    pub organizations: Vec<SyncResProfileOrganization>,
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncResProfileOrganization {
    pub id: String,
    #[serde(alias = "Key")]
    pub key: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncResFolder {
    pub id: String,
    pub name: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CipherLogin {
    pub username: Option<String>,
    pub password: Option<String>,
    pub totp: Option<String>,
    pub uris: Option<Vec<CipherLoginUri>>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CipherLoginUri {
    pub uri: Option<String>,
    #[serde(rename = "match")]
    pub match_type: Option<UriMatchType>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CipherCard {
    pub cardholder_name: Option<String>,
    pub number: Option<String>,
    pub brand: Option<String>,
    pub exp_month: Option<String>,
    pub exp_year: Option<String>,
    pub code: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CipherIdentity {
    pub title: Option<String>,
    pub first_name: Option<String>,
    pub middle_name: Option<String>,
    pub last_name: Option<String>,
    pub address1: Option<String>,
    pub address2: Option<String>,
    pub address3: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub ssn: Option<String>,
    pub license_number: Option<String>,
    pub passport_number: Option<String>,
    pub username: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CipherSshKey {
    pub private_key: Option<String>,
    pub public_key: Option<String>,
    #[serde(alias = "keyFingerprint")]
    pub fingerprint: Option<String>,
}

#[derive(
    serde_repr::Serialize_repr, serde_repr::Deserialize_repr, Debug, Clone, Copy, PartialEq, Eq,
)]
#[repr(u16)]
pub enum FieldType {
    Text = 0,
    Hidden = 1,
    Boolean = 2,
    Linked = 3,
}

pub type ApiFieldType = FieldType;

#[derive(
    serde_repr::Serialize_repr, serde_repr::Deserialize_repr, Debug, Clone, Copy, PartialEq, Eq,
)]
#[repr(u16)]
pub enum LinkedIdType {
    LoginUsername = 100,
    LoginPassword = 101,
    Username = 102,
    Password = 103,
    CardCardholderName = 300,
    CardExpMonth = 301,
    CardExpYear = 302,
    CardCode = 303,
    CardBrand = 304,
    CardNumber = 305,
    IdentityTitle = 400,
    IdentityMiddleName = 401,
    IdentityAddress1 = 402,
    IdentityAddress2 = 403,
    IdentityAddress3 = 404,
    IdentityCity = 405,
    IdentityState = 406,
    IdentityPostalCode = 407,
    IdentityCountry = 408,
    IdentityCompany = 409,
    IdentityEmail = 410,
    IdentityPhone = 411,
    IdentitySsn = 412,
    IdentityUsername = 413,
    IdentityPassportNumber = 414,
    IdentityLicenseNumber = 415,
    IdentityFirstName = 416,
    IdentityLastName = 417,
    IdentityFullName = 418,
}

pub type ApiLinkedIdType = LinkedIdType;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CipherField {
    #[serde(rename = "Type", alias = "type")]
    pub ty: Option<FieldType>,
    #[serde(rename = "Name", alias = "name")]
    pub name: Option<String>,
    #[serde(rename = "Value", alias = "value")]
    pub value: Option<String>,
    #[serde(rename = "LinkedId", alias = "linkedId")]
    pub linked_id: Option<LinkedIdType>,
}

impl From<crate::db::Field> for CipherField {
    fn from(f: crate::db::Field) -> Self {
        Self {
            ty: f.ty,
            name: f.name,
            value: f.value.map(|v| v.expose().to_string()),
            linked_id: f.linked_id,
        }
    }
}

// this is just a name and some notes, both of which are already on the cipher
// object
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CipherSecureNote {}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncResPasswordHistory {
    pub last_used_date: String,
    pub password: Option<String>,
}

#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CiphersPostReq {
    #[serde(rename = "type")]
    pub ty: u32,
    pub folder_id: serde_json::Value,
    pub favorite: bool,
    pub name: String,
    pub notes: Option<String>,
    pub login: Option<serde_json::Value>,
    pub card: Option<CipherCard>,
    pub identity: Option<CipherIdentity>,
    pub secure_note: Option<CipherSecureNote>,
    pub ssh_key: Option<serde_json::Value>,
    pub fields: Vec<CipherField>,
    pub reprompt: CipherRepromptType,
}

#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CiphersPutReq {
    #[serde(rename = "type")]
    pub ty: u32,
    pub folder_id: serde_json::Value,
    pub organization_id: Option<String>,
    pub favorite: bool,
    pub name: String,
    pub notes: Option<String>,
    pub login: Option<serde_json::Value>,
    pub card: Option<CipherCard>,
    pub identity: Option<CipherIdentity>,
    pub fields: Vec<CipherField>,
    pub secure_note: Option<CipherSecureNote>,
    pub ssh_key: Option<serde_json::Value>,
    pub password_history: Vec<CiphersPutReqHistory>,
    pub reprompt: CipherRepromptType,
}

#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CiphersPutReqHistory {
    pub last_used_date: String,
    pub password: String,
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct FoldersRes {
    #[serde(alias = "Data")]
    pub data: Vec<FoldersResData>,
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct FoldersResData {
    pub id: String,
    pub name: String,
}

#[derive(serde::Serialize, Debug)]
struct FoldersPostReq {
    name: String,
}

#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct RegisterReq {
    email: String,
    master_password_hash: String,
    master_password_hint: Option<String>,
    key: String,
    kdf: KdfType,
    kdf_iterations: u32,
}

// Used for the Bitwarden-Client-Name header. Accepted values:
// https://github.com/bitwarden/server/blob/main/src/Core/Enums/BitwardenClient.cs
const BITWARDEN_CLIENT: &str = "cli";
const BITWARDEN_VERSION: &str = "2024.12.0";

// DeviceType.LinuxDesktop, as per Bitwarden API device types.
const DEVICE_TYPE: u8 = 8;

#[derive(Debug)]
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
        master_password_hash: &str,
        protected_key: &str,
        kdf: KdfType,
        kdf_iterations: u32,
    ) -> Result<()> {
        let req = RegisterReq {
            email: email.to_string(),
            master_password_hash: master_password_hash.to_string(),
            master_password_hint: None,
            key: protected_key.to_string(),
            kdf,
            kdf_iterations,
        };
        let client = self.reqwest_client().await?;
        let res = client
            .post(self.api_url("/accounts/register"))
            .json(&req)
            .send()
            .await
            .map_err(|source| Error::Reqwest { source })?;
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

fn classify_login_error(error_res: &ConnectErrorRes, code: u16) -> Error {
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
