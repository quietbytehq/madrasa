//! Backup key escrow (ADR 0005 §6): a school may leave its backup data key with its Nexus
//! hub, so head office can help when both the backup password and the recovery key are lost.
//!
//! - The hub has an X25519 **escrow key pair**. Its private key is stored only wrapped by a
//!   key derived (Argon2id) from the **escrow passphrase**, which is never stored. A copy of
//!   the hub database therefore reveals no school's key.
//! - A school **seals** its data key to the hub's public key: ephemeral X25519, HKDF-SHA256,
//!   AES-256-GCM, the construction backup files already use. The school's id is bound into
//!   the key derivation and the authenticated data, so a deposit cannot be passed off as
//!   another school's.
//! - **Opening** a deposit needs the passphrase. The data key is then shown once as a
//!   printable code, which restore accepts as a third way to open a backup.
//!
//! Audited crates only (`x25519-dalek`, `hkdf`, `aes-gcm`, `argon2`, `sha2`); nothing here is
//! hand-rolled cryptography.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::{DateTime, Utc};
use hkdf::Hkdf;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};

use crate::backup::{self, archive, BackupKeyError, WrappedKey};

const INFO: &[u8] = b"madrasa escrow deposit v1";
const CHECK_INFO: &[u8] = b"madrasa released key v1";
/// Crockford base32: no I, L, O or U, so a code read aloud or copied by hand survives.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// A released key is the 32-byte data key and a 2-byte check, as 55 characters in 11 groups.
const RELEASED_BYTES: usize = 34;
const RELEASED_CHARS: usize = 55;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EscrowError {
    #[error("the escrow passphrase is incorrect")]
    Passphrase,
    #[error("the deposit is damaged or belongs to another school")]
    Tampered,
    #[error("the escrow data is malformed")]
    Malformed,
    #[error("the key could not be sealed")]
    Crypto,
}

/// The hub's escrow key pair as stored: the public key in clear, the private key wrapped by
/// the passphrase. Safe to keep in the hub's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscrowKeys {
    pub version: u8,
    /// X25519 public key (base64). Given to schools.
    pub public_key: String,
    /// X25519 private key, wrapped by a key derived from the escrow passphrase.
    pub private_key: WrappedKey,
}

/// A school's backup data key sealed to the hub's public key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deposit {
    pub version: u8,
    pub ephemeral_public: String,
    pub nonce: String,
    pub ciphertext: String,
    /// Which key this is: the public key derived from the data key (already public in every
    /// backup file). Lets either side see whether a deposit is current without opening it.
    pub key_id: String,
}

fn decode32(text: &str) -> Result<[u8; 32], EscrowError> {
    STANDARD
        .decode(text)
        .ok()
        .and_then(|v| v.try_into().ok())
        .ok_or(EscrowError::Malformed)
}

/// Create the hub's key pair, the private key wrapped by `passphrase`.
pub fn generate(passphrase: &str) -> Result<EscrowKeys, EscrowError> {
    let mut private = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut private);
    let public = PublicKey::from(&StaticSecret::from(private));
    let keys = EscrowKeys {
        version: 1,
        public_key: STANDARD.encode(public.as_bytes()),
        private_key: backup::wrap(&private, passphrase.as_bytes())
            .map_err(|_| EscrowError::Crypto)?,
    };
    private.fill(0);
    Ok(keys)
}

fn private_key(keys: &EscrowKeys, passphrase: &str) -> Result<[u8; 32], EscrowError> {
    backup::unwrap(&keys.private_key, passphrase.as_bytes()).map_err(|e| match e {
        BackupKeyError::Unwrap => EscrowError::Passphrase,
        _ => EscrowError::Malformed,
    })
}

/// Is this the escrow passphrase?
pub fn check_passphrase(keys: &EscrowKeys, passphrase: &str) -> bool {
    private_key(keys, passphrase).is_ok()
}

/// Wrap the same private key with a new passphrase. Existing deposits keep working.
pub fn change_passphrase(
    keys: &EscrowKeys,
    current: &str,
    new: &str,
) -> Result<EscrowKeys, EscrowError> {
    let mut private = private_key(keys, current)?;
    let wrapped = backup::wrap(&private, new.as_bytes()).map_err(|_| EscrowError::Crypto);
    private.fill(0);
    Ok(EscrowKeys {
        version: keys.version,
        public_key: keys.public_key.clone(),
        private_key: wrapped?,
    })
}

/// The id of a data key: the public key derived from it (base64).
pub fn key_id(dek: &[u8; 32]) -> String {
    STANDARD.encode(archive::public_key_for(dek))
}

