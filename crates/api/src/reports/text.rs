//! Every fixed text that appears in a report, in English and Urdu. Keys are stable strings
//! used by the catalogue and the report builders; a test checks every key has both languages.

use super::Lang;

/// (key, English, Urdu)
const TEXTS: &[(&str, &str, &str)] = &[
    // Modules
    ("module.students", "Students", "طلبہ"),
    ("module.fees", "Fees & Finance", "فیس و مالیات"),
    ("module.attendance", "Attendance", "حاضری"),
    ("module.exams", "Examinations", "امتحانات"),
    // Parameters
    ("param.academic_year", "Academic year", "تعلیمی سال"),
    ("param.class", "Class", "جماعت"),
    ("param.section", "Section", "سیکشن"),
    ("param.status", "Status", "حیثیت"),
    ("param.month", "Month", "مہینہ"),
    ("param.from", "From", "از"),
    ("param.to", "To", "تا"),
    ("param.all", "All", "سب"),
    // Common
    ("common.generated", "Generated on", "تاریخِ اجرا"),
    ("common.page", "Page", "صفحہ"),
    ("common.total", "Total", "کل"),
    ("common.rows", "Rows", "قطاریں"),
    ("common.yes", "Yes", "ہاں"),
    ("common.no", "No", "نہیں"),
    ("common.none", "None", "کوئی نہیں"),
    ("common.not_enrolled", "Not enrolled", "داخلہ نہیں"),
    // Columns
    ("col.no", "No.", "نمبر شمار"),
    ("col.student_number", "Student number", "طالب علم نمبر"),
    ("col.name", "Name", "نام"),
    ("col.father_name", "Father's name", "والد کا نام"),
    ("col.gender", "Gender", "جنس"),
    ("col.date_of_birth", "Date of birth", "تاریخ پیدائش"),
    ("col.class", "Class", "جماعت"),
    ("col.section", "Section", "سیکشن"),
    ("col.roll_no", "Roll no.", "رول نمبر"),
    ("col.status", "Status", "حیثیت"),
    ("col.admission_date", "Admission date", "تاریخ داخلہ"),
    ("col.guardian", "Guardian", "سرپرست"),
    ("col.phone", "Phone", "فون"),
    ("col.email", "Email", "ای میل"),
    ("col.relationship", "Relationship", "رشتہ"),
    ("col.primary", "Primary", "بنیادی"),
    ("col.cnic", "CNIC / B-Form", "شناختی کارڈ / ب فارم"),
    ("col.occupation", "Occupation", "پیشہ"),
    ("col.address", "Address", "پتہ"),
    ("col.class_teacher", "Class teacher", "کلاس ٹیچر"),
    ("col.capacity", "Capacity", "گنجائش"),
    ("col.boys", "Boys", "لڑکے"),
    ("col.girls", "Girls", "لڑکیاں"),
    ("col.other", "Other", "دیگر"),
    ("col.total", "Total", "کل"),
    ("col.date", "Date", "تاریخ"),
    ("col.reason", "Reason", "وجہ"),
    ("col.kind", "Movement", "نوعیت"),
    ("col.day", "Day", "دن"),
    ("col.age", "Turns", "عمر ہوگی"),
    ("col.missing", "Missing", "نامکمل"),
    // Values
    ("value.male", "Male", "مرد"),
    ("value.female", "Female", "عورت"),
    ("value.other", "Other", "دیگر"),
    ("value.active", "Active", "زیرِ تعلیم"),
    ("value.left", "Left", "چھوڑ گیا"),
    ("value.graduated", "Graduated", "فارغ التحصیل"),
    ("value.father", "Father", "والد"),
    ("value.mother", "Mother", "والدہ"),
    ("value.guardian", "Guardian", "سرپرست"),
    ("value.admission", "Admission", "داخلہ"),
    ("value.withdrawal", "Withdrawal", "اخراج"),
    ("missing.cnic", "CNIC/B-Form", "شناختی کارڈ/ب فارم"),
    ("missing.photo", "photo", "تصویر"),
    ("missing.guardian", "guardian", "سرپرست"),
    ("missing.phone", "guardian phone", "سرپرست کا فون"),
    ("missing.roll_no", "roll number", "رول نمبر"),
    ("missing.enrolment", "enrolment", "داخلہ"),
    ("missing.address", "address", "پتہ"),
    // Reports
    ("report.students.list", "Student list", "طلبہ کی فہرست"),
    (
        "report.students.list.desc",
        "Students with class, section, roll number and the primary guardian's phone.",
        "طلبہ، جماعت، سیکشن، رول نمبر اور بنیادی سرپرست کے فون کے ساتھ۔",
    ),
    (
        "report.students.strength",
        "Class strength",
        "جماعت وار تعداد",
    ),
    (
        "report.students.strength.desc",
        "Active students per section with the boys/girls split and totals.",
        "ہر سیکشن میں زیرِ تعلیم طلبہ، لڑکے/لڑکیاں اور کل۔",
    ),
    (
        "report.students.movements",
        "Admissions and withdrawals",
        "داخلے اور اخراج",
    ),
    (
        "report.students.movements.desc",
        "Students admitted, left or graduated in a period.",
        "ایک مدت میں داخل ہونے، چھوڑنے یا فارغ ہونے والے طلبہ۔",
    ),
    (
        "report.students.contacts",
        "Guardian contact list",
        "سرپرستوں کے رابطے",
    ),
    (
        "report.students.contacts.desc",
        "Every guardian of the students in scope with phone and email.",
        "طلبہ کے تمام سرپرست، فون اور ای میل کے ساتھ۔",
    ),
    ("report.students.birthdays", "Birthdays", "سالگرہ"),
    (
        "report.students.birthdays.desc",
        "Active students born in a month, by day.",
        "ایک مہینے میں پیدا ہونے والے زیرِ تعلیم طلبہ، دن کے لحاظ سے۔",
    ),
    ("report.students.missing", "Missing data", "نامکمل ریکارڈ"),
    (
        "report.students.missing.desc",
        "Students without a CNIC/B-Form, photo, guardian, guardian phone, roll number or address.",
        "وہ طلبہ جن کا شناختی کارڈ/ب فارم، تصویر، سرپرست، فون، رول نمبر یا پتہ درج نہیں۔",
    ),
    ("group.class", "Class", "جماعت"),
    ("subtotal", "Subtotal", "ذیلی کل"),
];

