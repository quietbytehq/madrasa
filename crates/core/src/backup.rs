//! Backup key material.
//!
//! At setup a random 256-bit data encryption key (DEK) is generated. It is wrapped twice with
//! AES-256-GCM: by a key derived with Argon2id from the backup password, and by a key derived
//! from a random recovery key. Only the wrapped keys and salts are stored; the DEK, password
//! and recovery key never are. Either secret can later unwrap the DEK to decrypt backups.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// Recovery key: 8 groups of 4 Crockford base32 chars = 160 bits of entropy.
pub const RECOVERY_GROUPS: usize = 8;
pub const RECOVERY_GROUP_LEN: usize = 4;

#[derive(Debug, thiserror::Error)]
pub enum BackupKeyError {
    #[error("key derivation failed")]
    Derive,
    #[error("the password or recovery key is incorrect")]
    Unwrap,
    #[error("stored key material is malformed")]
    Malformed,
}

/// A DEK wrapped by one derived key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedKey {
    /// Argon2id salt (base64).
    pub salt: String,
    /// AES-GCM nonce (base64).
    pub nonce: String,
    /// Ciphertext + tag (base64).
    pub ciphertext: String,
}

/// Everything stored for backup encryption. Safe to persist in `settings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupKeySet {
    pub version: u8,
    pub by_password: WrappedKey,
    pub by_recovery_key: WrappedKey,
}

/// Output of setup: what to store and what to show once.
#[derive(Debug)]
pub struct BackupSetup {
    pub key_set: BackupKeySet,
    /// Formatted `XXXX-XXXX-...`; show once, never store.
    pub recovery_key: String,
}

/// Generate a recovery key in display form.
pub fn generate_recovery_key() -> String {
    let mut bytes = [0u8; RECOVERY_GROUPS * RECOVERY_GROUP_LEN];
    rand::thread_rng().fill_bytes(&mut bytes);
    let chars: Vec<char> = bytes
        .iter()
        .map(|b| CROCKFORD[(*b % 32) as usize] as char)
        .collect();
    chars
        .chunks(RECOVERY_GROUP_LEN)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

/// Canonical form of a recovery key as typed by a user: uppercase, Crockford aliases folded,
/// separators removed. Returns `None` if it is not a valid key.
pub fn normalize_recovery_key(input: &str) -> Option<String> {
    let mut out = String::with_capacity(RECOVERY_GROUPS * RECOVERY_GROUP_LEN);
    for ch in input.chars() {
        let c = match ch.to_ascii_uppercase() {
            '-' | ' ' => continue,
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        };
        if !CROCKFORD.contains(&(c as u8)) {
            return None;
        }
        out.push(c);
    }
    (out.len() == RECOVERY_GROUPS * RECOVERY_GROUP_LEN).then_some(out)
}

fn derive_key(secret: &[u8], salt: &[u8]) -> Result<[u8; 32], BackupKeyError> {
    let params = Params::new(64 * 1024, 3, 1, Some(32)).map_err(|_| BackupKeyError::Derive)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon
        .hash_password_into(secret, salt, &mut key)
        .map_err(|_| BackupKeyError::Derive)?;
    Ok(key)
}

fn wrap(dek: &[u8; 32], secret: &[u8]) -> Result<WrappedKey, BackupKeyError> {
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut nonce);
    let kek = derive_key(secret, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&kek).map_err(|_| BackupKeyError::Derive)?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), dek.as_slice())
        .map_err(|_| BackupKeyError::Derive)?;
    Ok(WrappedKey {
        salt: STANDARD.encode(salt),
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(ciphertext),
    })
}

fn unwrap(wrapped: &WrappedKey, secret: &[u8]) -> Result<[u8; 32], BackupKeyError> {
    let salt = STANDARD
        .decode(&wrapped.salt)
        .map_err(|_| BackupKeyError::Malformed)?;
    let nonce = STANDARD
        .decode(&wrapped.nonce)
        .map_err(|_| BackupKeyError::Malformed)?;
    let ciphertext = STANDARD
        .decode(&wrapped.ciphertext)
        .map_err(|_| BackupKeyError::Malformed)?;
    if nonce.len() != 12 {
        return Err(BackupKeyError::Malformed);
    }
    let kek = derive_key(secret, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&kek).map_err(|_| BackupKeyError::Derive)?;
    let plain = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_slice())
        .map_err(|_| BackupKeyError::Unwrap)?;
    plain.try_into().map_err(|_| BackupKeyError::Malformed)
}

