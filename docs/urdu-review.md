# Urdu: what a native reader needs to look at

**Status: no Urdu in Madrasa has been read by a native speaker.** Every Urdu string was
written by the developer (an AI model) and checked only by machine: that it is there, that it
fits its place, that the screen turns right-to-left, that nothing is cut off and that it can be
read against its background (the browser tests of pilot stage 2). Whether it is *good* Urdu,
the Urdu a school office in Pakistan would use, nobody has judged. This list says where the
Urdu is, what to read first, and which choices the developer was unsure of. Nothing below was
"corrected" by guessing.

## Where the Urdu is

| What | Where | Size |
| --- | --- | --- |
| Every screen of the desktop app, the parent app and the hub console | `apps/desktop/src/i18n/ur.ts` (one line per string; the English is the same key in `en.ts`) | about 2,590 strings |
| Words on printed reports and PDFs (vouchers, receipts, report cards, registers, timetables) | `crates/api/src/reports/text.rs`; the hub's reports in `crates/nexus/src/reports/text.rs` | about 300 strings |
| Import templates: the **ہدایات** sheet, the Urdu column headings and the headings an import recognises | `crates/core/src/imports.rs` (`template_notes`, `urdu_label`, the alias lists) | about 150 strings |
| User guide | `docs/user-guide/ur.md` | 443 lines |
| User manual (the PDF) | `docs/manual/manual.yaml` (each step has `en:` and `ur:`) | about 300 passages |

Server messages for a single refusal ("Amount must be greater than zero.") are in English
only and are shown as they are, also in the Urdu app. The three most common ones (a fault on
the server, no permission, signed out) are said by the app in Urdu since stage 5.

## What to read first

1. **Money and anything a parent is handed**: `fees.*` (137 strings), the printed voucher and
   receipt in `reports/text.rs`, `parent.*` (138, the parent app). A wrong word here is seen
   outside the school.
2. **What is used every day**: `nav.*`, `action.*`, `common.*`, `dashboard.*`, `students.*`,
   `attendance.*`, `exams.*`, `login.*`, `topbar.*` (about 400 strings).
3. **Warnings a person must understand at once**: `backup.*` (95), `offline.*` (56),
   `import.*` (95), `health.*` (33), `password.*`, `app.error.*`.
4. The Standard edition's modules (`tt.*`, `lib.*`, `inv.*`, `hst.*`, `trp.*`, `adm.*`,
   `leave.*`, `boards.*`), the school network (`hub.*`, `nexus.*`, `pa.*`), settings.
5. The user guide and the manual.

## Written during the pilot-readiness run (2026-11-25/26), never seen by anyone

These are the newest and the least settled.

| Stage | Keys in `ur.ts` | Elsewhere |
| --- | --- | --- |
| 1 Security | `password.change.*` (10) | `crates/core/src/password.rs` (one line) |
| 3 Onboarding | `import.dry.*` (20), `import.kind.*`, `import.confirm.*`, `import.subtitle`, `health.*` (33), `backup.exportAll*` | the **ہدایات** sheet and Urdu headings of all five templates; user guide §1a and the "Export everything" bullet of §6 |
| 4 Reliability | `backup.alert.*` (6), `backup.check.*` (3), `backup.runs.check.*` (3) | user guide §6, the two bullets on the backup check and the red notice |
| 5 Polish | `app.error.server`, `app.error.forbidden`, `app.error.signedOut`, `fees.collect.hint`, `fees.ledger.receipts.empty`, `fees.ledger.concessions.empty`; reworded: `lib.loans.empty`, `hst.register.empty` | user guide, the keyboard paragraph |

## Choices the developer was unsure of

A reader should settle these once, and the same word should then be used everywhere.

| English | Used now | The doubt |
| --- | --- | --- |
| backup | بیک اپ | A loan word throughout. Is "محفوظ نقل" wanted anywhere, at least once as an explanation? |
| restore | بحالی / بحال کرنا | Understood for data? |
| dashboard | ڈیش بورڈ | Loan word. |
| voucher (fee) | واؤچر | Schools also say "فیس چالان". Which one for the pilot schools? |
| receipt | رسید | |
| section (of a class) | سیکشن | |
| guardian | سرپرست | Used for father, mother and guardian alike. |
| concession / scholarship | رعایت / وظیفہ | |
| defaulters | نادہندگان | Formal; does a school office say this? |
| sign in / log in | لاگ ان (26 strings) and, in newer strings, سائن ان (4) | **Both are in use.** One should be chosen. |
| import / export | درآمد / برآمد | Literal. "ایکسل سے لانا" may be plainer. |
| dry run ("what will happen") | آزمائشی جائزہ | Made up for this. |
| Data health | ڈیٹا کی جانچ | The English idiom was avoided; "جانچ" is also used for the backup check and for "check" in general (26 strings). Too many meanings for one word? |
| temporary database | عارضی ڈیٹا بیس | |
| school network / head office (Nexus) | اسکول نیٹ ورک / ہیڈ آفس | "ہیڈ آفس" is a loan word (49 strings); "صدر دفتر" is the Urdu. |
| the person who looks after Madrasa | مدرسہ سنبھالنے والا | Invented for the new error message. |
| filter | فلٹر | Loan word. |

Other things a reader should check that a machine cannot:

- **Tone and politeness** (کریں throughout, never کرو): is it even across the app?
- **Numbers**: the app shows Western digits (0-9) in Urdu too, and amounts as "Rs 2,500.00".
  Is that what schools want on a printed voucher?
- **Mixed-direction lines**: a voucher number, a file name or a Latin term inside an Urdu
  sentence. They are laid out correctly by the browser tests' measure; whether they *read*
  well is a reader's call.
- **The product's name**: written "مدرسہ" in running Urdu text and "Madrasa" in the logo.
- **Long explanations** (`*.hint`, `*.why.*`, `*.body`): the most likely to read as
  translated English.
- Six strings are the same in both languages on purpose (a language's own name, "1Bill",
  "Kuickpay", "ISBN", two that are only numbers).

## How to send corrections

Any form will do: a marked-up printout, a spreadsheet with the key and the better wording, or
an edit of `ur.ts` itself (one line per string, the key says where it is used). The English of
any key is the same key in `apps/desktop/src/i18n/en.ts`.
