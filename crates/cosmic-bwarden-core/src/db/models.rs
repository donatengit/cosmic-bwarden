use crate::api;
use std::collections::HashMap;
use zeroize::Zeroize;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Entry {
    pub id: String,
    pub org_id: Option<String>,
    pub folder: Option<String>,
    pub folder_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub favorite: bool,
    pub data: EntryData,
    pub fields: Vec<Field>,
    pub notes: Option<Secret>,
    pub history: Vec<HistoryEntry>,
    pub key: Option<String>,
    pub master_password_reprompt: api::CipherRepromptType,
}

impl Entry {
    pub fn master_password_reprompt(&self) -> bool {
        self.master_password_reprompt != api::CipherRepromptType::None
    }

    pub fn get_field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name.as_deref() == Some(name))
    }

    pub fn set_field(&mut self, name: &str, value: &str, ty: api::FieldType) {
        if let Some(field) = self.fields.iter_mut().find(|f| f.name.as_deref() == Some(name)) {
            field.value = Some(Secret::from(value.to_string()));
            field.ty = Some(ty);
        } else {
            self.fields.push(Field {
                name: Some(name.to_string()),
                value: Some(Secret::from(value.to_string())),
                ty: Some(ty),
                linked_id: None,
            });
        }
    }

    pub fn remove_field(&mut self, name: &str) {
        self.fields.retain(|f| f.name.as_deref() != Some(name));
    }

    pub fn decrypt(&self, keys: &crate::locked::Keys, org_keys: &HashMap<String, crate::locked::Keys>) -> Self {
        let keys = self.org_id.as_ref().and_then(|id| org_keys.get(id)).unwrap_or(keys);
        let id = &self.id;
        let entry_key = self.key.as_deref();

        // Decrypt a plaintext field; logs a warning on failure so no error is ever silent.
        let d = |cipher: &str, field: &'static str| -> Option<String> {
            match crate::vault::decrypt(cipher, keys, entry_key) {
                Ok(v) => Some(v),
                Err(e) => {
                    log::warn!("entry {id}: failed to decrypt '{field}': {e}");
                    None
                }
            }
        };
        let ds = |cipher: &str, field: &'static str| -> Option<Secret> {
            d(cipher, field).map(Secret::from)
        };

        let mut decrypted = self.clone();

        if let Some(v) = d(&self.name, "name") { decrypted.name = v; }
        if let Some(notes) = &self.notes {
            if let Some(v) = ds(notes.expose(), "notes") { decrypted.notes = Some(v); }
        }

        match &mut decrypted.data {
            EntryData::Login { username, password, totp, uris } => {
                if let Some(u) = username.as_deref() { *username = d(u, "username"); }
                if let Some(p) = password.as_ref() { *password = ds(p.expose(), "password"); }
                if let Some(t) = totp.as_ref() { *totp = ds(t.expose(), "totp"); }
                for uri in uris.iter_mut() {
                    if let Some(plain) = d(&uri.uri, "uri") {
                        uri.uri = plain;
                    }
                }
            }
            EntryData::Card { cardholder_name, number, brand, exp_month, exp_year, code } => {
                if let Some(v) = cardholder_name.as_deref() { *cardholder_name = d(v, "cardholder_name"); }
                if let Some(v) = number.as_ref() { *number = ds(v.expose(), "number"); }
                if let Some(v) = brand.as_deref() { *brand = d(v, "brand"); }
                if let Some(v) = exp_month.as_deref() { *exp_month = d(v, "exp_month"); }
                if let Some(v) = exp_year.as_deref() { *exp_year = d(v, "exp_year"); }
                if let Some(v) = code.as_ref() { *code = ds(v.expose(), "code"); }
            }
            EntryData::Identity {
                title, first_name, middle_name, last_name,
                address1, address2, address3, city, state,
                postal_code, country, phone, email,
                ssn, license_number, passport_number, username,
            } => {
                if let Some(v) = title.as_deref() { *title = d(v, "title"); }
                if let Some(v) = first_name.as_deref() { *first_name = d(v, "first_name"); }
                if let Some(v) = middle_name.as_deref() { *middle_name = d(v, "middle_name"); }
                if let Some(v) = last_name.as_deref() { *last_name = d(v, "last_name"); }
                if let Some(v) = address1.as_deref() { *address1 = d(v, "address1"); }
                if let Some(v) = address2.as_deref() { *address2 = d(v, "address2"); }
                if let Some(v) = address3.as_deref() { *address3 = d(v, "address3"); }
                if let Some(v) = city.as_deref() { *city = d(v, "city"); }
                if let Some(v) = state.as_deref() { *state = d(v, "state"); }
                if let Some(v) = postal_code.as_deref() { *postal_code = d(v, "postal_code"); }
                if let Some(v) = country.as_deref() { *country = d(v, "country"); }
                if let Some(v) = phone.as_deref() { *phone = d(v, "phone"); }
                if let Some(v) = email.as_deref() { *email = d(v, "email"); }
                if let Some(v) = ssn.as_deref() { *ssn = d(v, "ssn"); }
                if let Some(v) = license_number.as_deref() { *license_number = d(v, "license_number"); }
                if let Some(v) = passport_number.as_deref() { *passport_number = d(v, "passport_number"); }
                if let Some(v) = username.as_deref() { *username = d(v, "username"); }
            }
            EntryData::SecureNote => {}
            EntryData::SshKey { private_key, public_key, fingerprint } => {
                if let Some(v) = private_key.as_ref() { *private_key = ds(v.expose(), "private_key"); }
                if let Some(v) = public_key.as_deref() { *public_key = d(v, "public_key"); }
                if let Some(v) = fingerprint.as_deref() { *fingerprint = d(v, "fingerprint"); }
            }
        }

        for field in &mut decrypted.fields {
            if let Some(n) = field.name.as_deref() {
                field.name = match d(n, "field.name") {
                    Some(v) => Some(v),
                    None => Some(n.to_string()),
                };
            }
            if let Some(v) = field.value.as_ref() {
                field.value = ds(v.expose(), "field.value");
            }
        }

        let mut pk_fallback = None;
        let mut pubk_fallback = None;
        for f in &decrypted.fields {
            if f.name.as_deref() == Some("private_key") {
                pk_fallback = f.value.clone();
            }
            if f.name.as_deref() == Some("public_key") {
                pubk_fallback = f.value.as_ref().map(|v| v.expose().to_string());
            }
        }

        // SSH Key Fallback
        if let EntryData::SshKey {
            private_key,
            public_key,
            ..
        } = &mut decrypted.data
        {
            if private_key.is_none() {
                *private_key = pk_fallback;
            }
            if public_key.is_none() {
                *public_key = pubk_fallback;
            }
        }

        decrypted
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Uri {
    pub uri: String,
    pub match_type: Option<api::UriMatchType>,
}

