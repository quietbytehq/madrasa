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

/// A real hash of a password nobody has, made once with the same settings as every other
/// hash. Checking against it costs what checking a real account costs, so the time a login
/// takes does not tell whether the account exists. (Made here rather than written down, so it
/// always has the settings in use.)
pub fn dummy_hash() -> &'static str {
    static DUMMY: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        let mut bytes = [0u8; 24];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
        let nobody: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        hash_password(&nobody).expect("hashing a random password")
    });
    &DUMMY
}

/// Failed logins for names (usernames, phone numbers) that are not accounts.
///
/// A real account locks after five wrong passwords. If names that do not exist never locked,
/// the fifth answer would tell an attacker which names are real. So unknown names are counted
/// the same way, in memory, and answer "locked" on the same attempt and for as long.
#[derive(Debug, Default)]
pub struct UnknownLogins {
    seen: std::sync::Mutex<std::collections::HashMap<String, (u32, Option<DateTime<Utc>>)>>,
}

impl UnknownLogins {
    /// Never grows without bound: a flood of made-up names starts the count again.
    const MAX_NAMES: usize = 10_000;

    fn key(name: &str) -> String {
        name.trim().to_lowercase()
    }

    pub fn is_locked(&self, policy: &LockoutPolicy, name: &str, now: DateTime<Utc>) -> bool {
        let seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        seen.get(&Self::key(name))
            .is_some_and(|(_, until)| policy.is_locked(*until, now))
    }

    /// Count one more failure; `true` when this attempt locks the name.
    pub fn fail(&self, policy: &LockoutPolicy, name: &str, now: DateTime<Utc>) -> bool {
        let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        if seen.len() >= Self::MAX_NAMES {
            seen.clear();
        }
        let entry = seen.entry(Self::key(name)).or_insert((0, None));
        // A lock that has run out starts a new count; for a name that is nobody's, time is
        // the only way out.
        if entry.1.is_some_and(|until| until <= now) {
            *entry = (0, None);
        }
        let next = policy.after_failure(entry.0, now);
        *entry = (next.failed_count, next.locked_until);
        next.locked_until.is_some()
    }

    /// One failed attempt on a name that is not an account: check the password against the
    /// dummy hash (same cost as a real check) and say whether the answer is "locked".
    pub fn refuse(
        &self,
        policy: &LockoutPolicy,
        name: &str,
        password: &str,
        now: DateTime<Utc>,
    ) -> bool {
        if self.is_locked(policy, name, now) {
            return true;
        }
        let _ = verify_password(password, dummy_hash());
        self.fail(policy, name, now)
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

    #[test]
    fn the_dummy_hash_is_a_real_hash_with_the_usual_settings() {
        assert!(dummy_hash().starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(!verify_password("anything at all", dummy_hash()));
        assert!(PasswordHash::new(dummy_hash()).is_ok());
    }

    #[test]
    fn an_unknown_name_locks_on_the_same_attempt_as_a_real_account() {
        let policy = LockoutPolicy::default();
        let unknown = UnknownLogins::default();
        let now = Utc::now();
        for attempt in 1..policy.max_attempts {
            assert!(!unknown.fail(&policy, " Nobody ", now), "attempt {attempt}");
            assert!(!unknown.is_locked(&policy, "nobody", now));
        }
        assert!(unknown.fail(&policy, "NOBODY", now), "the fifth locks");
        assert!(unknown.is_locked(&policy, "nobody", now));
        assert!(
            unknown.refuse(&policy, "nobody", "x", now),
            "and stays locked"
        );
        assert!(!unknown.is_locked(&policy, "someone-else", now));
        // The lock runs out; the count starts again.
        let later = now + policy.lock_duration + Duration::seconds(1);
        assert!(!unknown.is_locked(&policy, "nobody", later));
        assert!(!unknown.fail(&policy, "nobody", later));
    }
}
