//! Parent app rules shared by the school and the hub (docs/adr/0006-parent-app.md): what a
//! notice or a message may contain, and when a notice is shown to a parent. Pure.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Longest message a parent or a teacher can send.
pub const MAX_MESSAGE_CHARS: usize = 2000;
pub const MAX_NOTICE_TITLE_CHARS: usize = 200;
pub const MAX_NOTICE_BODY_CHARS: usize = 5000;

/// Who wrote a message, or whose "read up to" mark it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Guardian,
    Staff,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Guardian => "guardian",
            Side::Staff => "staff",
        }
    }
}

/// Line endings become `\n`; other control characters are refused.
fn tidy(text: &str, what: &str) -> Result<String, String> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(format!("{what} contains characters that cannot be used."));
    }
    Ok(text.trim().to_string())
}

/// A message as it will be stored, or why it cannot be sent.
pub fn clean_message(body: &str) -> Result<String, String> {
    let body = tidy(body, "The message")?;
    if body.is_empty() {
        return Err("Write a message first.".into());
    }
    if body.chars().count() > MAX_MESSAGE_CHARS {
        return Err(format!(
            "The message is too long (at most {MAX_MESSAGE_CHARS} characters)."
        ));
    }
    Ok(body)
}

/// A notice's title and text as they will be stored, or why they cannot be.
pub fn clean_notice(title: &str, body: &str) -> Result<(String, String), String> {
    let title = tidy(title, "The title")?.replace('\n', " ");
    if title.is_empty() {
        return Err("A notice needs a title.".into());
    }
    if title.chars().count() > MAX_NOTICE_TITLE_CHARS {
        return Err(format!(
            "The title is too long (at most {MAX_NOTICE_TITLE_CHARS} characters)."
        ));
    }
    let body = tidy(body, "The notice")?;
    if body.chars().count() > MAX_NOTICE_BODY_CHARS {
        return Err(format!(
            "The notice is too long (at most {MAX_NOTICE_BODY_CHARS} characters)."
        ));
    }
    Ok((title, body))
}

/// Parents see a notice from the moment it is published until the end of its last day.
pub fn notice_is_shown(
    published_at: Option<DateTime<Utc>>,
    expires_on: Option<NaiveDate>,
    now: DateTime<Utc>,
    today: NaiveDate,
) -> bool {
    published_at.is_some_and(|at| at <= now) && expires_on.is_none_or(|last| today <= last)
}

/// A notice for no particular class is for the whole school.
pub fn notice_is_for(class_ids: &[Uuid], class: Option<Uuid>) -> bool {
    class_ids.is_empty() || class.is_some_and(|c| class_ids.contains(&c))
}

/// The start of a message, for a list of conversations.
pub fn preview(body: &str, max_chars: usize) -> String {
    let one_line = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max_chars {
        return one_line;
    }
    let mut cut: String = one_line.chars().take(max_chars.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

// ---- parent accounts at the hub ------------------------------------------------------------

/// Characters of an access code: Crockford base32 (no I, L, O or U), easy to read off a
/// printed slip and to type on a phone.
const CODE_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// An access code is 12 characters (60 random bits) in three groups: `ABCD-EFGH-JKMN`.
pub const ACCESS_CODE_CHARS: usize = 12;
/// Days an access code works when it is not used.
pub const ACCESS_CODE_DAYS: i64 = 14;

/// A new one-time access code for a guardian.
pub fn generate_access_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut code = String::with_capacity(ACCESS_CODE_CHARS + 2);
    for i in 0..ACCESS_CODE_CHARS {
        if i > 0 && i % 4 == 0 {
            code.push('-');
        }
        code.push(CODE_ALPHABET[rng.gen_range(0..32)] as char);
    }
    code
}

/// An access code as typed, in the one form it is hashed in: capitals, no separators, the
/// letters people confuse read as digits. `None` when it cannot be an access code.
pub fn normalize_access_code(input: &str) -> Option<String> {
    let mut code = String::with_capacity(ACCESS_CODE_CHARS);
    for c in input.chars() {
        if c == '-' || c.is_whitespace() {
            continue;
        }
        let c = match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        };
        if !CODE_ALPHABET.contains(&(c as u8)) || !c.is_ascii() {
            return None;
        }
        code.push(c);
    }
    (code.len() == ACCESS_CODE_CHARS).then_some(code)
}

/// A phone number as an account name: digits only, with a leading `+` kept. Spaces, dashes
/// and brackets are dropped. A parent signs in with the number in the same form they chose
/// (`0300 1234567` and `+92 300 1234567` are different names).
pub fn normalize_phone(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    let plus = trimmed.starts_with('+');
    let mut digits = String::new();
    for c in trimmed.chars().skip(usize::from(plus)) {
        match c {
            '0'..='9' => digits.push(c),
            ' ' | '-' | '(' | ')' | '.' => {}
            _ => return Err("Enter the phone number in digits.".into()),
        }
    }
    if !(7..=15).contains(&digits.len()) {
        return Err("Enter the whole phone number (7 to 15 digits).".into());
    }
    Ok(if plus { format!("+{digits}") } else { digits })
}

/// A phone number with all but its last four digits hidden, for lists.
pub fn mask_phone(phone: &str) -> String {
    let digits: Vec<char> = phone.chars().filter(char::is_ascii_digit).collect();
    let tail: String = digits[digits.len().saturating_sub(4)..].iter().collect();
    format!("•••• {tail}")
}