fn seal_key(shared: &[u8], ephemeral: &[u8; 32], recipient: &[u8; 32], context: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, shared);
    let mut info = Vec::with_capacity(INFO.len() + 64 + context.len());
    info.extend_from_slice(INFO);
    info.extend_from_slice(ephemeral);
    info.extend_from_slice(recipient);
    info.extend_from_slice(context);
    let mut out = [0u8; 32];
    hk.expand(&info, &mut out)
        .expect("32 bytes is a valid HKDF length");
    out
}

/// Seal a school's data key to the hub's escrow public key. `context` is the school's id:
/// the deposit opens only when the same context is given.
pub fn seal(
    escrow_public_key: &str,
    dek: &[u8; 32],
    context: &[u8],
) -> Result<Deposit, EscrowError> {
    let recipient = decode32(escrow_public_key)?;
    let ephemeral = EphemeralSecret::random_from_rng(rand::thread_rng());
    let ephemeral_public = PublicKey::from(&ephemeral).to_bytes();
    let shared = ephemeral.diffie_hellman(&PublicKey::from(recipient));
    let key = seal_key(shared.as_bytes(), &ephemeral_public, &recipient, context);
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ciphertext = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| EscrowError::Crypto)?
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: dek.as_slice(),
                aad: context,
            },
        )
        .map_err(|_| EscrowError::Crypto)?;
    Ok(Deposit {
        version: 1,
        ephemeral_public: STANDARD.encode(ephemeral_public),
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(ciphertext),
        key_id: key_id(dek),
    })
}

/// Open a deposit with the escrow passphrase. Fails for a wrong passphrase, a changed
/// deposit, or a deposit made for another school.
pub fn open(
    keys: &EscrowKeys,
    passphrase: &str,
    deposit: &Deposit,
    context: &[u8],
) -> Result<[u8; 32], EscrowError> {
    let mut private = private_key(keys, passphrase)?;
    let secret = StaticSecret::from(private);
    private.fill(0);
    let recipient = PublicKey::from(&secret).to_bytes();
    let ephemeral = decode32(&deposit.ephemeral_public)?;
    let nonce: [u8; 12] = STANDARD
        .decode(&deposit.nonce)
        .ok()
        .and_then(|v| v.try_into().ok())
        .ok_or(EscrowError::Malformed)?;
    let ciphertext = STANDARD
        .decode(&deposit.ciphertext)
        .map_err(|_| EscrowError::Malformed)?;
    let shared = secret.diffie_hellman(&PublicKey::from(ephemeral));
    let key = seal_key(shared.as_bytes(), &ephemeral, &recipient, context);
    let plain = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| EscrowError::Crypto)?
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: ciphertext.as_slice(),
                aad: context,
            },
        )
        .map_err(|_| EscrowError::Tampered)?;
    let dek: [u8; 32] = plain.try_into().map_err(|_| EscrowError::Tampered)?;
    // The deposit says which key it holds; a mismatch means it was altered.
    if key_id(&dek) != deposit.key_id {
        return Err(EscrowError::Tampered);
    }
    Ok(dek)
}

fn check_bytes(dek: &[u8; 32]) -> [u8; 2] {
    let mut h = Sha256::new();
    h.update(CHECK_INFO);
    h.update(dek);
    let digest = h.finalize();
    [digest[0], digest[1]]
}

/// A released data key as a printable code: 11 groups of 5 characters, with a check so a
/// mistyped character is caught before a restore is tried.
pub fn format_released_key(dek: &[u8; 32]) -> String {
    let mut bytes = [0u8; RELEASED_BYTES];
    bytes[..32].copy_from_slice(dek);
    bytes[32..].copy_from_slice(&check_bytes(dek));
    let mut chars = String::with_capacity(RELEASED_CHARS + 10);
    let (mut buffer, mut bits) = (0u32, 0u32);
    let mut written = 0;
    let mut push = |value: u32, chars: &mut String| {
        if written > 0 && written % 5 == 0 {
            chars.push('-');
        }
        chars.push(ALPHABET[value as usize] as char);
        written += 1;
    };
    for byte in bytes {
        buffer = (buffer << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            push((buffer >> bits) & 31, &mut chars);
        }
    }
    if bits > 0 {
        push((buffer << (5 - bits)) & 31, &mut chars);
    }
    bytes.fill(0);
    chars
}

