use crate::error::{Error, Result};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
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
}

#[derive(serde::Serialize, Debug)]
pub(crate) struct ConnectTokenPassword {
    pub(crate) username: String,
    pub(crate) password: String,
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