/// Create the key set for a new install.
pub fn setup(backup_password: &str) -> Result<BackupSetup, BackupKeyError> {
    let mut dek = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut dek);
    let recovery_key = generate_recovery_key();
    let normalized = normalize_recovery_key(&recovery_key).expect("generated key is valid");
    let key_set = BackupKeySet {
        version: 1,
        by_password: wrap(&dek, backup_password.as_bytes())?,
        by_recovery_key: wrap(&dek, normalized.as_bytes())?,
    };
    Ok(BackupSetup {
        key_set,
        recovery_key,
    })
}

/// Recover the DEK with the backup password.
pub fn unlock_with_password(
    set: &BackupKeySet,
    password: &str,
) -> Result<[u8; 32], BackupKeyError> {
    unwrap(&set.by_password, password.as_bytes())
}

/// Recover the DEK with the recovery key (any formatting accepted).
pub fn unlock_with_recovery_key(set: &BackupKeySet, key: &str) -> Result<[u8; 32], BackupKeyError> {
    let normalized = normalize_recovery_key(key).ok_or(BackupKeyError::Unwrap)?;
    unwrap(&set.by_recovery_key, normalized.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_key_format() {
        let key = generate_recovery_key();
        assert_eq!(
            key.len(),
            RECOVERY_GROUPS * RECOVERY_GROUP_LEN + RECOVERY_GROUPS - 1
        );
        assert_eq!(key.split('-').count(), RECOVERY_GROUPS);
        assert!(key
            .chars()
            .all(|c| c == '-' || CROCKFORD.contains(&(c as u8))));
        assert_ne!(key, generate_recovery_key());
    }

    #[test]
    fn normalizes_user_input() {
        let key = "abcd-efgh-jkmn-pqrs-tvwx-yz01-2345-6789";
        let n = normalize_recovery_key(key).unwrap();
        assert_eq!(n, "ABCDEFGHJKMNPQRSTVWXYZ0123456789");
        // Crockford aliases: O→0, I/L→1; spaces ignored.
        assert_eq!(
            normalize_recovery_key("ABCD EFGH JKMN PQRS TVWX YZOI 2345 678L").unwrap(),
            "ABCDEFGHJKMNPQRSTVWXYZ0123456781"
        );
        assert!(normalize_recovery_key("ABCD").is_none());
        assert!(normalize_recovery_key("ABCD-EFGH-JKMN-PQRS-TVWX-YZ01-2345-678U").is_none());
    }

    #[test]
    fn both_secrets_unlock_the_same_dek_and_wrong_ones_fail() {
        let setup = setup("backup-password-123").unwrap();
        let a = unlock_with_password(&setup.key_set, "backup-password-123").unwrap();
        let b = unlock_with_recovery_key(&setup.key_set, &setup.recovery_key).unwrap();
        assert_eq!(a, b);
        let lower = setup.recovery_key.to_lowercase().replace('-', " ");
        assert_eq!(unlock_with_recovery_key(&setup.key_set, &lower).unwrap(), a);
        assert!(matches!(
            unlock_with_password(&setup.key_set, "wrong"),
            Err(BackupKeyError::Unwrap)
        ));
        assert!(unlock_with_recovery_key(&setup.key_set, "nope").is_err());
    }

    #[test]
    fn key_set_serializes_without_secrets() {
        let setup = setup("backup-password-123").unwrap();
        let json = serde_json::to_string(&setup.key_set).unwrap();
        assert!(!json.contains("backup-password-123"));
        assert!(!json.contains(&setup.recovery_key));
        let back: BackupKeySet = serde_json::from_str(&json).unwrap();
        assert_eq!(back, setup.key_set);
        let mut tampered = back.clone();
        tampered.by_password.ciphertext = "AAAA".into();
        assert!(unlock_with_password(&tampered, "backup-password-123").is_err());
    }
}
