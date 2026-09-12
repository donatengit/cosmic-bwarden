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
    #[serde(rename = "BankAccount", alias = "bankAccount")]
    pub bank_account: Option<CipherBankAccount>,
    #[serde(rename = "DriversLicense", alias = "driversLicense")]
    pub drivers_license: Option<CipherDriversLicense>,
    #[serde(rename = "Passport", alias = "passport")]
    pub passport: Option<CipherPassport>,
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
                    uris: login.and_then(|l| l.uris.as_ref()).map_or_else(
                        std::vec::Vec::new,
                        |uris| {
                            uris.iter()
                                .filter_map(|uri| {
                                    uri.uri.clone().map(|s| crate::db::Uri {
                                        uri: s,
                                        match_type: uri.match_type,
                                    })
                                })
                                .collect()
                        },
                    ),
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
            6 => {
                let bank = self.bank_account.as_ref();
                crate::db::EntryData::BankAccount {
                    bank_name: bank.and_then(|b| b.bank_name.clone()),
                    name_on_account: bank.and_then(|b| b.name_on_account.clone()),
                    account_type: bank.and_then(|b| b.account_type.clone()),
                    account_number: bank.and_then(|b| b.account_number.clone().map(Into::into)),
                    routing_number: bank.and_then(|b| b.routing_number.clone().map(Into::into)),
                    branch_number: bank.and_then(|b| b.branch_number.clone()),
                    pin: bank.and_then(|b| b.pin.clone().map(Into::into)),
                    swift_code: bank.and_then(|b| b.swift_code.clone()),
                    iban: bank.and_then(|b| b.iban.clone()),
                    bank_contact_phone: bank.and_then(|b| b.bank_contact_phone.clone()),
                }
            }
            7 => {
                let dl = self.drivers_license.as_ref();
                crate::db::EntryData::DriversLicense {
                    first_name: dl.and_then(|d| d.first_name.clone()),
                    middle_name: dl.and_then(|d| d.middle_name.clone()),
                    last_name: dl.and_then(|d| d.last_name.clone()),
                    date_of_birth: dl.and_then(|d| d.date_of_birth.clone()),
                    license_number: dl.and_then(|d| d.license_number.clone().map(Into::into)),
                    issuing_country: dl.and_then(|d| d.issuing_country.clone()),
                    issuing_state: dl.and_then(|d| d.issuing_state.clone()),
                    issue_date: dl.and_then(|d| d.issue_date.clone()),
                    expiration_date: dl.and_then(|d| d.expiration_date.clone()),
                    issuing_authority: dl.and_then(|d| d.issuing_authority.clone()),
                    license_class: dl.and_then(|d| d.license_class.clone()),
                }
            }
            8 => {
                let passport = self.passport.as_ref();
                crate::db::EntryData::Passport {
                    surname: passport.and_then(|p| p.surname.clone()),
                    given_name: passport.and_then(|p| p.given_name.clone()),
                    date_of_birth: passport.and_then(|p| p.date_of_birth.clone()),
                    sex: passport.and_then(|p| p.sex.clone()),
                    birth_place: passport.and_then(|p| p.birth_place.clone()),
                    nationality: passport.and_then(|p| p.nationality.clone()),
                    issuing_country: passport.and_then(|p| p.issuing_country.clone()),
                    passport_number: passport
                        .and_then(|p| p.passport_number.clone().map(Into::into)),
                    passport_type: passport.and_then(|p| p.passport_type.clone()),
                    national_identification_number: passport
                        .and_then(|p| p.national_identification_number.clone()),
                    issuing_authority: passport.and_then(|p| p.issuing_authority.clone()),
                    issue_date: passport.and_then(|p| p.issue_date.clone()),
                    expiration_date: passport.and_then(|p| p.expiration_date.clone()),
                }
            }
            unknown => {
                // Never silently drop a cipher the server knows about: a
                // future cipher type would otherwise vanish from the vault
                // without a trace.
                log::warn!("entry {}: skipping unknown cipher type {unknown}", self.id);
                return None;
            }
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

/// Bitwarden v2026.7.0 cipher type 6 (`bankAccount`). Field set mirrors the
/// official clients' `BankAccountData` (tmp_code_examples/
/// bitwarden-official-clients/.../bank-account.data.ts).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CipherBankAccount {
    pub bank_name: Option<String>,
    pub name_on_account: Option<String>,
    pub account_type: Option<String>,
    pub account_number: Option<String>,
    pub routing_number: Option<String>,
    pub branch_number: Option<String>,
    pub pin: Option<String>,
    pub swift_code: Option<String>,
    pub iban: Option<String>,
    pub bank_contact_phone: Option<String>,
}

