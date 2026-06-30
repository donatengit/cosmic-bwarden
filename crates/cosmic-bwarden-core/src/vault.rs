use crate::api;
use crate::cipherstring::CipherString;
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::locked;
use std::collections::HashMap;

/// Unlock the vault using pre-derived identity keys (enc_key_expanded ‖ mac_key_expanded).
/// Used by the TPM PIN unlock path where the keys are unsealed from the TPM instead of
/// being derived from a password.
/// Derive org keys from already-unlocked vault symmetric keys (skipping protected-key
/// decryption). Used by the TPM PIN unlock path where the vault keys come from the
/// sealed blob rather than being derived from the master password.
pub fn decrypt_org_keys<S: std::hash::BuildHasher>(
    vault_keys: &locked::Keys,
    protected_private_key: Option<&str>,
    protected_org_keys: &HashMap<String, String, S>,
) -> Result<HashMap<String, locked::Keys>> {
    let private_key = if let Some(ppk) = protected_private_key {
        let ppk_cipher = CipherString::new(ppk)?;
        match ppk_cipher.decrypt_locked_symmetric(vault_keys) {
            Ok(pk) => Some(locked::PrivateKey::new(pk)),
            Err(e) => return Err(e),
        }
    } else {
        None
    };

    let mut org_keys = HashMap::new();
    if let Some(pk) = private_key {
        for (org_id, protected_org_key) in protected_org_keys {
            let pok_cipher = CipherString::new(protected_org_key)?;
            let org_key = match pok_cipher.decrypt_locked_asymmetric(&pk) {
                Ok(k) => locked::Keys::new(k),
                Err(e) => return Err(e),
            };
            org_keys.insert(org_id.clone(), org_key);
        }
    }
    Ok(org_keys)
}

pub fn unlock_from_keys<S: std::hash::BuildHasher>(
    identity_keys: &locked::Keys,
    protected_key: &str,
    protected_private_key: Option<&str>,
    protected_org_keys: &HashMap<String, String, S>,
) -> Result<(locked::Keys, HashMap<String, locked::Keys>)> {
    let protected_key = CipherString::new(protected_key)?;
    let vault_keys = match protected_key.decrypt_locked_symmetric(identity_keys) {
        Ok(master_keys) => locked::Keys::new(master_keys),
        Err(Error::InvalidMac) => {
            return Err(Error::Other("Password is incorrect. Try again.".to_string()))
        }
        Err(e) => return Err(e),
    };

    let org_keys = decrypt_org_keys(&vault_keys, protected_private_key, protected_org_keys)?;
    Ok((vault_keys, org_keys))
}

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
) -> Result<(locked::Keys, HashMap<String, locked::Keys>)> {
    let identity = Identity::new(email, password, kdf, iterations, memory, parallelism)?;
    unlock_from_keys(&identity.keys, protected_key, protected_private_key, protected_org_keys)
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