/// The text for a key, or the key itself (never panics; the test below keeps keys complete).
pub fn tr(lang: Lang, key: &str) -> &'static str {
    match TEXTS.iter().find(|(k, _, _)| *k == key) {
        Some((_, en, ur)) => match lang {
            Lang::En => en,
            Lang::Ur => ur,
        },
        None => {
            tracing::warn!(key, "report text missing");
            Box::leak(key.to_string().into_boxed_str())
        }
    }
}

/// `true` when the key exists (used by tests and the catalogue).
pub fn has(key: &str) -> bool {
    TEXTS.iter().any(|(k, _, _)| *k == key)
}

/// A value such as a gender or a status, translated when a `value.*` key exists.
pub fn value(lang: Lang, raw: &str) -> String {
    let key = format!("value.{raw}");
    if has(&key) {
        tr(lang, &key).to_string()
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_both_languages_and_is_unique() {
        let mut seen = std::collections::HashSet::new();
        for (k, en, ur) in TEXTS {
            assert!(seen.insert(*k), "duplicate key {k}");
            assert!(!en.trim().is_empty(), "{k} has no English");
            assert!(!ur.trim().is_empty(), "{k} has no Urdu");
            assert!(
                ur.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)),
                "{k} Urdu is not Urdu: {ur}"
            );
        }
        assert_eq!(tr(Lang::Ur, "col.name"), "نام");
        assert_eq!(value(Lang::En, "male"), "Male");
        assert_eq!(value(Lang::Ur, "unknown"), "unknown");
    }
}
