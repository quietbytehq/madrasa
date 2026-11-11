//! Titles, column headings and notes of the network reports, in English and Urdu. One table
//! for both languages, so a report cannot exist in one and not the other.

/// Report language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Ur,
}

impl Lang {
    pub fn parse(code: Option<&str>) -> Lang {
        match code.map(str::trim) {
            Some("ur") => Lang::Ur,
            _ => Lang::En,
        }
    }
}

/// (key, English, Urdu).
const TEXT: &[(&str, &str, &str)] = &[
    // Titles and descriptions
    ("summary.title", "Network summary", "نیٹ ورک کا خلاصہ"),
    ("summary.about", "Each school's students, staff and sections, and when it last synced.", "ہر اسکول کے طلبہ، عملہ اور سیکشن، اور آخری ہم آہنگی کا وقت۔"),
    ("enrolment.title", "Enrolment by school and class", "اسکول اور جماعت کے لحاظ سے داخلے"),
    ("enrolment.about", "Active students of the current academic year in each class, with boys and girls.", "رواں تعلیمی سال میں ہر جماعت کے فعال طلبہ، لڑکوں اور لڑکیوں کے ساتھ۔"),
    ("fees.title", "Fees billed and collected by school and month", "اسکول اور مہینے کے لحاظ سے فیس کے بل اور وصولی"),
    ("fees.about", "What each school billed for a month, what it collected in that month, and what is still outstanding.", "ہر اسکول نے کسی مہینے کا کتنا بل بنایا، اس مہینے میں کتنا وصول کیا، اور کتنا باقی ہے۔"),
    ("attendance.title", "Attendance by school and month", "اسکول اور مہینے کے لحاظ سے حاضری"),
    ("attendance.about", "Student attendance marks of each school per month.", "ہر اسکول کی ماہانہ طلبہ حاضری۔"),
    ("sync.title", "Sync health", "ہم آہنگی کی حالت"),
    ("sync.about", "Every school in the register: its status, last sync and anything waiting.", "رجسٹر کا ہر اسکول: اس کی حالت، آخری ہم آہنگی اور جو کچھ منتظر ہے۔"),
    // Columns
    ("col.school", "School", "اسکول"),
    ("col.code", "Code", "کوڈ"),
    ("col.edition", "Edition", "ایڈیشن"),
    ("col.version", "Version", "ورژن"),
    ("col.students", "Students", "طلبہ"),
    ("col.staff", "Staff", "عملہ"),
    ("col.sections", "Sections", "سیکشن"),
    ("col.last_sync", "Last sync", "آخری ہم آہنگی"),
    ("col.class", "Class", "جماعت"),
    ("col.boys", "Boys", "لڑکے"),
    ("col.girls", "Girls", "لڑکیاں"),
    ("col.other", "Other", "دیگر"),
    ("col.total", "Total", "کل"),
    ("col.currency", "Currency", "کرنسی"),
    ("col.month", "Month", "مہینہ"),
    ("col.billed", "Billed", "بل"),
    ("col.collected", "Collected", "وصولی"),
    ("col.receipts", "Receipts", "رسیدیں"),
    ("col.outstanding", "Outstanding", "بقایا"),
    ("col.marked", "Marks", "اندراجات"),
    ("col.present", "Present", "حاضر"),
    ("col.late", "Late", "تاخیر"),
    ("col.absent", "Absent", "غیر حاضر"),
    ("col.leave", "Leave", "رخصت"),
    ("col.present_pct", "Present %", "حاضری %"),
    ("col.status", "Status", "حالت"),
    ("col.joined", "Joined", "شمولیت"),
    ("col.pending", "Waiting to go to the school", "اسکول کو بھیجے جانے کے منتظر"),
    ("col.parked", "Waiting to be stored", "محفوظ ہونے کے منتظر"),
    ("col.overwrites", "Overwritten values", "بدلی گئی قدریں"),
    // Rows and notes
    ("row.total", "Network total", "نیٹ ورک کا کل"),
    ("row.total_currency", "Total", "کل"),
    ("note.active_only", "Only schools that are joined are included. Schools that left or were removed are not.", "صرف شامل اسکول شمار ہیں۔ چھوڑنے والے یا ہٹائے گئے اسکول شامل نہیں۔"),
    ("note.per_currency", "Money is never added across currencies: totals are per currency.", "مختلف کرنسیوں کی رقم جمع نہیں کی جاتی: کل ہر کرنسی کا الگ ہے۔"),
    ("note.as_synced", "Figures are as of each school's last sync.", "اعداد ہر اسکول کی آخری ہم آہنگی تک کے ہیں۔"),
    ("note.billed", "Billed is the month's own charges and late fees, without arrears carried from earlier months. Collected is by date of payment. Outstanding is what is unpaid now on that month's vouchers.", "بل میں اس مہینے کی اپنی فیس اور لیٹ فیس شامل ہے، پچھلے مہینوں کا بقایا نہیں۔ وصولی ادائیگی کی تاریخ کے مطابق ہے۔ بقایا وہ رقم ہے جو اس مہینے کے واؤچروں پر ابھی ادا نہیں ہوئی۔"),
    ("note.present_pct", "Present % counts present and late marks.", "حاضری % میں حاضر اور تاخیر دونوں شامل ہیں۔"),
    ("status.joining", "Joining", "شامل ہو رہا ہے"),
    ("status.active", "Joined", "شامل"),
    ("status.left", "Left", "چھوڑ دیا"),
    ("status.removed", "Removed", "ہٹایا گیا"),
    ("status.purged", "Data deleted", "ڈیٹا حذف"),
];

/// The text for a key in a language; the key itself when it is unknown (visible, not empty).
pub fn tr(lang: Lang, key: &str) -> String {
    TEXT.iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, en, ur)| match lang {
            Lang::En => *en,
            Lang::Ur => *ur,
        })
        .unwrap_or(key)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_exists_in_both_languages_once() {
        let mut keys: Vec<&str> = TEXT.iter().map(|(k, _, _)| *k).collect();
        let total = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), total, "a key is listed twice");
        for (key, en, ur) in TEXT {
            assert!(!en.trim().is_empty() && !ur.trim().is_empty(), "{key}");
            assert_ne!(en, ur, "{key} is not translated");
        }
        assert_eq!(tr(Lang::Ur, "col.school"), "اسکول");
        assert_eq!(tr(Lang::En, "no.such.key"), "no.such.key");
        assert_eq!(Lang::parse(Some("ur")), Lang::Ur);
        assert_eq!(Lang::parse(Some("fr")), Lang::En);
        assert_eq!(Lang::parse(None), Lang::En);
    }
}
