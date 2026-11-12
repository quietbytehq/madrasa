//! Parent app rules shared by the school and the hub (docs/adr/0006-parent-app.md): what a
//! notice or a message may contain, and when a notice is shown to a parent. Pure.

use chrono::{DateTime, NaiveDate, Utc};
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

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
