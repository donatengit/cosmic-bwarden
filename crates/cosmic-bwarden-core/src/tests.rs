#[cfg(test)]
mod tests {
    use crate::cipherstring::CipherString;
    use crate::locked;
    use crate::db::{Entry, EntryData};

    #[test]
    fn test_text_processing_utf8_emojis() {
        let mut key_data = locked::Vec::new();
        key_data.extend([0u8; 64].iter().copied());
        let keys = locked::Keys::new(key_data);
        
        let test_strings = vec![
            "simple ascii",
            "special characters: !@#$%^&*()_+",
            "new\nlines\nand\rtabs\t",
            "English text",
            "UTF-8: π, é, 中文",
            "Emojis: 🦀, 🚀, 🔐",
        ];

        for original in test_strings {
            let encrypted = CipherString::encrypt_symmetric(&keys, original.as_bytes()).unwrap();
            let decrypted_bytes = encrypted.decrypt_symmetric(&keys, None).unwrap();
            let decrypted = String::from_utf8(decrypted_bytes).unwrap();
            assert_eq!(original, decrypted);
        }
    }

    #[test]
    fn test_entry_types_serialization() {
        let entries = vec![
            Entry {
                id: "1".to_string(),
                org_id: None,
                folder: None,
                folder_id: None,
                name: "Login Entry".to_string(),
                data: EntryData::Login {
                    username: Some("user".to_string()),
                    password: Some("pass".into()),
                    totp: None,
                    uris: vec![],
                },
                fields: vec![],
                notes: Some("Some notes".into()),
                history: vec![],
                key: None,
                master_password_reprompt: crate::api::CipherRepromptType::None,
                favorite: false,
            },
            Entry {
                id: "2".to_string(),
                org_id: None,
                folder: None,
                folder_id: None,
                name: "Secure Note".to_string(),
                data: EntryData::SecureNote,
                fields: vec![],
                notes: Some("Secret content here".into()),
                history: vec![],
                key: None,
                master_password_reprompt: crate::api::CipherRepromptType::None,
                favorite: false,
            },
            Entry {
                id: "3".to_string(),
                org_id: None,
                folder: None,
                folder_id: None,
                name: "SSH Key".to_string(),
                data: EntryData::SshKey {
                    private_key: Some("private".into()),
                    public_key: Some("public".to_string()),
                    fingerprint: Some("fingerprint".to_string()),
                },
                fields: vec![],
                notes: None,
                history: vec![],
                key: None,
                master_password_reprompt: crate::api::CipherRepromptType::None,
                favorite: false,
            },
            Entry {
                id: "4".to_string(),
                org_id: None,
                folder: None,
                folder_id: None,
                name: "Protected Entry".to_string(),
                data: EntryData::Login {
                    username: Some("protected".to_string()),
                    password: Some("secret".into()),
                    totp: None,
                    uris: vec![],
                },
                fields: vec![],
                notes: None,
                history: vec![],
                key: None,
                master_password_reprompt: crate::api::CipherRepromptType::Password,
                favorite: false,
            },
        ];

        for entry in entries {
            let serialized = serde_json::to_string(&entry).unwrap();
            let deserialized: Entry = serde_json::from_str(&serialized).unwrap();
            assert_eq!(entry, deserialized);
        }
    }
}