/// Bitwarden v2026.7.0 cipher type 7 (`driversLicense`). Field set mirrors the
/// official clients' `DriversLicenseData`.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CipherDriversLicense {
    pub first_name: Option<String>,
    pub middle_name: Option<String>,
    pub last_name: Option<String>,
    pub date_of_birth: Option<String>,
    pub license_number: Option<String>,
    pub issuing_country: Option<String>,
    pub issuing_state: Option<String>,
    pub issue_date: Option<String>,
    pub expiration_date: Option<String>,
    pub issuing_authority: Option<String>,
    pub license_class: Option<String>,
}

/// Bitwarden v2026.7.0 cipher type 8 (`passport`). Field set mirrors the
/// official clients' `PassportData`.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CipherPassport {
    pub surname: Option<String>,
    pub given_name: Option<String>,
    pub date_of_birth: Option<String>,
    pub sex: Option<String>,
    pub birth_place: Option<String>,
    pub nationality: Option<String>,
    pub issuing_country: Option<String>,
    pub passport_number: Option<String>,
    pub passport_type: Option<String>,
    pub national_identification_number: Option<String>,
    pub issuing_authority: Option<String>,
    pub issue_date: Option<String>,
    pub expiration_date: Option<String>,
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
    pub(crate) bank_account: Option<CipherBankAccount>,
    pub(crate) drivers_license: Option<CipherDriversLicense>,
    pub(crate) passport: Option<CipherPassport>,
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
    pub(crate) bank_account: Option<CipherBankAccount>,
    pub(crate) drivers_license: Option<CipherDriversLicense>,
    pub(crate) passport: Option<CipherPassport>,
    pub(crate) password_history: Vec<serde_json::Value>,
    pub(crate) reprompt: CipherRepromptType,
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

#[derive(
    serde_repr::Serialize_repr, serde_repr::Deserialize_repr, Debug, Clone, Copy, PartialEq, Eq,
)]
#[repr(u16)]
pub enum LinkedIdType {
    Username = 0,
    Password = 1,
}

#[cfg(test)]
mod new_item_type_tests {
    use super::*;

    /// A type-6 cipher in Vaultwarden's emission shape (v2026.7.0+):
    /// PascalCase top-level keys, camelCase typed-object keys.
    fn bank_cipher_json() -> serde_json::Value {
        serde_json::json!({
            "Id": "bank-1",
            "Type": 6,
            "Name": "2.1.0",
            "Favorite": false,
            "Reprompt": 0,
            "BankAccount": {
                "bankName": "2.1.0",
                "nameOnAccount": "Alice Smith",
                "accountType": "checking",
                "accountNumber": "2.0.account",
                "routingNumber": "2.0.routing",
                "branchNumber": "044",
                "pin": "2.0.pin",
                "swiftCode": "CHASUS33",
                "iban": "DE89370400440532013000",
                "bankContactPhone": "+1-555-0100"
            }
        })
    }

    fn dl_cipher_json() -> serde_json::Value {
        serde_json::json!({
            "id": "dl-1",
            "type": 7,
            "name": "2.1.0",
            "favorite": false,
            "reprompt": 0,
            "driversLicense": {
                "firstName": "Alice",
                "middleName": "Q",
                "lastName": "Smith",
                "dateOfBirth": "1990-01-01",
                "licenseNumber": "2.0.license",
                "issuingCountry": "US",
                "issuingState": "CA",
                "issueDate": "2019-01-01",
                "expirationDate": "2029-01-01",
                "issuingAuthority": "DMV",
                "licenseClass": "C"
            }
        })
    }

    fn passport_cipher_json() -> serde_json::Value {
        serde_json::json!({
            "id": "pp-1",
            "type": 8,
            "name": "2.1.0",
            "favorite": false,
            "reprompt": 0,
            "passport": {
                "surname": "Smith",
                "givenName": "Alice",
                "dateOfBirth": "1990-01-01",
                "sex": "F",
                "birthPlace": "Springfield",
                "nationality": "US",
                "issuingCountry": "US",
                "passportNumber": "2.0.passport",
                "passportType": "P",
                "nationalIdentificationNumber": "NIN-123",
                "issuingAuthority": "DOS",
                "issueDate": "2019-01-01",
                "expirationDate": "2029-01-01"
            }
        })
    }

    fn parse(value: serde_json::Value) -> SyncResCipher {
        serde_json::from_value(value).expect("fixture must deserialize")
    }

