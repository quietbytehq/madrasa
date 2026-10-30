//! Session tokens: 32 random bytes, base64url for the client; the server stores only the
//! SHA-256 hash so a database leak does not leak live sessions.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Duration;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Default session lifetime (sliding).
pub fn default_lifetime() -> Duration {
    Duration::hours(12)
}

/// A freshly generated token and its storable hash.
#[derive(Debug, Clone)]
pub struct NewToken {
    /// Give this to the client exactly once.
    pub token: String,
    /// Store this.
    pub hash: String,
}

pub fn generate() -> NewToken {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash(&token);
    NewToken { token, hash }
}

/// Hex SHA-256 of the token as presented by the client.
pub fn hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_and_hash_deterministically() {
        let a = generate();
        let b = generate();
        assert_ne!(a.token, b.token);
        assert_eq!(a.token.len(), 43); // 32 bytes base64url without padding
        assert_eq!(a.hash.len(), 64);
        assert_eq!(hash(&a.token), a.hash);
        assert_ne!(a.hash, b.hash);
    }
}
