use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::Rng;

use crate::http::errors::ApiError;

const FORMAT_VERSION: u8 = 0x01;
const VERSION_LENGTH: usize = 1;
const IV_LENGTH: usize = 12;
const AUTH_TAG_LENGTH: usize = 16;

#[derive(Clone)]
pub struct Encryptor {
    cipher: Aes256Gcm,
}

impl Encryptor {
    pub fn new(key: &[u8; 32]) -> Self {
        let cipher = Aes256Gcm::new(key.into());
        Self { cipher }
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, ApiError> {
        let mut iv = [0u8; IV_LENGTH];
        rand::rng().fill_bytes(&mut iv);
        let nonce = Nonce::from_slice(&iv);

        let ciphertext = self
            .cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: &[],
                },
            )
            .map_err(|_| ApiError::internal("encryption failed"))?;

        let mut out = Vec::with_capacity(VERSION_LENGTH + IV_LENGTH + ciphertext.len());
        out.push(FORMAT_VERSION);
        out.extend_from_slice(&iv);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    pub fn decrypt(&self, encrypted: &[u8]) -> Result<String, ApiError> {
        let min_len = VERSION_LENGTH + IV_LENGTH + AUTH_TAG_LENGTH;
        if encrypted.len() < min_len {
            return Err(ApiError::internal(
                "Invalid encrypted data: buffer too short",
            ));
        }
        if encrypted[0] != FORMAT_VERSION {
            return Err(ApiError::internal(format!(
                "Unsupported encryption format version: {}",
                encrypted[0]
            )));
        }

        let iv = &encrypted[VERSION_LENGTH..VERSION_LENGTH + IV_LENGTH];
        let ct_and_tag = &encrypted[VERSION_LENGTH + IV_LENGTH..];
        let nonce = Nonce::from_slice(iv);

        let plaintext = self
            .cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ct_and_tag,
                    aad: &[],
                },
            )
            .map_err(|_| {
                ApiError::internal(
                    "Decryption failed: data may be corrupted or encrypted with a different key",
                )
            })?;

        String::from_utf8(plaintext)
            .map_err(|_| ApiError::internal("Decryption failed: invalid UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let key = [7u8; 32];
        let enc = Encryptor::new(&key);
        let blob = enc.encrypt("super-secret-value").unwrap();
        assert_eq!(blob[0], FORMAT_VERSION);

        assert_eq!(blob.len(), 1 + 12 + "super-secret-value".len() + 16);
        let back = enc.decrypt(&blob).unwrap();
        assert_eq!(back, "super-secret-value");
    }

    #[test]
    fn wrong_key_fails() {
        let blob = Encryptor::new(&[1u8; 32]).encrypt("x").unwrap();
        assert!(Encryptor::new(&[2u8; 32]).decrypt(&blob).is_err());
    }
}