/// Read a released key back. Spaces, dashes and case do not matter; O is read as 0, I and L
/// as 1. `None` when it is not a released key or a character is wrong.
pub fn parse_released_key(text: &str) -> Option<[u8; 32]> {
    let mut bytes = Vec::with_capacity(RELEASED_BYTES + 1);
    let (mut buffer, mut bits, mut count) = (0u32, 0u32, 0usize);
    for c in text.chars() {
        if c == '-' || c.is_whitespace() {
            continue;
        }
        let c = match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        };
        let value = ALPHABET.iter().position(|a| *a as char == c)? as u32;
        count += 1;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            bytes.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    if count != RELEASED_CHARS || bytes.len() != RELEASED_BYTES {
        return None;
    }
    let dek: [u8; 32] = bytes[..32].try_into().ok()?;
    if bytes[32..] != check_bytes(&dek) {
        return None;
    }
    Some(dek)
}

/// Fewest characters in an escrow passphrase. It protects every school's key at once, so it
/// is longer than an ordinary password.
pub const MIN_PASSPHRASE_CHARS: usize = 12;

/// Why a passphrase cannot be used, or `None` when it can.
pub fn passphrase_problem(passphrase: &str) -> Option<String> {
    if passphrase.chars().count() < MIN_PASSPHRASE_CHARS {
        return Some(format!(
            "The escrow passphrase must be at least {MIN_PASSPHRASE_CHARS} characters."
        ));
    }
    if passphrase.trim() != passphrase {
        return Some("The escrow passphrase cannot start or end with a space.".to_string());
    }
    None
}

/// The context a school's deposit is bound to: the school's own id.
pub fn context_for(school_id: Uuid) -> Vec<u8> {
    school_id.to_string().into_bytes()
}

// ---- what the school and the hub say to each other -------------------------------------------

/// What the hub tells a school about escrow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscrowStatus {
    /// The hub's escrow public key; `None` while head office has not set escrow up.
    pub public_key: Option<String>,
    /// The key this school has left with head office, if any.
    pub deposit: Option<DepositInfo>,
    /// Every time head office released this school's key, newest first.
    pub releases: Vec<ReleaseInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepositInfo {
    pub key_id: String,
    pub deposited_at: DateTime<Utc>,
}

/// One release of a school's key: who, when and why. Never the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub id: Uuid,
    pub released_at: DateTime<Utc>,
    /// The name of the head office user who released it.
    pub released_by: String,
    pub reason: String,
    pub key_id: String,
    /// The school has confirmed it recorded this release.
    pub seen: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositRequest {
    pub deposit: Deposit,
}