    #[test]
    fn type_6_bank_account_parses_all_fields() {
        let cipher = parse(bank_cipher_json());
        let entry = cipher.to_entry(&[]).expect("type 6 must map to an entry");
        match entry.data {
            crate::db::EntryData::BankAccount {
                bank_name,
                name_on_account,
                account_type,
                account_number,
                routing_number,
                branch_number,
                pin,
                swift_code,
                iban,
                bank_contact_phone,
            } => {
                assert_eq!(bank_name.as_deref(), Some("2.1.0"));
                assert_eq!(name_on_account.as_deref(), Some("Alice Smith"));
                assert_eq!(account_type.as_deref(), Some("checking"));
                assert_eq!(
                    account_number.as_ref().expect("secret").expose(),
                    "2.0.account"
                );
                assert_eq!(
                    routing_number.as_ref().expect("secret").expose(),
                    "2.0.routing"
                );
                assert_eq!(branch_number.as_deref(), Some("044"));
                assert_eq!(pin.as_ref().expect("secret").expose(), "2.0.pin");
                assert_eq!(swift_code.as_deref(), Some("CHASUS33"));
                assert_eq!(iban.as_deref(), Some("DE89370400440532013000"));
                assert_eq!(bank_contact_phone.as_deref(), Some("+1-555-0100"));
            }
            other => panic!("expected BankAccount, got {other:?}"),
        }
    }

    #[test]
    fn type_7_drivers_license_parses_all_fields() {
        let cipher = parse(dl_cipher_json());
        let entry = cipher.to_entry(&[]).expect("type 7 must map to an entry");
        match entry.data {
            crate::db::EntryData::DriversLicense {
                first_name,
                middle_name,
                last_name,
                date_of_birth,
                license_number,
                issuing_country,
                issuing_state,
                issue_date,
                expiration_date,
                issuing_authority,
                license_class,
            } => {
                assert_eq!(first_name.as_deref(), Some("Alice"));
                assert_eq!(middle_name.as_deref(), Some("Q"));
                assert_eq!(last_name.as_deref(), Some("Smith"));
                assert_eq!(date_of_birth.as_deref(), Some("1990-01-01"));
                assert_eq!(
                    license_number.as_ref().expect("secret").expose(),
                    "2.0.license"
                );
                assert_eq!(issuing_country.as_deref(), Some("US"));
                assert_eq!(issuing_state.as_deref(), Some("CA"));
                assert_eq!(issue_date.as_deref(), Some("2019-01-01"));
                assert_eq!(expiration_date.as_deref(), Some("2029-01-01"));
                assert_eq!(issuing_authority.as_deref(), Some("DMV"));
                assert_eq!(license_class.as_deref(), Some("C"));
            }
            other => panic!("expected DriversLicense, got {other:?}"),
        }
    }

    #[test]
    fn type_8_passport_parses_all_fields() {
        let cipher = parse(passport_cipher_json());
        let entry = cipher.to_entry(&[]).expect("type 8 must map to an entry");
        match entry.data {
            crate::db::EntryData::Passport {
                surname,
                given_name,
                date_of_birth,
                sex,
                birth_place,
                nationality,
                issuing_country,
                passport_number,
                passport_type,
                national_identification_number,
                issuing_authority,
                issue_date,
                expiration_date,
            } => {
                assert_eq!(surname.as_deref(), Some("Smith"));
                assert_eq!(given_name.as_deref(), Some("Alice"));
                assert_eq!(date_of_birth.as_deref(), Some("1990-01-01"));
                assert_eq!(sex.as_deref(), Some("F"));
                assert_eq!(birth_place.as_deref(), Some("Springfield"));
                assert_eq!(nationality.as_deref(), Some("US"));
                assert_eq!(issuing_country.as_deref(), Some("US"));
                assert_eq!(
                    passport_number.as_ref().expect("secret").expose(),
                    "2.0.passport"
                );
                assert_eq!(passport_type.as_deref(), Some("P"));
                assert_eq!(national_identification_number.as_deref(), Some("NIN-123"));
                assert_eq!(issuing_authority.as_deref(), Some("DOS"));
                assert_eq!(issue_date.as_deref(), Some("2019-01-01"));
                assert_eq!(expiration_date.as_deref(), Some("2029-01-01"));
            }
            other => panic!("expected Passport, got {other:?}"),
        }
    }

    #[test]
    fn unknown_cipher_type_is_skipped() {
        let mut value = bank_cipher_json();
        value["Type"] = serde_json::json!(9);
        let cipher = parse(value);
        assert!(cipher.to_entry(&[]).is_none());
    }
}
