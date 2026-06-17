use crate::api::models::auth::CipherRepromptType;

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
pub struct SyncResCipher {
    #[serde(rename = "Id", alias = "id")]
    pub id: String,
    #[serde(rename = "Type", alias = "type")]
    pub ty: u32,
    #[serde(rename = "FolderId", alias = "folderId")]
    pub folder_id: Option<String>,
    #[serde(rename = "OrganizationId", alias = "organizationId")]
    pub organization_id: Option<String>,
    #[serde(rename = "Favorite", alias = "favorite")]
    pub favorite: bool,
    #[serde(rename = "Name", alias = "name")]
    pub name: String,
    #[serde(rename = "Login", alias = "login")]
    pub login: Option<CipherLogin>,
    #[serde(rename = "Card", alias = "card")]
    pub card: Option<CipherCard>,
    #[serde(rename = "Identity", alias = "identity")]
    pub identity: Option<CipherIdentity>,
    #[serde(rename = "SecureNote", alias = "secureNote")]
    pub secure_note: Option<CipherSecureNote>,
    #[serde(rename = "SshKey", alias = "sshKey")]
    pub ssh_key: Option<CipherSshKey>,
    #[serde(rename = "Notes", alias = "notes")]
    pub notes: Option<String>,
    #[serde(rename = "PasswordHistory", alias = "passwordHistory")]
    pub password_history: Option<Vec<SyncResPasswordHistory>>,
    #[serde(rename = "Fields", alias = "fields")]
    pub fields: Option<Vec<CipherField>>,
    #[serde(rename = "DeletedDate", alias = "deletedDate")]
    pub deleted_date: Option<String>,
    #[serde(rename = "Key", alias = "key")]
    pub key: Option<String>,
    #[serde(rename = "Reprompt", alias = "reprompt")]
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
        let data = match self.ty {
            1 => {
                let login = self.login.as_ref();
                crate::db::EntryData::Login {
                    username: login.and_then(|l| l.username.clone()),
                    password: login.and_then(|l| l.password.clone().map(Into::into)),
                    totp: login.and_then(|l| l.totp.clone().map(Into::into)),
                    uris: login.and_then(|l| l.uris.as_ref()).map_or_else(std::vec::Vec::new, |uris| {
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
            }
            2 => crate::db::EntryData::SecureNote,
            3 => {
                let card = self.card.as_ref();
                crate::db::EntryData::Card {
                    cardholder_name: card.and_then(|c| c.cardholder_name.clone()),
                    number: card.and_then(|c| c.number.clone().map(Into::into)),
                    brand: card.and_then(|c| c.brand.clone()),
                    exp_month: card.and_then(|c| c.exp_month.clone()),
                    exp_year: card.and_then(|c| c.exp_year.clone()),
                    code: card.and_then(|c| c.code.clone().map(Into::into)),
                }
            }
            4 => {
                let identity = self.identity.as_ref();
                crate::db::EntryData::Identity {
                    title: identity.and_then(|i| i.title.clone()),
                    first_name: identity.and_then(|i| i.first_name.clone()),
                    middle_name: identity.and_then(|i| i.middle_name.clone()),
                    last_name: identity.and_then(|i| i.last_name.clone()),
                    address1: identity.and_then(|i| i.address1.clone()),
                    address2: identity.and_then(|i| i.address2.clone()),
                    address3: identity.and_then(|i| i.address3.clone()),
                    city: identity.and_then(|i| i.city.clone()),
                    state: identity.and_then(|i| i.state.clone()),
                    postal_code: identity.and_then(|i| i.postal_code.clone()),
                    country: identity.and_then(|i| i.country.clone()),
                    phone: identity.and_then(|i| i.phone.clone()),
                    email: identity.and_then(|i| i.email.clone()),
                    ssn: identity.and_then(|i| i.ssn.clone()),
                    license_number: identity.and_then(|i| i.license_number.clone()),
                    passport_number: identity.and_then(|i| i.passport_number.clone()),
                    username: identity.and_then(|i| i.username.clone()),
                }
            }
            5 => {
                let ssh_key = self.ssh_key.as_ref();
                crate::db::EntryData::SshKey {
                    private_key: ssh_key.and_then(|s| s.private_key.clone().map(Into::into)),
                    public_key: ssh_key.and_then(|s| s.public_key.clone()),
                    fingerprint: ssh_key.and_then(|s| s.fingerprint.clone()),
                }
            }
            _ => return None,
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
            favorite: self.favorite,
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
    #[serde(default = "default_uri_match_type")]
    pub match_type: Option<UriMatchType>,
}

fn default_uri_match_type() -> Option<UriMatchType> {
    Some(UriMatchType::Domain)
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
pub struct CipherSshKey {
    #[serde(rename = "PrivateKey", alias = "privateKey")]
    pub private_key: Option<String>,
    #[serde(rename = "PublicKey", alias = "publicKey")]
    pub public_key: Option<String>,
    #[serde(rename = "Fingerprint", alias = "keyFingerprint")]
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
    pub ty: Option<FieldType>,
    pub linked_id: Option<LinkedIdType>,
}


#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CiphersPostReq {
    #[serde(rename = "type")]
    pub(crate) ty: u32,
    pub(crate) folder_id: Option<String>,
    pub(crate) favorite: bool,
    pub(crate) name: String,
    pub(crate) notes: Option<String>,
    pub(crate) login: Option<serde_json::Value>,
    pub(crate) card: Option<CipherCard>,
    pub(crate) identity: Option<CipherIdentity>,
    pub(crate) secure_note: Option<CipherSecureNote>,
    pub(crate) ssh_key: Option<CipherSshKey>,
    pub(crate) fields: Vec<CipherField>,
    pub(crate) reprompt: CipherRepromptType,
}

#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CiphersPutReq {
    #[serde(rename = "type")]
    pub(crate) ty: u32,
    pub(crate) folder_id: Option<String>,
    pub(crate) organization_id: Option<String>,
    pub(crate) favorite: bool,
    pub(crate) name: String,
    pub(crate) notes: Option<String>,
    pub(crate) login: Option<serde_json::Value>,
    pub(crate) card: Option<CipherCard>,
    pub(crate) identity: Option<CipherIdentity>,
    pub(crate) fields: Vec<CipherField>,
    pub(crate) secure_note: Option<CipherSecureNote>,
    pub(crate) ssh_key: Option<CipherSshKey>,
    pub(crate) password_history: Vec<serde_json::Value>,
    pub(crate) reprompt: CipherRepromptType,
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
