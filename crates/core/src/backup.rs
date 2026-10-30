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
    /// X25519 public key derived from the DEK (base64). Lets unattended backups encrypt a
    /// per-file key without knowing the password; absent on key sets written before M1
    /// stage 5 until the password is entered once.
    #[serde(default)]
    pub public_key: Option<String>,
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
        version: 2,
        by_password: wrap(&dek, backup_password.as_bytes())?,
        by_recovery_key: wrap(&dek, normalized.as_bytes())?,
        public_key: Some(STANDARD.encode(archive::public_key_for(&dek))),
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

/// Re-wrap the DEK with a new password. The recovery key keeps working; backups made
/// before the change still open with the old password (their file carries the old key set).
pub fn change_password(
    set: &BackupKeySet,
    dek: &[u8; 32],
    new_password: &str,
) -> Result<BackupKeySet, BackupKeyError> {
    Ok(BackupKeySet {
        version: 2,
        by_password: wrap(dek, new_password.as_bytes())?,
        by_recovery_key: set.by_recovery_key.clone(),
        public_key: Some(STANDARD.encode(archive::public_key_for(dek))),
    })
}

/// Fill in the public key of a key set written before it existed (needs the unlocked DEK).
pub fn with_public_key(set: &BackupKeySet, dek: &[u8; 32]) -> BackupKeySet {
    BackupKeySet {
        version: 2,
        public_key: Some(STANDARD.encode(archive::public_key_for(dek))),
        ..set.clone()
    }
}