/// What the school asks the hub for: a one-time access code for one guardian.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessCodeRequest {
    pub guardian_id: Uuid,
}

/// The hub's answer. The code is shown once; the hub keeps only its hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessCode {
    pub code: String,
    pub expires_at: DateTime<Utc>,
    pub guardian_name: String,
}

/// One guardian's access to the parent app, as the hub tells the school.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardianAccess {
    pub guardian_id: Uuid,
    /// `active`: an account is linked. `code`: a code was issued and not used yet.
    pub state: String,
    /// The account's phone number with all but the last four digits hidden.
    pub phone_hint: Option<String>,
    /// When the account was linked.
    pub since: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub code_expires_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn access_codes_are_short_typable_and_forgiving() {
        let code = generate_access_code();
        assert_eq!(code.len(), 14, "{code}");
        assert_eq!(code.split('-').map(str::len).collect::<Vec<_>>(), [4, 4, 4]);
        let plain = normalize_access_code(&code).unwrap();
        assert_eq!(plain.len(), ACCESS_CODE_CHARS);
        // Lower case, spaces, and O / I / L typed for 0 / 1 are all the same code.
        assert_eq!(
            normalize_access_code(" abcd efgh jkmn ").as_deref(),
            Some("ABCDEFGHJKMN")
        );
        assert_eq!(
            normalize_access_code("OIL0-1111-2222").as_deref(),
            Some("011011112222")
        );
        assert_eq!(normalize_access_code("ABCD-EFGH"), None, "too short");
        assert_eq!(
            normalize_access_code("ABCD-EFGH-JKMN-PQRS"),
            None,
            "too long"
        );
        assert_eq!(normalize_access_code("ABCD-EFGH-JKM!"), None);
        assert_eq!(
            normalize_access_code("ABCD-EFGH-JKMU"),
            None,
            "U is not used"
        );
        assert_ne!(generate_access_code(), generate_access_code());
    }

    #[test]
    fn phone_numbers_become_account_names() {
        assert_eq!(normalize_phone(" 0300-123 4567 ").unwrap(), "03001234567");
        assert_eq!(
            normalize_phone("+92 (300) 123.4567").unwrap(),
            "+923001234567"
        );
        assert!(normalize_phone("12345").is_err());
        assert!(normalize_phone("0300-CALL-ME").is_err());
        assert!(normalize_phone("1234567890123456").is_err());
        assert_eq!(mask_phone("+923001234567"), "•••• 4567");
        assert_eq!(mask_phone("123"), "•••• 123");
    }

    #[test]
    fn messages_are_trimmed_and_limited() {
        assert_eq!(
            clean_message("  Salaam,\r\nAli was unwell today.  ").unwrap(),
            "Salaam,\nAli was unwell today."
        );
        assert_eq!(clean_message(" \n ").unwrap_err(), "Write a message first.");
        assert!(clean_message(&"x".repeat(MAX_MESSAGE_CHARS)).is_ok());
        assert!(clean_message(&"x".repeat(MAX_MESSAGE_CHARS + 1)).is_err());
        // Urdu counts by character, not by byte.
        assert!(clean_message(&"ع".repeat(MAX_MESSAGE_CHARS)).is_ok());
        assert!(clean_message("bell\u{7}").is_err());
    }

    #[test]
    fn notices_need_a_title() {
        let (title, body) = clean_notice(" Sports day \n", " Friday.\r\nBring water. ").unwrap();
        assert_eq!(
            (title.as_str(), body.as_str()),
            ("Sports day", "Friday.\nBring water.")
        );
        assert_eq!(
            clean_notice("  ", "text").unwrap_err(),
            "A notice needs a title."
        );
        assert!(clean_notice(&"t".repeat(201), "").is_err());
        assert!(clean_notice("t", &"b".repeat(5001)).is_err());
        assert!(clean_notice("t", "").is_ok(), "a title alone is a notice");
    }

    #[test]
    fn a_notice_is_shown_from_publication_to_the_end_of_its_last_day() {
        let now = Utc.with_ymd_and_hms(2026, 11, 12, 9, 0, 0).unwrap();
        let today = now.date_naive();
        let before = now - chrono::Duration::hours(1);
        assert!(
            !notice_is_shown(None, None, now, today),
            "a draft is not shown"
        );
        assert!(notice_is_shown(Some(before), None, now, today));
        assert!(
            notice_is_shown(Some(before), Some(today), now, today),
            "its last day counts"
        );
        assert!(!notice_is_shown(
            Some(before),
            Some(today.pred_opt().unwrap()),
            now,
            today
        ));
        assert!(!notice_is_shown(
            Some(now + chrono::Duration::hours(1)),
            None,
            now,
            today
        ));
    }

    #[test]
    fn a_notice_for_chosen_classes_is_not_for_the_others() {
        let (five, six) = (Uuid::from_u128(5), Uuid::from_u128(6));
        assert!(notice_is_for(&[], Some(five)));
        assert!(
            notice_is_for(&[], None),
            "a whole-school notice reaches a child without a class"
        );
        assert!(notice_is_for(&[five, six], Some(six)));
        assert!(!notice_is_for(&[five], Some(six)));
        assert!(!notice_is_for(&[five], None));
    }

    #[test]
    fn previews_are_one_short_line() {
        assert_eq!(
            preview("Salaam,\nAli   was unwell", 40),
            "Salaam, Ali was unwell"
        );
        assert_eq!(preview("abcdefghij", 5), "abcd…");
        assert_eq!(preview("", 5), "");
    }
}
