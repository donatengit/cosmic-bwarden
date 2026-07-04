use crate::error::{Error, Result};
use crate::locked;

use aes::cipher::{BlockDecryptMut as _, BlockEncryptMut as _, KeyIvInit as _};
use hmac::Mac as _;
use pkcs8::DecodePrivateKey as _;
use rand::RngCore as _;
use zeroize::Zeroize as _;

pub enum CipherString {
    Symmetric {
        // ty: 2 (AES_256_CBC_HMAC_SHA256)
        iv: Vec<u8>,
        ciphertext: Vec<u8>,
        mac: Option<Vec<u8>>,
    },
    Asymmetric {
        // ty: 4 (RSA_2048_OAEP_SHA1)
        ciphertext: Vec<u8>,
    },
}

impl CipherString {
    pub fn new(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 2 {
            return Err(Error::InvalidCipherString {
                reason: "couldn't find type".to_string(),
            });
        }

        let ty = parts[0].as_bytes();
        if ty.len() != 1 || !ty[0].is_ascii_digit() {
            return Err(Error::UnimplementedCipherStringType {
                ty: parts[0].to_string(),
            });
        }

        let ty = ty[0] - b'0';
        let contents = parts[1];

        match ty {
            2 => {
                let parts: Vec<&str> = contents.split('|').collect();
                // Bitwarden type 2 (AES-256-CBC-HMAC-SHA256) always carries a MAC.
                // Reject MAC-less (2-part) cipherstrings: without authentication a
                // malicious server could strip the MAC and exploit CBC malleability.
                if parts.len() != 3 {
                    return Err(Error::InvalidCipherString {
                        reason: format!(
                            "type 2 cipherstring with {} parts (expected 3: iv|ct|mac)",
                            parts.len()
                        ),
                    });
                }

                let iv = crate::base64::decode(parts[0])
                    .map_err(|source| Error::InvalidBase64 { source })?;
                let ciphertext = crate::base64::decode(parts[1])
                    .map_err(|source| Error::InvalidBase64 { source })?;
                let mac = Some(
                    crate::base64::decode(parts[2])
                        .map_err(|source| Error::InvalidBase64 { source })?,
                );

                Ok(Self::Symmetric {
                    iv,
                    ciphertext,
                    mac,
                })
            }
            4 | 6 => {
                let contents = contents.split('|').next().unwrap();
                let ciphertext = crate::base64::decode(contents)
                    .map_err(|source| Error::InvalidBase64 { source })?;
                Ok(Self::Asymmetric { ciphertext })
            }
            _ => {
                if ty < 6 {
                    Err(Error::TooOldCipherStringType { ty: ty.to_string() })
                } else {
                    Err(Error::UnimplementedCipherStringType { ty: ty.to_string() })
                }
            }
        }
    }

    pub fn encrypt_symmetric(keys: &locked::Keys, plaintext: &[u8]) -> Result<Self> {
        let iv = random_iv();

        let cipher =
            cbc::Encryptor::<aes::Aes256>::new(keys.enc_key().into(), iv.as_slice().into());
        let ciphertext = cipher.encrypt_padded_vec_mut::<block_padding::Pkcs7>(plaintext);

        let mut digest = hmac::Hmac::<sha2::Sha256>::new_from_slice(keys.mac_key())
            .map_err(|source| Error::CreateHmac { source })?;
        digest.update(&iv);
        digest.update(&ciphertext);
        let mac = digest.finalize().into_bytes().as_slice().to_vec();

        Ok(Self::Symmetric {
            iv,
            ciphertext,
            mac: Some(mac),
        })
    }

    pub fn decrypt_symmetric(
        &self,
        keys: &locked::Keys,
        entry_key: Option<&locked::Keys>,
    ) -> Result<Vec<u8>> {
        if let Self::Symmetric {
            iv,
            ciphertext,
            mac,
        } = self
        {
            let cipher = decrypt_common_symmetric(
                entry_key.unwrap_or(keys),
                iv,
                ciphertext,
                mac.as_deref(),
            )?;
            cipher
                .decrypt_padded_vec_mut::<block_padding::Pkcs7>(ciphertext)
                .map_err(|source| Error::Decrypt { source })
        } else {
            Err(Error::InvalidCipherString {
                reason: "found an asymmetric cipherstring, expecting symmetric".to_string(),
            })
        }
    }

    pub fn decrypt_locked_symmetric(&self, keys: &locked::Keys) -> Result<locked::Vec> {
        if let Self::Symmetric {
            iv,
            ciphertext,
            mac,
        } = self
        {
            let mut res = locked::Vec::new();
            res.extend(ciphertext.iter().copied());
            let cipher = decrypt_common_symmetric(keys, iv, ciphertext, mac.as_deref())?;
            cipher
                .decrypt_padded_mut::<block_padding::Pkcs7>(res.data_mut())
                .map_err(|source| Error::Decrypt { source })?;
            Ok(res)
        } else {
            Err(Error::InvalidCipherString {
                reason: "found an asymmetric cipherstring, expecting symmetric".to_string(),
            })
        }
    }

