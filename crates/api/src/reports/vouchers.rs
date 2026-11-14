//! Fee vouchers typeset with the report PDF engine (v0.2.0-alpha B2/B3): every amount
//! carries the school's currency, the text is real text (copy-paste, screen readers), and
//! the payment-reference block follows the school's setting. One voucher per page, school
//! copy and parent copy side by side.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;

use super::pdf::{compile, lit, mark};
use super::text::tr;
use super::{format_date, money, Lang};
use crate::ApiError;

/// Everything a voucher page needs, already resolved (names, policy, wording).
#[derive(Debug, Clone, Serialize)]
pub struct VoucherDoc {
    pub school_name: String,
    pub school_city: String,
    pub school_code: String,
    /// Currency symbol printed before every amount, e.g. `Rs`.
    pub currency: String,
    pub voucher_no: String,
    pub student_name: String,
    pub father_name: String,
    pub student_number: String,
    pub class_section: String,
    pub billing_month: NaiveDate,
    pub issue_date: NaiveDate,
    pub due_date: NaiveDate,
    pub lines: Vec<(String, Decimal)>,
    pub gross: Decimal,
    pub sibling_discount: Decimal,
    pub concession_discount: Decimal,
    pub net_monthly: Decimal,
    pub arrears: Decimal,
    pub total_due: Decimal,
    pub late_fee: Decimal,
    pub net_paid: Decimal,
    pub balance: Decimal,
    pub late_fee_percent: Decimal,
    pub reference_label: String,
    pub reference_note: String,
    pub reference: String,
}

fn month_label(d: NaiveDate) -> String {
    d.format("%B %Y").to_string()
}

/// The typst source for a set of vouchers. Public so tests can inspect it.
pub fn source(docs: &[VoucherDoc], lang: Lang) -> String {
    let rtl = lang.is_rtl();
    let (font, size, lang_code, dir) = match lang {
        Lang::En => ("(\"Inter\", \"Noto Nastaliq Urdu\")", "8.5pt", "en", "ltr"),
        Lang::Ur => ("(\"Noto Nastaliq Urdu\", \"Inter\")", "9pt", "ur", "rtl"),
    };
    let mut s = String::new();
    s.push_str("#set page(paper: \"a4\", flipped: true, margin: (x: 1.2cm, y: 1.2cm))\n");
    s.push_str(&format!(
        "#set text(font: {font}, size: {size}, lang: \"{lang_code}\", dir: {dir})\n#set par(leading: {leading})\n#set table(stroke: none, inset: (x: 3pt, y: 2pt))\n",
        leading = if rtl { "0.8em" } else { "0.5em" },
    ));
    for (i, d) in docs.iter().enumerate() {
        if i > 0 {
            s.push_str("#pagebreak()\n");
        }
        let school_copy = copy(d, lang, tr(lang, "voucher.school_copy"));
        let parent_copy = copy(d, lang, tr(lang, "voucher.parent_copy"));
        s.push_str(&format!(
            "#grid(columns: (1fr, 1fr), column-gutter: 14pt, {school_copy}, {parent_copy})\n"
        ));
    }
    s
}

fn amount(d: &VoucherDoc, v: Decimal) -> String {
    format!("{} {}", d.currency, money(v))
}

