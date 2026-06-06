use crate::api;
use crate::cipherstring::CipherString;
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::locked;
use std::collections::HashMap;

pub fn unlock<S: std::hash::BuildHasher>(
    email: &str,
    password: &locked::Password,
    kdf: api::KdfType,
    iterations: u32,
    memory: Option<u32>,
    parallelism: Option<u32>,
    protected_key: &str,
    protected_private_key: Option<&str>,
    protected_org_keys: &HashMap<String, String, S>,
) -> Result<(
    locked::Keys,
    HashMap<String, locked::Keys>,
)> {
    let identity = Identity::new(
        email,
        password,
        kdf,
        iterations,
        memory,
        parallelism,
    )?;

    let protected_key = CipherString::new(protected_key)?;
    let key = match protected_key.decrypt_locked_symmetric(&identity.keys) {
        Ok(master_keys) => locked::Keys::new(master_keys),
        Err(Error::InvalidMac) => {
            return Err(Error::Other("Password is incorrect. Try again.".to_string()))
        }
        Err(e) => return Err(e),
    };

    let private_key = if let Some(ppk) = protected_private_key {
        let protected_private_key = CipherString::new(ppk)?;
        match protected_private_key.decrypt_locked_symmetric(&key) {
            Ok(private_key) => Some(locked::PrivateKey::new(private_key)),
            Err(e) => return Err(e),
        }
    } else {
        None
    };

    let mut org_keys = HashMap::new();
    if let Some(pk) = private_key {
        for (org_id, protected_org_key) in protected_org_keys {
            let protected_org_key = CipherString::new(protected_org_key)?;
            let org_key = match protected_org_key.decrypt_locked_asymmetric(&pk) {
                Ok(org_key) => locked::Keys::new(org_key),
                Err(e) => return Err(e),
            };
            org_keys.insert(org_id.clone(), org_key);
        }
    }

    Ok((key, org_keys))
}

pub fn decrypt(
    cipherstring: &str,
    keys: &locked::Keys,
    entry_key: Option<&str>,
) -> Result<String> {
    let cipher = CipherString::new(cipherstring)?;
    let entry_key = if let Some(ek) = entry_key {
        let ek_cipher = CipherString::new(ek)?;
        Some(locked::Keys::new(ek_cipher.decrypt_locked_symmetric(keys)?))
    } else {
        None
    };

    let plaintext = cipher.decrypt_symmetric(keys, entry_key.as_ref())?;
    String::from_utf8(plaintext).map_err(|e| Error::Other(format!("invalid utf8 in decrypted secret: {}", e)))
}
