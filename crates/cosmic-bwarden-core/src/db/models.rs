use crate::api;
use zeroize::Zeroize;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Entry {
    pub id: String,
    pub org_id: Option<String>,
    pub folder: Option<String>,
    pub folder_id: Option<String>,
    pub name: String,
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

    pub fn decrypt(&self, keys: &crate::locked::Keys) -> Self {
        let mut decrypted = self.clone();
        if let Ok(name) = crate::vault::decrypt(&self.name, keys, self.key.as_deref()) {
            decrypted.name = name;
        }
        if let Some(notes) = &self.notes {
            if let Ok(dec) = crate::vault::decrypt(notes.expose(), keys, self.key.as_deref()) {
                decrypted.notes = Some(Secret::from(dec));
            }
        }

        match &mut decrypted.data {
            EntryData::Login {
                username,
                password,
                totp,
                ..
            } => {
                if let Some(u) = username {
                    if let Ok(dec) = crate::vault::decrypt(u, keys, self.key.as_deref()) {
                        *username = Some(dec);
                    }
                }
                if let Some(p) = password {
                    if let Ok(dec) = crate::vault::decrypt(p.expose(), keys, self.key.as_deref()) {
                        *password = Some(Secret::from(dec));
                    }
                }
                if let Some(t) = totp {
                    if let Ok(dec) = crate::vault::decrypt(t.expose(), keys, self.key.as_deref()) {
                        *totp = Some(Secret::from(dec));
                    }
                }
            }
            EntryData::Card {
                cardholder_name,
                number,
                brand,
                exp_month,
                exp_year,
                code,
            } => {
                if let Some(v) = cardholder_name {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *cardholder_name = Some(dec);
                    }
                }
                if let Some(v) = number {
                    if let Ok(dec) = crate::vault::decrypt(v.expose(), keys, self.key.as_deref()) {
                        *number = Some(Secret::from(dec));
                    }
                }
                if let Some(v) = brand {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *brand = Some(dec);
                    }
                }
                if let Some(v) = exp_month {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *exp_month = Some(dec);
                    }
                }
                if let Some(v) = exp_year {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *exp_year = Some(dec);
                    }
                }
                if let Some(v) = code {
                    if let Ok(dec) = crate::vault::decrypt(v.expose(), keys, self.key.as_deref()) {
                        *code = Some(Secret::from(dec));
                    }
                }
            }
            EntryData::Identity {
                title,
                first_name,
                middle_name,
                last_name,
                address1,
                address2,
                address3,
                city,
                state,
                postal_code,
                country,
                phone,
                email,
                ssn,
                license_number,
                passport_number,
                username,
            } => {
                if let Some(v) = title {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *title = Some(dec);
                    }
                }
                if let Some(v) = first_name {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *first_name = Some(dec);
                    }
                }
                if let Some(v) = middle_name {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *middle_name = Some(dec);
                    }
                }
                if let Some(v) = last_name {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *last_name = Some(dec);
                    }
                }
                if let Some(v) = address1 {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *address1 = Some(dec);
                    }
                }
                if let Some(v) = address2 {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *address2 = Some(dec);
                    }
                }
                if let Some(v) = address3 {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *address3 = Some(dec);
                    }
                }
                if let Some(v) = city {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *city = Some(dec);
                    }
                }
                if let Some(v) = state {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *state = Some(dec);
                    }
                }
                if let Some(v) = postal_code {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *postal_code = Some(dec);
                    }
                }
                if let Some(v) = country {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *country = Some(dec);
                    }
                }
                if let Some(v) = phone {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *phone = Some(dec);
                    }
                }
                if let Some(v) = email {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *email = Some(dec);
                    }
                }
                if let Some(v) = ssn {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *ssn = Some(dec);
                    }
                }
                if let Some(v) = license_number {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *license_number = Some(dec);
                    }
                }
                if let Some(v) = passport_number {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *passport_number = Some(dec);
                    }
                }
                if let Some(v) = username {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *username = Some(dec);
                    }
                }
            }
            EntryData::SecureNote => {}
            EntryData::SshKey {
                private_key,
                public_key,
                fingerprint,
            } => {
                if let Some(v) = private_key {
                    if let Ok(dec) = crate::vault::decrypt(v.expose(), keys, self.key.as_deref()) {
                        *private_key = Some(Secret::from(dec));
                    }
                }
                if let Some(v) = public_key {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *public_key = Some(dec);
                    }
                }
                if let Some(v) = fingerprint {
                    if let Ok(dec) = crate::vault::decrypt(v, keys, self.key.as_deref()) {
                        *fingerprint = Some(dec);
                    }
                }
            }
        }

        for field in &mut decrypted.fields {
            if let Some(name) = &field.name {
                if let Ok(dec) = crate::vault::decrypt(name, keys, self.key.as_deref()) {
                    field.name = Some(dec);
                }
            }
            if let Some(value) = &field.value {
                if let Ok(dec) = crate::vault::decrypt(value.expose(), keys, self.key.as_deref()) {
                    field.value = Some(Secret::from(dec));
                }
            }
        }

        decrypted
    }
}

#[derive(serde::Serialize, Debug, Clone, Eq, PartialEq)]
pub struct Uri {
    pub uri: String,
    pub match_type: Option<api::UriMatchType>,
}

impl<'de> serde::Deserialize<'de> for Uri {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StringOrUri;
        impl<'de> serde::de::Visitor<'de> for StringOrUri {
            type Value = Uri;

            fn expecting(
                &self,
                formatter: &mut std::fmt::Formatter,
            ) -> std::fmt::Result {
                formatter.write_str("uri")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Uri {
                    uri: value.to_string(),
                    match_type: None,
                })
            }

            fn visit_map<M>(self, mut map: M) -> std::result::Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let mut uri = None;
                let mut match_type = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        "uri" => {
                            if uri.is_some() {
                                return Err(serde::de::Error::duplicate_field("uri"));
                            }
                            uri = Some(map.next_value()?);
                        }
                        "match_type" => {
                            if match_type.is_some() {
                                return Err(serde::de::Error::duplicate_field("match_type"));
                            }
                            match_type = map.next_value()?;
                        }
                        _ => {
                            return Err(serde::de::Error::unknown_field(
                                key,
                                &["uri", "match_type"],
                            ))
                        }
                    }
                }

                uri.map_or_else(
                    || Err(serde::de::Error::missing_field("uri")),
                    |uri| Ok(Self::Value { uri, match_type }),
                )
            }
        }

        deserializer.deserialize_any(StringOrUri)
    }
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