fn copy(d: &VoucherDoc, lang: Lang, which: &str) -> String {
    let mut rows: Vec<(String, String, bool)> = d
        .lines
        .iter()
        .map(|(desc, amt)| (desc.clone(), amount(d, *amt), false))
        .collect();
    rows.push((tr(lang, "voucher.gross").into(), amount(d, d.gross), true));
    if d.sibling_discount > Decimal::ZERO {
        rows.push((
            tr(lang, "voucher.sibling").into(),
            format!("− {}", amount(d, d.sibling_discount)),
            false,
        ));
    }
    if d.concession_discount > Decimal::ZERO {
        rows.push((
            tr(lang, "voucher.concession").into(),
            format!("− {}", amount(d, d.concession_discount)),
            false,
        ));
    }
    if d.sibling_discount + d.concession_discount > Decimal::ZERO {
        rows.push((
            tr(lang, "voucher.net_monthly").into(),
            amount(d, d.net_monthly),
            false,
        ));
    }
    if d.arrears != Decimal::ZERO {
        rows.push((
            tr(lang, "voucher.arrears").into(),
            amount(d, d.arrears),
            false,
        ));
    }
    rows.push((
        tr(lang, "voucher.total_due").into(),
        amount(d, d.total_due),
        true,
    ));
    if d.late_fee > Decimal::ZERO {
        rows.push((
            tr(lang, "voucher.late_fee").into(),
            amount(d, d.late_fee),
            false,
        ));
    }
    if d.net_paid > Decimal::ZERO {
        rows.push((
            tr(lang, "voucher.paid").into(),
            format!("− {}", amount(d, d.net_paid)),
            false,
        ));
    }
    rows.push((
        tr(lang, "voucher.balance").into(),
        amount(d, d.balance),
        true,
    ));
    let table_rows: String = rows
        .iter()
        .map(|(label, value, strong)| {
            if *strong {
                format!(
                    "[#text(weight: \"bold\")[#{}]], [#align(end)[#text(weight: \"bold\")[#{}]]],",
                    lit(label),
                    lit(value)
                )
            } else {
                format!("[#{}], [#align(end)[#{}]],", lit(label), lit(value))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let pay_before = tr(lang, "voucher.pay_before")
        .replace("{date}", &format_date(d.due_date))
        .replace("{percent}", &d.late_fee_percent.normalize().to_string());
    let detail = |label: &str, value: &str| {
        format!(
            "[#text(fill: luma(90))[#{}]], [#{}],",
            lit(label),
            lit(value)
        )
    };
    let details = [
        detail(tr(lang, "voucher.student"), &d.student_name),
        detail(tr(lang, "voucher.father"), &d.father_name),
        detail(tr(lang, "voucher.number"), &d.student_number),
        detail(tr(lang, "voucher.class"), &d.class_section),
        detail(tr(lang, "voucher.month"), &month_label(d.billing_month)),
        detail(tr(lang, "voucher.issue"), &format_date(d.issue_date)),
        detail(tr(lang, "voucher.due"), &format_date(d.due_date)),
    ]
    .join("\n");
    format!(
        "block(stroke: 0.5pt + black, inset: 10pt, width: 100%)[\
#grid(columns: (auto, 1fr, auto), column-gutter: 6pt, [#{mark}], [#text(size: 12pt, weight: \"bold\")[#{school}]\\\n#text(size: 8pt, fill: luma(90))[#{city} · #{code}]], \
[#align(end)[#text(size: 8pt, weight: \"bold\")[#upper({which})]\\\n#text(font: \"Inter\")[#{voucher_no}]]])\
#line(length: 100%, stroke: 0.4pt + luma(170))\
#grid(columns: (auto, 1fr), column-gutter: 8pt, row-gutter: 2pt, {details})\
#v(4pt)\
#table(columns: (1fr, auto), {table_rows})\
#v(2pt)#text(size: 7.5pt, fill: luma(90))[#{pay_before}]\
#v(6pt)#block(stroke: (dash: \"dashed\", thickness: 0.5pt, paint: luma(120)), inset: 6pt, width: 100%)[\
#text(size: 7.5pt, weight: \"bold\")[#upper({ref_label})]\\\n#text(font: \"Inter\", size: 11pt)[#{reference}]\\\n#text(size: 7.5pt, fill: luma(90))[#{ref_note}]\
#v(8pt)#grid(columns: (1fr, 1fr), [#text(size: 7.5pt)[#{received_by}: \\_\\_\\_\\_\\_\\_\\_\\_\\_\\_\\_\\_]], [#align(end)[#text(size: 7.5pt)[#{signature}: \\_\\_\\_\\_\\_\\_\\_\\_\\_\\_\\_\\_]]])]]",
        mark = mark("0.85cm"),
        school = lit(&d.school_name),
        city = lit(&d.school_city),
        code = lit(&d.school_code),
        which = lit(which),
        voucher_no = lit(&d.voucher_no),
        pay_before = lit(&pay_before),
        ref_label = lit(&d.reference_label),
        reference = lit(&d.reference),
        ref_note = lit(&d.reference_note),
        received_by = lit(tr(lang, "voucher.received_by")),
        signature = lit(tr(lang, "voucher.signature")),
    )
}

/// Typeset the vouchers. CPU-bound: call from `spawn_blocking`.
pub fn render(docs: &[VoucherDoc], lang: Lang) -> Result<Vec<u8>, ApiError> {
    compile(&source(docs, lang))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn sample() -> VoucherDoc {
        VoucherDoc {
            school_name: "Gulshan High".into(),
            school_city: "Karachi".into(),
            school_code: "KHI-MC".into(),
            currency: "Rs".into(),
            voucher_no: "V-2026-04-00017".into(),
            student_name: "Ali Khan".into(),
            father_name: "Ahmed Khan".into(),
            student_number: "KHI-MC-2026-00001".into(),
            class_section: "9-A".into(),
            billing_month: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            issue_date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            due_date: NaiveDate::from_ymd_opt(2026, 4, 15).unwrap(),
            lines: vec![
                ("Tuition".into(), dec!(2500)),
                ("Transport".into(), dec!(800)),
            ],
            gross: dec!(3300),
            sibling_discount: dec!(250),
            concession_discount: Decimal::ZERO,
            net_monthly: dec!(3050),
            arrears: dec!(1200),
            total_due: dec!(4250),
            late_fee: Decimal::ZERO,
            net_paid: dec!(1000),
            balance: dec!(3250),
            late_fee_percent: dec!(10),
            reference_label: "Bank / 1Bill / Kuickpay reference".into(),
            reference_note: "Pay at any branch quoting this reference.".into(),
            reference: "KHI-MC-V-2026-04-00017".into(),
        }
    }

    #[test]
    fn every_amount_carries_the_currency_and_both_copies_are_present() {
        let src = source(&[sample()], Lang::En);
        assert!(src.contains("Rs 3,300.00"), "gross with currency");
        assert!(src.contains("Rs 3,250.00"), "balance with currency");
        assert!(src.contains("Rs 1,200.00"), "arrears with currency");
        assert!(src.contains("− Rs 250.00"), "discount with currency");
        assert!(src.contains("School copy") && src.contains("Parent copy"));
        assert!(src.contains("KHI-MC-V-2026-04-00017"));
        assert!(src.contains("Bank / 1Bill / Kuickpay reference"));
        assert!(src.contains("April 2026"));
        assert!(src.contains("Pay by 15-04-2026"));
        let ur = source(&[sample()], Lang::Ur);
        assert!(ur.contains("dir: rtl"));
        assert!(ur.contains("Rs 3,250.00"));
    }

    #[test]
    fn vouchers_typeset_one_per_page_in_both_languages() {
        let docs = vec![sample(), sample()];
        for lang in [Lang::En, Lang::Ur] {
            let pdf = render(&docs, lang).unwrap();
            assert!(pdf.starts_with(b"%PDF"), "{lang:?}");
            assert!(pdf.len() > 2_000);
        }
    }
}