    pub fn decrypt_locked_asymmetric(
        &self,
        private_key: &locked::PrivateKey,
    ) -> Result<locked::Vec> {
        if let Self::Asymmetric { ciphertext } = self {
            let privkey_data = private_key.private_key();
            let privkey_data = pkcs7_unpad(privkey_data).ok_or(Error::Padding)?;
            let pkey = rsa::RsaPrivateKey::from_pkcs8_der(privkey_data)
                .map_err(|source| Error::RsaPkcs8 { source })?;
            let mut bytes = pkey
                .decrypt(rsa::Oaep::new::<sha1::Sha1>(), ciphertext)
                .map_err(|source| Error::Rsa { source })?;

            let mut res = locked::Vec::new();
            res.extend(bytes.iter().copied());
            bytes.zeroize();

            Ok(res)
        } else {
            Err(Error::InvalidCipherString {
                reason: "found a symmetric cipherstring, expecting asymmetric".to_string(),
            })
        }
    }
}

fn decrypt_common_symmetric(
    keys: &locked::Keys,
    iv: &[u8],
    ciphertext: &[u8],
    mac: Option<&[u8]>,
) -> Result<cbc::Decryptor<aes::Aes256>> {
    if let Some(mac) = mac {
        let mut key = hmac::Hmac::<sha2::Sha256>::new_from_slice(keys.mac_key())
            .map_err(|source| Error::CreateHmac { source })?;
        key.update(iv);
        key.update(ciphertext);

        if key.verify(mac.into()).is_err() {
            return Err(Error::InvalidMac);
        }
    }

    cbc::Decryptor::<aes::Aes256>::new_from_slices(keys.enc_key(), iv)
        .map_err(|source| Error::CreateBlockMode { source })
}

impl std::fmt::Display for CipherString {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Symmetric {
                iv,
                ciphertext,
                mac,
            } => {
                let iv = crate::base64::encode(iv);
                let ciphertext = crate::base64::encode(ciphertext);
                if let Some(mac) = &mac {
                    let mac = crate::base64::encode(mac);
                    write!(f, "2.{iv}|{ciphertext}|{mac}")
                } else {
                    write!(f, "2.{iv}|{ciphertext}")
                }
            }
            Self::Asymmetric { ciphertext } => {
                let ciphertext = crate::base64::encode(ciphertext);
                write!(f, "4.{ciphertext}")
            }
        }
    }
}

fn random_iv() -> Vec<u8> {
    let mut iv = vec![0_u8; 16];
    let mut rng = rand::rng();
    rng.fill_bytes(&mut iv);
    iv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type2_requires_mac() {
        let iv = crate::base64::encode(&[0u8; 16]);
        let ct = crate::base64::encode(&[0u8; 16]);
        let mac = crate::base64::encode(&[0u8; 32]);

        // 3-part (iv|ct|mac) is accepted.
        assert!(CipherString::new(&format!("2.{iv}|{ct}|{mac}")).is_ok());

        // 2-part (MAC stripped) is rejected — no unauthenticated CBC.
        assert!(matches!(
            CipherString::new(&format!("2.{iv}|{ct}")),
            Err(Error::InvalidCipherString { .. })
        ));
    }

    #[test]
    fn non_digit_type_is_rejected_without_panic() {
        // Must not panic on `b'x' - b'0'` underflow.
        assert!(matches!(
            CipherString::new("x.aQ=="),
            Err(Error::UnimplementedCipherStringType { .. })
        ));
    }

    // Deterministic mini-fuzz: cipherstrings arrive from the server, so the
    // parser is attacker-reachable (threat A6). Arbitrary input must return
    // Ok/Err, never panic. Seeded so failures reproduce.
    #[test]
    fn arbitrary_input_never_panics() {
        use rand::{Rng as _, SeedableRng as _};
        let mut rng = rand::rngs::StdRng::seed_from_u64(0xC0B_5EED);

        // Structured near-misses first — the historically dangerous shapes.
        for s in [
            "",
            ".",
            "..",
            "2.",
            "2.|",
            "2.||",
            "2.|||",
            "4.",
            "6.",
            "9.x",
            "2.!!|??|##",
            "22.a|b|c",
            "-1.a",
            "\u{0}.\u{0}",
            "2.a|b|c|d",
        ] {
            let _ = CipherString::new(s);
        }

        // Random byte soup (valid UTF-8 by construction).
        for _ in 0..5_000 {
            let len = rng.random_range(0..64);
            let s: String = (0..len)
                .map(|_| char::from_u32(rng.random_range(1..0x500)).unwrap_or('.'))
                .collect();
            let _ = CipherString::new(&s);

            // And the same soup shoved into a type-2 payload position.
            let _ = CipherString::new(&format!("2.{s}"));
        }
    }
}

fn pkcs7_unpad(b: &[u8]) -> Option<&[u8]> {
    if b.is_empty() {
        return None;
    }

    let padding_val = b[b.len() - 1];
    if padding_val == 0 {
        return None;
    }

    let padding_len = usize::from(padding_val);
    if padding_len > b.len() {
        return None;
    }

    for c in b.iter().copied().skip(b.len() - padding_len) {
        if c != padding_val {
            return None;
        }
    }

    Some(&b[..b.len() - padding_len])
}