#[derive(Clone, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Zeroize for Secret {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "********")
    }
}

impl std::fmt::Display for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "********")
    }
}

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for Secret {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<String> for Secret {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Secret {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// Variant sizes intentionally differ (Identity carries ~17 fields, SecureNote
// none). Entries live in one Vec<Entry> per vault; boxing the big variants
// would add a heap hop on every decrypt/search for a modest memory win.
#[allow(clippy::large_enum_variant)]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum EntryData {
    Login {
        username: Option<String>,
        password: Option<Secret>,
        totp: Option<Secret>,
        uris: Vec<Uri>,
    },
    Card {
        cardholder_name: Option<String>,
        number: Option<Secret>,
        brand: Option<String>,
        exp_month: Option<String>,
        exp_year: Option<String>,
        code: Option<Secret>,
    },
    Identity {
        title: Option<String>,
        first_name: Option<String>,
        middle_name: Option<String>,
        last_name: Option<String>,
        address1: Option<String>,
        address2: Option<String>,
        address3: Option<String>,
        city: Option<String>,
        state: Option<String>,
        postal_code: Option<String>,
        country: Option<String>,
        phone: Option<String>,
        email: Option<String>,
        ssn: Option<String>,
        license_number: Option<String>,
        passport_number: Option<String>,
        username: Option<String>,
    },
    SecureNote,
    SshKey {
        private_key: Option<Secret>,
        public_key: Option<String>,
        fingerprint: Option<String>,
    },
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Field {
    pub ty: Option<api::FieldType>,
    pub name: Option<String>,
    pub value: Option<Secret>,
    pub linked_id: Option<api::LinkedIdType>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct HistoryEntry {
    pub last_used_date: String,
    pub password: Secret,
}
