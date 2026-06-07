// serde_repr generates some as conversions that we can't seem to silence from
// here, unfortunately
#![allow(clippy::as_conversions)]

use crate::error::{Error, Result};
use crate::json::{DeserializeJsonWithPath as _};

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

#[derive(
    serde_repr::Serialize_repr, serde_repr::Deserialize_repr, Debug, Copy, Clone, PartialEq, Eq,
)]
#[repr(u32)]
pub enum KdfType {
    Pbkdf2 = 0,
    Argon2id = 1,
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

#[derive(
    serde_repr::Serialize_repr, serde_repr::Deserialize_repr, Debug, Copy, Clone, PartialEq, Eq,
)]
#[repr(u8)]
pub enum CipherRepromptType {
    None = 0,
    Password = 1,
}

#[derive(serde::Serialize, Debug)]
pub(crate) struct PreloginReq {
    pub(crate) email: String,
}

#[derive(serde::Deserialize, Debug)]
pub(crate) struct PreloginRes {
    #[serde(rename = "Kdf", alias = "kdf")]
    pub(crate) kdf: KdfType,
    #[serde(rename = "KdfIterations", alias = "kdfIterations")]
    pub(crate) kdf_iterations: u32,
    #[serde(rename = "KdfMemory", alias = "kdfMemory")]
    pub(crate) kdf_memory: Option<u32>,
    #[serde(rename = "KdfParallelism", alias = "kdfParallelism")]
    pub(crate) kdf_parallelism: Option<u32>,
}

#[derive(serde::Serialize, Debug)]
pub(crate) struct ConnectTokenReq {
    pub(crate) grant_type: String,
    pub(crate) scope: String,
    pub(crate) client_id: String,
    #[serde(rename = "deviceType")]
    pub(crate) device_type: u32,
    #[serde(rename = "deviceIdentifier")]
    pub(crate) device_identifier: String,
    #[serde(rename = "deviceName")]
    pub(crate) device_name: String,
    #[serde(rename = "devicePushToken")]
    pub(crate) device_push_token: String,
    #[serde(rename = "twoFactorToken")]
    pub(crate) two_factor_token: Option<String>,
    #[serde(rename = "twoFactorProvider")]
    pub(crate) two_factor_provider: Option<u32>,
    #[serde(rename = "newDeviceOtp", skip_serializing_if = "Option::is_none")]
    pub(crate) device_verification_code: Option<String>,
    #[serde(flatten)]
    pub(crate) auth: ConnectTokenAuth,
}

#[derive(serde::Serialize, Debug)]
#[serde(untagged)]
pub(crate) enum ConnectTokenAuth {
    Password(ConnectTokenPassword),
    AuthCode(ConnectTokenAuthCode),
    ClientCredentials(ConnectTokenClientCredentials),
}

#[derive(serde::Serialize, Debug)]
pub(crate) struct ConnectTokenPassword {
    pub(crate) username: String,
    pub(crate) password: String,
}

#[derive(serde::Serialize, Debug)]
pub(crate) struct ConnectTokenAuthCode {
    pub(crate) code: String,
    pub(crate) code_verifier: String,
    pub(crate) redirect_uri: String,
}

#[derive(serde::Serialize, Debug)]
pub(crate) struct ConnectTokenClientCredentials {
    pub(crate) username: String,
    pub(crate) client_secret: String,
}