/// Encrypted, signed backup files (`.mbk`).
///
/// Layout: magic, big-endian header length, JSON header, AES-256-GCM payload (gzip of the
/// database dump) with the header bytes as associated data, and a SHA-256 trailer over
/// everything before it. The header is readable without a key (restore preview); any change
/// to the header or payload makes decryption fail ("tampered").
///
/// The payload key is random per file and wrapped for the school's X25519 public key
/// (ephemeral Diffie–Hellman, HKDF-SHA256, AES-256-GCM), so scheduled backups never need the
/// password. The header also carries the wrapped DEK (by password and by recovery key), so a
/// file restores on a fresh machine with either secret.
pub mod archive {
    use super::{BackupKeyError, BackupKeySet, STANDARD};
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    use aes_gcm::{Aes256Gcm, Nonce};
    use base64::Engine;
    use chrono::{DateTime, Utc};
    use flate2::read::GzDecoder;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use hkdf::Hkdf;
    use rand::RngCore;
    use serde::{Deserialize, Serialize};
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};

    pub const MAGIC: &[u8] = b"MADRASABK1\n";
    pub const EXTENSION: &str = "mbk";
    const INFO: &[u8] = b"madrasa backup file key v1";

    #[derive(Debug, thiserror::Error, PartialEq, Eq)]
    pub enum ArchiveError {
        #[error("this is not a Madrasa backup file")]
        NotABackup,
        #[error("the backup file is damaged or has been tampered with")]
        Tampered,
        #[error("the password or recovery key is incorrect")]
        WrongKey,
        #[error("automatic backups need the backup password entered once on this installation")]
        NoPublicKey,
        #[error("the backup could not be written")]
        Encrypt,
    }

    impl From<BackupKeyError> for ArchiveError {
        fn from(e: BackupKeyError) -> Self {
            match e {
                BackupKeyError::Unwrap => ArchiveError::WrongKey,
                _ => ArchiveError::Tampered,
            }
        }
    }

    /// Plain-language facts about a backup, readable without a key.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct BackupMeta {
        pub created_at: DateTime<Utc>,
        pub app_version: String,
        pub school_id: uuid::Uuid,
        pub school_name: String,
        pub school_code: String,
        /// Applied migration versions; a restore needs the same schema.
        pub migrations: Vec<i64>,
        pub counts: BTreeMap<String, i64>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Header {
        version: u8,
        meta: BackupMeta,
        key_set: BackupKeySet,
        ephemeral_public: String,
        file_key_nonce: String,
        file_key_wrapped: String,
        payload_nonce: String,
    }

    pub fn public_key_for(dek: &[u8; 32]) -> [u8; 32] {
        PublicKey::from(&StaticSecret::from(*dek)).to_bytes()
    }

    fn wrap_key(shared: &[u8], eph_pub: &[u8; 32], recipient: &[u8; 32]) -> [u8; 32] {
        let hk = Hkdf::<Sha256>::new(None, shared);
        let mut info = Vec::with_capacity(INFO.len() + 64);
        info.extend_from_slice(INFO);
        info.extend_from_slice(eph_pub);
        info.extend_from_slice(recipient);
        let mut out = [0u8; 32];
        hk.expand(&info, &mut out)
            .expect("32 bytes is a valid HKDF length");
        out
    }

    fn decode32(s: &str) -> Result<[u8; 32], ArchiveError> {
        STANDARD
            .decode(s)
            .ok()
            .and_then(|v| v.try_into().ok())
            .ok_or(ArchiveError::Tampered)
    }

    fn decode12(s: &str) -> Result<[u8; 12], ArchiveError> {
        STANDARD
            .decode(s)
            .ok()
            .and_then(|v| v.try_into().ok())
            .ok_or(ArchiveError::Tampered)
    }

    /// Encrypt and sign a dump for the school whose key set is given.
    pub fn seal(
        meta: &BackupMeta,
        key_set: &BackupKeySet,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, ArchiveError> {
        let recipient = decode32(
            key_set
                .public_key
                .as_deref()
                .ok_or(ArchiveError::NoPublicKey)?,
        )
        .map_err(|_| ArchiveError::NoPublicKey)?;
        let mut file_key = [0u8; 32];
        let mut fk_nonce = [0u8; 12];
        let mut payload_nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut file_key);
        rand::thread_rng().fill_bytes(&mut fk_nonce);
        rand::thread_rng().fill_bytes(&mut payload_nonce);
        let eph = EphemeralSecret::random_from_rng(rand::thread_rng());
        let eph_pub = PublicKey::from(&eph).to_bytes();
        let shared = eph.diffie_hellman(&PublicKey::from(recipient));
        let kek = wrap_key(shared.as_bytes(), &eph_pub, &recipient);
        let wrapped = Aes256Gcm::new_from_slice(&kek)
            .map_err(|_| ArchiveError::Encrypt)?
            .encrypt(Nonce::from_slice(&fk_nonce), file_key.as_slice())
            .map_err(|_| ArchiveError::Encrypt)?;
        let header = Header {
            version: 1,
            meta: meta.clone(),
            key_set: key_set.clone(),
            ephemeral_public: STANDARD.encode(eph_pub),
            file_key_nonce: STANDARD.encode(fk_nonce),
            file_key_wrapped: STANDARD.encode(wrapped),
            payload_nonce: STANDARD.encode(payload_nonce),
        };
        let header_bytes = serde_json::to_vec(&header).map_err(|_| ArchiveError::Encrypt)?;
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(plaintext).map_err(|_| ArchiveError::Encrypt)?;
        let compressed = gz.finish().map_err(|_| ArchiveError::Encrypt)?;
        let ciphertext = Aes256Gcm::new_from_slice(&file_key)
            .map_err(|_| ArchiveError::Encrypt)?
            .encrypt(
                Nonce::from_slice(&payload_nonce),
                Payload {
                    msg: &compressed,
                    aad: &header_bytes,
                },
            )
            .map_err(|_| ArchiveError::Encrypt)?;
        let mut out =
            Vec::with_capacity(MAGIC.len() + 4 + header_bytes.len() + ciphertext.len() + 32);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&header_bytes);
        out.extend_from_slice(&ciphertext);
        let digest = Sha256::digest(&out);
        out.extend_from_slice(&digest);
        Ok(out)
    }

    fn split(bytes: &[u8]) -> Result<(Header, &[u8], &[u8]), ArchiveError> {
        if bytes.len() < MAGIC.len() + 4 + 32 || &bytes[..MAGIC.len()] != MAGIC {
            return Err(ArchiveError::NotABackup);
        }
        let body_len = bytes.len() - 32;
        let (body, trailer) = bytes.split_at(body_len);
        if Sha256::digest(body).as_slice() != trailer {
            return Err(ArchiveError::Tampered);
        }
        let len_start = MAGIC.len();
        let header_len = u32::from_be_bytes(
            body[len_start..len_start + 4]
                .try_into()
                .map_err(|_| ArchiveError::Tampered)?,
        ) as usize;
        let header_start = len_start + 4;
        if body.len() < header_start + header_len {
            return Err(ArchiveError::Tampered);
        }
        let header_bytes = &body[header_start..header_start + header_len];
        let header: Header =
            serde_json::from_slice(header_bytes).map_err(|_| ArchiveError::Tampered)?;
        Ok((header, header_bytes, &body[header_start + header_len..]))
    }

    /// Read the facts about a backup without a key. Verifies the checksum.
    pub fn read_meta(bytes: &[u8]) -> Result<(BackupMeta, BackupKeySet), ArchiveError> {
        let (header, _, _) = split(bytes)?;
        Ok((header.meta, header.key_set))
    }

    /// Decrypt a backup with the unlocked DEK of the key set it was written for.
    pub fn open(bytes: &[u8], dek: &[u8; 32]) -> Result<(BackupMeta, Vec<u8>), ArchiveError> {
        let (header, header_bytes, ciphertext) = split(bytes)?;
        let eph_pub = decode32(&header.ephemeral_public)?;
        let recipient = public_key_for(dek);
        let shared = StaticSecret::from(*dek).diffie_hellman(&PublicKey::from(eph_pub));
        let kek = wrap_key(shared.as_bytes(), &eph_pub, &recipient);
        let wrapped = STANDARD
            .decode(&header.file_key_wrapped)
            .map_err(|_| ArchiveError::Tampered)?;
        let file_key: [u8; 32] = Aes256Gcm::new_from_slice(&kek)
            .map_err(|_| ArchiveError::Tampered)?
            .decrypt(
                Nonce::from_slice(&decode12(&header.file_key_nonce)?),
                wrapped.as_slice(),
            )
            .map_err(|_| ArchiveError::WrongKey)?
            .try_into()
            .map_err(|_| ArchiveError::Tampered)?;
        let compressed = Aes256Gcm::new_from_slice(&file_key)
            .map_err(|_| ArchiveError::Tampered)?
            .decrypt(
                Nonce::from_slice(&decode12(&header.payload_nonce)?),
                Payload {
                    msg: ciphertext,
                    aad: header_bytes,
                },
            )
            .map_err(|_| ArchiveError::Tampered)?;
        let mut plain = Vec::new();
        GzDecoder::new(compressed.as_slice())
            .read_to_end(&mut plain)
            .map_err(|_| ArchiveError::Tampered)?;
        Ok((header.meta, plain))
    }

    /// File name for a new backup: `madrasa-<code>-<UTC timestamp>.mbk`.
    pub fn file_name(school_code: &str, at: DateTime<Utc>) -> String {
        let code: String = school_code
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        format!(
            "madrasa-{}-{}.{}",
            code.to_lowercase(),
            at.format("%Y%m%d-%H%M%S"),
            EXTENSION
        )
    }

    #[cfg(test)]
    mod tests {
        use super::super::{
            change_password, setup, unlock_with_password, unlock_with_recovery_key,
        };
        use super::*;

        fn meta() -> BackupMeta {
            BackupMeta {
                created_at: "2026-10-30T20:00:00Z".parse().unwrap(),
                app_version: "0.1.0".into(),
                school_id: uuid::Uuid::nil(),
                school_name: "Gulshan High".into(),
                school_code: "KHI-MC".into(),
                migrations: vec![1, 2, 3],
                counts: BTreeMap::from([("students".to_string(), 1200)]),
            }
        }

        #[test]
        fn round_trip_with_password_and_recovery_key() {
            let s = setup("backup-password-1").unwrap();
            let plain = b"COPY students ... lots of rows ...".repeat(200);
            let file = seal(&meta(), &s.key_set, &plain).unwrap();
            assert!(file.starts_with(MAGIC));
            assert!(file.len() < plain.len(), "payload is compressed");
            let (m, embedded) = read_meta(&file).unwrap();
            assert_eq!(m, meta());
            assert_eq!(embedded, s.key_set);
            let dek = unlock_with_password(&embedded, "backup-password-1").unwrap();
            let (m2, out) = open(&file, &dek).unwrap();
            assert_eq!((m2, out), (meta(), plain.clone()));
            let dek2 = unlock_with_recovery_key(&embedded, &s.recovery_key).unwrap();
            assert_eq!(open(&file, &dek2).unwrap().1, plain);
        }

        #[test]
        fn wrong_password_and_wrong_dek_fail_cleanly() {
            let s = setup("backup-password-1").unwrap();
            let file = seal(&meta(), &s.key_set, b"data").unwrap();
            let (_, embedded) = read_meta(&file).unwrap();
            assert!(unlock_with_password(&embedded, "nope-nope-nope").is_err());
            let other = setup("other-password-1").unwrap();
            let wrong_dek = unlock_with_password(&other.key_set, "other-password-1").unwrap();
            assert_eq!(open(&file, &wrong_dek).unwrap_err(), ArchiveError::WrongKey);
        }

        #[test]
        fn any_tampering_is_refused() {
            let s = setup("backup-password-1").unwrap();
            let dek = unlock_with_password(&s.key_set, "backup-password-1").unwrap();
            let file = seal(&meta(), &s.key_set, b"the data").unwrap();
            // Flip a payload byte, a header byte, the trailer; truncate; replace the magic.
            let payload_pos = file.len() - 40;
            for pos in [payload_pos, MAGIC.len() + 10, file.len() - 1] {
                let mut bad = file.clone();
                bad[pos] ^= 0x01;
                assert_eq!(
                    read_meta(&bad).unwrap_err(),
                    ArchiveError::Tampered,
                    "pos {pos}"
                );
                assert_eq!(
                    open(&bad, &dek).unwrap_err(),
                    ArchiveError::Tampered,
                    "pos {pos}"
                );
            }
            // Header edited and checksum recomputed: the AEAD tag still catches it.
            let (header, header_bytes, ciphertext) = split(&file).unwrap();
            let mut edited = serde_json::to_vec(&Header {
                meta: BackupMeta {
                    school_name: "Evil".into(),
                    ..header.meta
                },
                ..header
            })
            .unwrap();
            assert_ne!(edited, header_bytes);
            let mut forged = Vec::new();
            forged.extend_from_slice(MAGIC);
            forged.extend_from_slice(&(edited.len() as u32).to_be_bytes());
            forged.append(&mut edited);
            forged.extend_from_slice(ciphertext);
            let digest = Sha256::digest(&forged);
            forged.extend_from_slice(&digest);
            assert!(read_meta(&forged).is_ok(), "checksum alone passes");
            assert_eq!(open(&forged, &dek).unwrap_err(), ArchiveError::Tampered);
            assert_eq!(
                read_meta(&file[..20]).unwrap_err(),
                ArchiveError::NotABackup
            );
            assert_eq!(read_meta(b"hello").unwrap_err(), ArchiveError::NotABackup);
        }

        #[test]
        fn password_change_applies_to_new_backups_only() {
            let s = setup("old-password-1").unwrap();
            let dek = unlock_with_password(&s.key_set, "old-password-1").unwrap();
            let old_file = seal(&meta(), &s.key_set, b"old").unwrap();
            let new_set = change_password(&s.key_set, &dek, "new-password-1").unwrap();
            assert_eq!(
                new_set.public_key, s.key_set.public_key,
                "same DEK, same public key"
            );
            assert!(unlock_with_password(&new_set, "old-password-1").is_err());
            let dek_new = unlock_with_password(&new_set, "new-password-1").unwrap();
            assert_eq!(dek_new, dek);
            assert_eq!(
                unlock_with_recovery_key(&new_set, &s.recovery_key).unwrap(),
                dek
            );
            let new_file = seal(&meta(), &new_set, b"new").unwrap();
            // Old file still opens with the old password (its own key set), not the new one.
            let (_, old_set) = read_meta(&old_file).unwrap();
            assert!(unlock_with_password(&old_set, "new-password-1").is_err());
            assert_eq!(
                open(
                    &old_file,
                    &unlock_with_password(&old_set, "old-password-1").unwrap()
                )
                .unwrap()
                .1,
                b"old"
            );
            let (_, embedded_new) = read_meta(&new_file).unwrap();
            assert!(unlock_with_password(&embedded_new, "old-password-1").is_err());
            assert_eq!(
                open(
                    &new_file,
                    &unlock_with_password(&embedded_new, "new-password-1").unwrap()
                )
                .unwrap()
                .1,
                b"new"
            );
        }

        #[test]
        fn key_sets_without_a_public_key_cannot_seal_until_unlocked_once() {
            let s = setup("backup-password-1").unwrap();
            let legacy = BackupKeySet {
                version: 1,
                public_key: None,
                ..s.key_set.clone()
            };
            assert_eq!(
                seal(&meta(), &legacy, b"x").unwrap_err(),
                ArchiveError::NoPublicKey
            );
            let dek = unlock_with_password(&legacy, "backup-password-1").unwrap();
            let fixed = super::super::with_public_key(&legacy, &dek);
            assert_eq!(fixed.public_key, s.key_set.public_key);
            assert!(seal(&meta(), &fixed, b"x").is_ok());
            assert_eq!(
                file_name("KHI-MC", meta().created_at),
                "madrasa-khi-mc-20261030-200000.mbk"
            );
        }
    }
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
