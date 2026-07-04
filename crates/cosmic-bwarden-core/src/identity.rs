use crate::api;
use crate::error::{Error, Result};
use crate::locked;
use sha2::Digest as _;

pub struct Identity {
    pub email: String,
    pub keys: locked::Keys,
    pub master_password_hash: locked::PasswordHash,
}

impl Identity {
    pub fn new(
        email: &str,
        password: &locked::Password,
        kdf: api::KdfType,
        iterations: u32,
        memory: Option<u32>,
        parallelism: Option<u32>,
    ) -> Result<Self> {
        let email = email.trim().to_lowercase();

        let iterations = std::num::NonZeroU32::new(iterations)
            .ok_or(Error::Other("KDF iterations must be greater than zero".to_string()))?;

        let mut keys = locked::Vec::new();
        keys.extend(std::iter::repeat_n(0, 64));

        let enc_key = &mut keys.data_mut()[0..32];

        match kdf {
            api::KdfType::Pbkdf2 => {
                pbkdf2::pbkdf2::<hmac::Hmac<sha2::Sha256>>(
                    password.password(),
                    email.as_bytes(),
                    iterations.get(),
                    enc_key,
                )
                .map_err(|_| Error::Other("failed to run pbkdf2".to_string()))?;
            }

            api::KdfType::Argon2id => {
                let mut hasher = sha2::Sha256::new();
                hasher.update(email.as_bytes());
                let salt = hasher.finalize();

                // Validate and clamp KDF params. A corrupt DB or a hostile server
                // prelogin response must not panic (missing params) or drive the
                // agent to allocate arbitrary memory.
                let memory_mib = memory.ok_or_else(|| {
                    Error::Other("Argon2id requires a memory parameter".to_string())
                })?;
                let parallelism = parallelism.ok_or_else(|| {
                    Error::Other("Argon2id requires a parallelism parameter".to_string())
                })?;
                // Bitwarden allows 16..=1024 MiB memory and 1..=16 parallelism.
                if !(16..=1024).contains(&memory_mib) {
                    return Err(Error::Other(format!(
                        "Argon2id memory {memory_mib} MiB out of allowed range (16..=1024)"
                    )));
                }
                if !(1..=16).contains(&parallelism) {
                    return Err(Error::Other(format!(
                        "Argon2id parallelism {parallelism} out of allowed range (1..=16)"
                    )));
                }
                let mem_kib = memory_mib.checked_mul(1024).ok_or_else(|| {
                    Error::Other("Argon2id memory parameter overflow".to_string())
                })?;

                let params = argon2::Params::new(
                    mem_kib,
                    iterations.get(),
                    parallelism,
                    Some(32),
                )
                .map_err(|e| Error::Other(format!("invalid Argon2id parameters: {e}")))?;
                let argon2_config = argon2::Argon2::new(
                    argon2::Algorithm::Argon2id,
                    argon2::Version::V0x13,
                    params,
                );
                argon2::Argon2::hash_password_into(
                    &argon2_config,
                    password.password(),
                    &salt,
                    enc_key,
                )
                .map_err(|_| Error::Other("failed to run argon2".to_string()))?;
            }
        }

        let mut hash = locked::Vec::new();
        hash.extend(std::iter::repeat_n(0, 32));
        pbkdf2::pbkdf2::<hmac::Hmac<sha2::Sha256>>(
            &keys.data()[0..32],
            password.password(),
            1,
            hash.data_mut(),
        )
        .map_err(|_| Error::Other("failed to run pbkdf2 for master password hash".to_string()))?;

        let hkdf = hkdf::Hkdf::<sha2::Sha256>::from_prk(&keys.data()[0..32])
            .map_err(|_| Error::Other("failed to expand with hkdf".to_string()))?;
        
        let mut enc_key_expanded = [0u8; 32];
        hkdf.expand(b"enc", &mut enc_key_expanded)
            .map_err(|_| Error::Other("failed to expand enc key with hkdf".to_string()))?;
        keys.data_mut()[0..32].copy_from_slice(&enc_key_expanded);

        let mut mac_key_expanded = [0u8; 32];
        hkdf.expand(b"mac", &mut mac_key_expanded)
            .map_err(|_| Error::Other("failed to expand mac key with hkdf".to_string()))?;
        keys.data_mut()[32..64].copy_from_slice(&mac_key_expanded);

        let keys = locked::Keys::new(keys);
        let master_password_hash = locked::PasswordHash::new(hash);

        Ok(Self {
            email,
            keys,
            master_password_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::KdfType;

    fn pw() -> locked::Password {
        locked::Password::from_string("correct horse battery staple")
    }

    // Zero iterations would make PBKDF2/Argon2id degenerate; must be rejected
    // before any key derivation runs.
    #[test]
    fn zero_iterations_rejected() {
        let r = Identity::new("a@b.com", &pw(), KdfType::Pbkdf2, 0, None, None);
        assert!(r.is_err(), "zero iterations must be rejected");
    }

    // A hostile/corrupt prelogin response must not drive unbounded memory
    // allocation: Argon2id memory is clamped to Bitwarden's 16..=1024 MiB range.
    #[test]
    fn argon2id_rejects_memory_above_range() {
        let r = Identity::new("a@b.com", &pw(), KdfType::Argon2id, 3, Some(4096), Some(4));
        assert!(r.is_err(), "memory above 1024 MiB must be rejected");
    }

    #[test]
    fn argon2id_rejects_memory_below_range() {
        let r = Identity::new("a@b.com", &pw(), KdfType::Argon2id, 3, Some(8), Some(4));
        assert!(r.is_err(), "memory below 16 MiB must be rejected");
    }

    #[test]
    fn argon2id_rejects_parallelism_out_of_range() {
        let r = Identity::new("a@b.com", &pw(), KdfType::Argon2id, 3, Some(64), Some(64));
        assert!(r.is_err(), "parallelism above 16 must be rejected");
    }

    // Argon2id requires memory and parallelism; missing params must error, not panic.
    #[test]
    fn argon2id_requires_memory_and_parallelism() {
        assert!(Identity::new("a@b.com", &pw(), KdfType::Argon2id, 3, None, Some(4)).is_err());
        assert!(Identity::new("a@b.com", &pw(), KdfType::Argon2id, 3, Some(64), None).is_err());
    }

    // Valid params succeed and the email is normalized (trimmed + lowercased),
    // which matters because the email is the KDF salt.
    #[test]
    fn valid_params_succeed_and_normalize_email() {
        // Small, in-range params keep the test fast while exercising the happy path.
        let id = Identity::new("  Foo@Bar.COM ", &pw(), KdfType::Argon2id, 3, Some(16), Some(1))
            .expect("in-range Argon2id params should derive a key");
        assert_eq!(id.email, "foo@bar.com");

        let id2 = Identity::new("User@Example.com", &pw(), KdfType::Pbkdf2, 100, None, None)
            .expect("PBKDF2 with valid iterations should succeed");
        assert_eq!(id2.email, "user@example.com");
    }
}