#[derive(serde::Deserialize, Debug)]
pub(crate) struct ConnectTokenRes {
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    #[serde(rename = "Key", alias = "key")]
    pub(crate) key: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
pub(crate) struct ConnectErrorRes {
    pub(crate) error: String,
    pub(crate) error_description: Option<String>,
    #[serde(rename = "ErrorModel", alias = "errorModel")]
    pub(crate) error_model: Option<ConnectErrorResErrorModel>,
    #[serde(rename = "TwoFactorProviders", alias = "twoFactorProviders")]
    pub(crate) two_factor_providers: Option<Vec<TwoFactorProviderType>>,
    #[serde(rename = "SsoEmail2faSessionToken", alias = "ssoEmail2faSessionToken")]
    pub(crate) sso_email_2fa_session_token: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
pub(crate) struct ConnectErrorResErrorModel {
    #[serde(rename = "Message", alias = "message")]
    pub(crate) message: String,
}

#[derive(serde::Serialize, Debug)]
pub(crate) struct ConnectRefreshTokenReq {
    pub(crate) grant_type: String,
    pub(crate) client_id: String,
    pub(crate) refresh_token: String,
}

#[derive(serde::Deserialize, Debug)]
pub(crate) struct ConnectRefreshTokenRes {
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    #[serde(rename = "Key", alias = "key")]
    pub(crate) key: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
pub(crate) struct SyncRes {
    #[serde(rename = "Ciphers", alias = "ciphers")]
    pub(crate) ciphers: Vec<SyncResCipher>,
    #[serde(rename = "Profile", alias = "profile")]
    pub(crate) profile: SyncResProfile,
    #[serde(rename = "Folders", alias = "folders")]
    pub(crate) folders: Vec<SyncResFolder>,
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
    pub(crate) fn to_entry(&self, folders: &[SyncResFolder]) -> Option<crate::db::Entry> {
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
                                match_type: Some(uri.match_type),
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
                    ty: field.ty.map(|t| match t {
                        ApiFieldType::Text => FieldType::Text,
                        ApiFieldType::Hidden => FieldType::Hidden,
                        ApiFieldType::Boolean => FieldType::Boolean,
                        ApiFieldType::Linked => FieldType::Linked,
                    }),
                    name: field.name.clone(),
                    value: field.value.clone().map(Into::into),
                    linked_id: field.linked_id.map(|l| match l {
                        ApiLinkedIdType::LoginUsername => LinkedIdType::Username,
                        ApiLinkedIdType::LoginPassword => LinkedIdType::Password,
                    }),
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
    pub name: String,
    #[serde(alias = "Key")]
    pub key: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
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
    pub uris: Option<Vec<CipherUri>>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CipherUri {
    pub uri: Option<String>,
    pub match_type: UriMatchType,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CipherCard {
    pub cardholder_name: Option<String>,
    pub brand: Option<String>,
    pub number: Option<String>,
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
    pub email: Option<String>,
    pub phone: Option<String>,
    pub ssn: Option<String>,
    pub license_number: Option<String>,
    pub passport_number: Option<String>,
    pub username: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CipherSecureNote {}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CipherSshKey {
    pub private_key: Option<String>,
    pub public_key: Option<String>,
    pub fingerprint: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncResPasswordHistory {
    pub password: Option<String>,
    pub last_used_date: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CipherField {
    pub name: Option<String>,
    pub value: Option<String>,
    #[serde(rename = "type")]
    pub ty: Option<ApiFieldType>,
    pub linked_id: Option<ApiLinkedIdType>,
}

#[derive(
    serde_repr::Serialize_repr, serde_repr::Deserialize_repr, Debug, Copy, Clone, PartialEq, Eq,
)]
#[repr(u8)]
pub enum ApiFieldType {
    Text = 0,
    Hidden = 1,
    Boolean = 2,
    Linked = 3,
}

#[derive(
    serde_repr::Serialize_repr, serde_repr::Deserialize_repr, Debug, Copy, Clone, PartialEq, Eq,
)]
#[repr(u8)]
pub enum ApiLinkedIdType {
    LoginUsername = 0,
    LoginPassword = 1,
}

#[derive(serde::Serialize, Debug)]
pub(crate) struct RegisterReq {
    pub(crate) email: String,
    pub(crate) name: String,
    #[serde(rename = "masterPasswordHash")]
    pub(crate) master_password_hash: String,
    #[serde(rename = "masterPasswordHint")]
    pub(crate) master_password_hint: String,
    pub(crate) key: String,
    pub(crate) kdf: KdfType,
    #[serde(rename = "kdfIterations")]
    pub(crate) kdf_iterations: u32,
}

#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CiphersPostReq {
    #[serde(rename = "type")]
    pub(crate) ty: u32,
    pub(crate) folder_id: serde_json::Value,
    pub(crate) favorite: bool,
    pub(crate) name: String,
    pub(crate) notes: Option<String>,
    pub(crate) login: Option<serde_json::Value>,
    pub(crate) card: Option<CipherCard>,
    pub(crate) identity: Option<CipherIdentity>,
    pub(crate) secure_note: Option<CipherSecureNote>,
    pub(crate) ssh_key: Option<serde_json::Value>,
    pub(crate) fields: Vec<CipherField>,
    pub(crate) reprompt: CipherRepromptType,
}

#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CiphersPutReq {
    #[serde(rename = "type")]
    pub(crate) ty: u32,
    pub(crate) folder_id: serde_json::Value,
    pub(crate) organization_id: Option<String>,
    pub(crate) favorite: bool,
    pub(crate) name: String,
    pub(crate) notes: Option<String>,
    pub(crate) login: Option<serde_json::Value>,
    pub(crate) card: Option<CipherCard>,
    pub(crate) identity: Option<CipherIdentity>,
    pub(crate) fields: Vec<CipherField>,
    pub(crate) secure_note: Option<CipherSecureNote>,
    pub(crate) ssh_key: Option<serde_json::Value>,
    pub(crate) password_history: Vec<serde_json::Value>,
    pub(crate) reprompt: CipherRepromptType,
}

#[derive(serde::Deserialize, Debug)]
pub(crate) struct FoldersRes {
    pub(crate) data: Vec<SyncResFolder>,
}

#[derive(serde::Deserialize, Debug)]
pub(crate) struct FoldersResData {
    pub(crate) id: String,
}

#[derive(serde::Serialize, Debug)]
pub(crate) struct FoldersPostReq {
    pub(crate) name: String,
}

#[derive(
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
#[repr(u16)]
pub enum FieldType {
    Text = 0,
    Hidden = 1,
    Boolean = 2,
    Linked = 3,
}

#[derive(
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
#[repr(u16)]
pub enum LinkedIdType {
    Username = 0,
    Password = 1,
}
