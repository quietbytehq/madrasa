//! Sealing small secrets (a directory bind password) with the instance key: AES-256-GCM
//! with a random nonce, encoded as base64 `nonce || ciphertext`. The key is 32 random bytes
//! kept in the settings table and never logged.

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    #[error("the sealed value is not valid")]
    Invalid,
    #[error("the key is not 32 bytes")]
    BadKey,
}

/// A fresh 32-byte key.
pub fn generate_key() -> [u8; 32] {
    Aes256Gcm::generate_key(OsRng).into()
}

fn key_of(key: &[u8]) -> Result<&Key<Aes256Gcm>, SecretError> {
    if key.len() != 32 {
        return Err(SecretError::BadKey);
    }
    Ok(Key::<Aes256Gcm>::from_slice(key))
}

pub fn seal(key: &[u8], plaintext: &[u8]) -> Result<String, SecretError> {
    let cipher = Aes256Gcm::new(key_of(key)?);
    let nonce = Aes256Gcm::generate_nonce(OsRng);
    let ct = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| SecretError::Invalid)?;
    let mut out = nonce.to_vec();
    out.extend(ct);
    Ok(B64.encode(out))
}

pub fn open(key: &[u8], sealed: &str) -> Result<Vec<u8>, SecretError> {
    let key = key_of(key)?;
    let bytes = B64.decode(sealed).map_err(|_| SecretError::Invalid)?;
    if bytes.len() < 12 {
        return Err(SecretError::Invalid);
    }
    let (nonce, ct) = bytes.split_at(12);
    Aes256Gcm::new(key)
        .decrypt(Nonce::from_slice(nonce), ct)
        .map_err(|_| SecretError::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_and_open_round_trip_and_reject_tampering() {
        let key = generate_key();
        let sealed = seal(&key, b"s3cret").unwrap();
        assert_eq!(open(&key, &sealed).unwrap(), b"s3cret");
        assert_ne!(
            seal(&key, b"s3cret").unwrap(),
            sealed,
            "fresh nonce each time"
        );
        let other = generate_key();
        assert!(open(&other, &sealed).is_err());
        assert!(open(&key, "not base64!").is_err());
        assert!(seal(&key[..5], b"x").is_err());
    }
}