/// The school has recorded these releases in its own audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeenRequest {
    pub ids: Vec<Uuid>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHOOL: &[u8] = b"school-0001";

    fn dek() -> [u8; 32] {
        let mut d = [0u8; 32];
        for (i, b) in d.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7).wrapping_add(3);
        }
        d
    }

    #[test]
    fn a_deposit_opens_only_with_the_passphrase() {
        let keys = generate("escrow-passphrase-1").unwrap();
        let deposit = seal(&keys.public_key, &dek(), SCHOOL).unwrap();
        assert_eq!(
            open(&keys, "escrow-passphrase-1", &deposit, SCHOOL).unwrap(),
            dek()
        );
        assert_eq!(
            open(&keys, "wrong-passphrase", &deposit, SCHOOL),
            Err(EscrowError::Passphrase)
        );
        assert!(check_passphrase(&keys, "escrow-passphrase-1"));
        assert!(!check_passphrase(&keys, "escrow-passphrase-2"));
        // Nothing stored reveals the data key or the private key.
        let stored = serde_json::to_string(&(&keys, &deposit)).unwrap();
        assert!(!stored.contains(&STANDARD.encode(dek())));
        assert_eq!(deposit.key_id, key_id(&dek()));
    }

    #[test]
    fn a_deposit_is_bound_to_its_school_and_detects_tampering() {
        let keys = generate("escrow-passphrase-1").unwrap();
        let deposit = seal(&keys.public_key, &dek(), SCHOOL).unwrap();
        assert_eq!(
            open(&keys, "escrow-passphrase-1", &deposit, b"school-0002"),
            Err(EscrowError::Tampered),
            "a deposit cannot be passed off as another school's"
        );
        let mut changed = deposit.clone();
        let mut raw = STANDARD.decode(&changed.ciphertext).unwrap();
        raw[0] ^= 1;
        changed.ciphertext = STANDARD.encode(raw);
        assert_eq!(
            open(&keys, "escrow-passphrase-1", &changed, SCHOOL),
            Err(EscrowError::Tampered)
        );
        let mut relabelled = deposit.clone();
        relabelled.key_id = key_id(&[9u8; 32]);
        assert_eq!(
            open(&keys, "escrow-passphrase-1", &relabelled, SCHOOL),
            Err(EscrowError::Tampered)
        );
        let mut broken = deposit;
        broken.nonce = "not base64".into();
        assert_eq!(
            open(&keys, "escrow-passphrase-1", &broken, SCHOOL),
            Err(EscrowError::Malformed)
        );
        // Another hub's key pair cannot open it.
        let other = generate("escrow-passphrase-1").unwrap();
        let deposit = seal(&keys.public_key, &dek(), SCHOOL).unwrap();
        assert_eq!(
            open(&other, "escrow-passphrase-1", &deposit, SCHOOL),
            Err(EscrowError::Tampered)
        );
        assert_eq!(seal("short", &dek(), SCHOOL), Err(EscrowError::Malformed));
    }

    #[test]
    fn passphrases_must_be_long() {
        assert!(passphrase_problem("short").is_some());
        assert!(passphrase_problem("eleven-char").is_some());
        assert!(passphrase_problem(" padded-passphrase ").is_some());
        assert_eq!(passphrase_problem("twelve-chars"), None);
        assert_eq!(
            context_for(Uuid::nil()),
            b"00000000-0000-0000-0000-000000000000"
        );
    }

    #[test]
    fn two_deposits_of_one_key_differ_and_both_open() {
        let keys = generate("escrow-passphrase-1").unwrap();
        let a = seal(&keys.public_key, &dek(), SCHOOL).unwrap();
        let b = seal(&keys.public_key, &dek(), SCHOOL).unwrap();
        assert_ne!(a.ciphertext, b.ciphertext);
        assert_ne!(a.ephemeral_public, b.ephemeral_public);
        assert_eq!(
            open(&keys, "escrow-passphrase-1", &a, SCHOOL),
            open(&keys, "escrow-passphrase-1", &b, SCHOOL)
        );
    }

    #[test]
    fn changing_the_passphrase_keeps_existing_deposits_openable() {
        let keys = generate("escrow-passphrase-1").unwrap();
        let deposit = seal(&keys.public_key, &dek(), SCHOOL).unwrap();
        assert_eq!(
            change_passphrase(&keys, "wrong", "new-passphrase-22").unwrap_err(),
            EscrowError::Passphrase
        );
        let changed = change_passphrase(&keys, "escrow-passphrase-1", "new-passphrase-22").unwrap();
        assert_eq!(changed.public_key, keys.public_key);
        assert_eq!(
            open(&changed, "new-passphrase-22", &deposit, SCHOOL).unwrap(),
            dek()
        );
        assert_eq!(
            open(&changed, "escrow-passphrase-1", &deposit, SCHOOL),
            Err(EscrowError::Passphrase)
        );
    }

    #[test]
    fn a_released_key_prints_reads_back_and_catches_typing_mistakes() {
        let code = format_released_key(&dek());
        assert_eq!(
            code.len(),
            65,
            "eleven groups of five and ten dashes: {code}"
        );
        assert_eq!(code.split('-').count(), 11);
        assert!(code.split('-').all(|g| g.len() == 5));
        assert_eq!(parse_released_key(&code), Some(dek()));
        // Lower case, spaces, and the letters people confuse are all accepted.
        let sloppy = code
            .to_lowercase()
            .replace('-', " ")
            .replace('0', "o")
            .replace('1', "l");
        assert_eq!(parse_released_key(&sloppy), Some(dek()));
        // One wrong character is caught by the check, not discovered at restore time.
        let mut wrong: Vec<char> = code.chars().collect();
        wrong[0] = if wrong[0] == 'A' { 'B' } else { 'A' };
        assert_eq!(
            parse_released_key(&wrong.into_iter().collect::<String>()),
            None
        );
        assert_eq!(parse_released_key(&code[..60]), None);
        assert_eq!(
            parse_released_key("ABCD-EFGH-JKMN-PQRS-TVWX-YZ12-3456-789A"),
            None,
            "a recovery key is not a released key"
        );
        assert_eq!(parse_released_key(""), None);
        assert_eq!(parse_released_key(&format!("{code}U")), None);
        // Every data key round-trips, whatever its bits.
        for seed in [0u8, 255, 0x55, 0xaa] {
            let k = [seed; 32];
            assert_eq!(parse_released_key(&format_released_key(&k)), Some(k));
        }
    }
}
