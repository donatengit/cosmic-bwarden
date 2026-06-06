#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to create block mode decryptor")]
    CreateBlockMode { source: aes::cipher::InvalidLength },

    #[error("failed to create hmac")]
    CreateHmac { source: aes::cipher::InvalidLength },

    #[error("failed to decrypt")]
    Decrypt { source: block_padding::UnpadError },

    #[error("invalid base64")]
    InvalidBase64 { source: base64::DecodeError },

    #[error("invalid cipherstring: {reason}")]
    InvalidCipherString { reason: String },

    #[error("invalid mac")]
    InvalidMac,

    #[error("invalid padding")]
    Padding,

    #[error("failed to decrypt RSA")]
    Rsa { source: rsa::errors::Error },

    #[error("failed to decode RSA PKCS8")]
    RsaPkcs8 { source: rsa::pkcs8::Error },

    #[error("cipherstring type {ty} too old")]
    TooOldCipherStringType { ty: String },

    #[error("unimplemented cipherstring type: {ty}")]
    UnimplementedCipherStringType { ty: String },

    #[error("network error: {source}")]
    Reqwest { source: reqwest::Error },

    #[error("failed to create reqwest client: {source}")]
    CreateReqwestClient { source: reqwest::Error },

    #[error("request failed with status: {status}")]
    RequestFailed { status: u16 },

    #[error("request unauthorized")]
    RequestUnauthorized,

    #[error("invalid two factor provider: {ty}")]
    InvalidTwoFactorProvider { ty: String },

    #[error("invalid kdf type: {ty}")]
    InvalidKdfType { ty: String },

    #[error("incorrect password: {message}")]
    IncorrectPassword { message: String },

    #[error("registration required")]
    RegistrationRequired,

    #[error("incorrect api key")]
    IncorrectApiKey,

    #[error("json error at {}: {}", .source.path(), .source.inner())]
    Json { source: serde_path_to_error::Error<serde_json::Error> },

    #[error("two factor authentication required")]
    TwoFactorRequired {
        providers: Vec<u32>, // Using u32 to avoid circular dependency if needed, or just import it
        token: String,
    },

    #[error("new device verification required")]
    NewDeviceVerificationRequired,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
