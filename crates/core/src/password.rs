//! Password hashing (Argon2id) and the login lockout policy.

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::{Algorithm, Argon2, Params, Version};
use chrono::{DateTime, Duration, Utc};

/// Minimum password length accepted anywhere in Madrasa.
pub const MIN_PASSWORD_LEN: usize = 10;

#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("password must be at least {MIN_PASSWORD_LEN} characters")]
    TooShort,
    #[error("password hashing failed")]
    Hash,
}

fn argon2() -> Argon2<'static> {
    // OWASP 2023 recommendation: m=19 MiB, t=2, p=1.
    let params = Params::new(19 * 1024, 2, 1, None).expect("valid argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Validate the password policy without hashing.
pub fn validate_password(password: &str) -> Result<(), PasswordError> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(PasswordError::TooShort);
    }
    Ok(())
}

/// Hash a password into a PHC string (`$argon2id$...`).
pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    validate_password(password)?;
    let salt = SaltString::generate(&mut OsRng);
    argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| PasswordError::Hash)
}

/// Verify a password against a stored PHC string. Malformed hashes verify as `false`.
pub fn verify_password(password: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => argon2()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// Account lockout policy: after `max_attempts` consecutive failures the account is locked
/// for `lock_duration`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockoutPolicy {
    pub max_attempts: u32,
    pub lock_duration: Duration,
}

impl Default for LockoutPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            lock_duration: Duration::minutes(15),
        }
    }
}

/// Result of applying the policy after a failed attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailedAttempt {
    pub failed_count: u32,
    pub locked_until: Option<DateTime<Utc>>,
}

impl LockoutPolicy {
    /// Is the account locked at `now`?
    pub fn is_locked(&self, locked_until: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
        matches!(locked_until, Some(until) if until > now)
    }

    /// New counter and lock after one more failed attempt.
    pub fn after_failure(&self, previous_failed: u32, now: DateTime<Utc>) -> FailedAttempt {
        let failed_count = previous_failed.saturating_add(1);
        let locked_until = (failed_count >= self.max_attempts).then(|| now + self.lock_duration);
        FailedAttempt {
            failed_count,
            locked_until,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies() {
        let phc = hash_password("correct horse battery").unwrap();
        assert!(phc.starts_with("$argon2id$"));
        assert!(verify_password("correct horse battery", &phc));
        assert!(!verify_password("wrong horse battery", &phc));
        assert!(!verify_password("anything", "not-a-hash"));
    }

    #[test]
    fn hashes_are_salted() {
        let a = hash_password("same password!").unwrap();
        let b = hash_password("same password!").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn enforces_minimum_length() {
        assert!(matches!(
            hash_password("short"),
            Err(PasswordError::TooShort)
        ));
        assert!(validate_password("exactly10c").is_ok());
        // Counted in characters, not bytes.
        assert!(validate_password("پاسورڈپاسورڈ").is_ok());
    }

    #[test]
    fn lockout_after_five_failures_for_fifteen_minutes() {
        let policy = LockoutPolicy::default();
        let now = Utc::now();
        let mut state = FailedAttempt {
            failed_count: 0,
            locked_until: None,
        };
        for _ in 0..4 {
            state = policy.after_failure(state.failed_count, now);
            assert!(state.locked_until.is_none());
        }
        state = policy.after_failure(state.failed_count, now);
        assert_eq!(state.failed_count, 5);
        let until = state.locked_until.unwrap();
        assert_eq!(until, now + Duration::minutes(15));
        assert!(policy.is_locked(Some(until), now));
        assert!(policy.is_locked(Some(until), now + Duration::minutes(14)));
        assert!(!policy.is_locked(Some(until), now + Duration::minutes(15)));
        assert!(!policy.is_locked(None, now));
    }
}
