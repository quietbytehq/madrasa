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
| Pre-pilot fixes (2026-11-27) | `import.resume.*` (9), `hub.users.resetHint`, `hub.users.ownPassword` | none |

## Terms settled by the owner (2026-11-27)

Three choices from the table below were settled, and every place was switched in the same
change. The list is what was found and changed; a search of the repository for the old words
now finds them only in this file and in the history in `docs/PROGRESS.md`.

| Was | Is now | Places changed |
| --- | --- | --- |
| سائن ان (sign in) | **لاگ ان** | 5 uses: 4 on screen, 1 in the manual |
| واؤچر (fee voucher) | **چالان** | 103 uses: 39 on screen, 13 in printed reports, 4 in import templates, 22 in the manual, 25 in the user guide |
| صدر دفتر (head office) | **ہیڈ آفس** | none: the product already said ہیڈ آفس everywhere (49 strings on screen, 13 lines each in the manual and the guide); صدر دفتر was only ever a suggestion in this file |

Changed with them, so that the same line does not use two families of words:
سائن آؤٹ (sign out) is now **لاگ آؤٹ** (1 place, `app.error.signedOut`; the other six
strings already said لاگ آؤٹ), and the plurals follow: واؤچروں is چالانوں, and the
English-style plural واؤچرز is plain چالان.

**سائن ان → لاگ ان**

- Hub console, `apps/desktop/src/i18n/ur.ts`: `hub.users.about`, `hub.users.lastLogin`, `hub.users.active`
- Desktop app, same file: `app.error.signedOut` (also سائن آؤٹ → لاگ آؤٹ)
- Manual, `docs/manual/manual.yaml`: chapter `hub-setup`, one passage
- Parent app: none (its sign-in already said لاگ ان)

**واؤچر → چالان**

- Desktop app, `apps/desktop/src/i18n/ur.ts`, 33 strings: `fees.tabs.vouchers`, `fees.generate`, `fees.generate.done`, `fees.voucherNo`, `fees.search`, `fees.empty`, `fees.pay.lateWarning`, `fees.cancel`, `fees.print.voucher`, `fees.ledger.empty`, `fees.setup.concessionBase.hint`, `backup.table.fee_vouchers`, `import.kind.opening_balances.hint`, `rollover.intro`, `settings.paymentReference`, `settings.paymentReference.hint`, `adm.convert.hint`, `adm.settings.feeHead`, `adm.settings.noFeeHead`, `lib.fine.settledVoucher`, `lib.settings.hint`, `trp.settings.hint`, `trp.settings.noFeeHead`, `hst.hostel.hint`, `hst.hostel.noFeeHead`, `apikeys.doc.fees`, `offline.notice.overpaid`, `fees.provisional.again`, `fees.provisional.again.required`, `health.why.students_without_section`, `health.why.classes_without_fee_structure`, `health.check.vouchers_without_fee_head`, `health.why.vouchers_without_fee_head`
- Parent app, same file, 6 strings: `parent.fees.openVouchers`, `parent.fees.openVoucher`, `parent.fees.vouchers`, `parent.fees.none`, `parent.fees.carried`, `parent.fees.voucherNo`
- Hub console: none on screen
- Printed reports and PDFs of a school, `crates/api/src/reports/text.rs`, 12 strings:
  `report.library.fines.desc`, `lib.how.voucher`, `report.hostel.outstanding.desc`, `hst.col.vouchers`, `col.voucher_no`, `col.unpaid_vouchers`, `col.vouchers`, `report.fees.ledger.desc`, `report.fees.vouchers`, `report.fees.vouchers.desc`, `note.head_split`, `source.fee_vouchers`
- The hub's reports, `crates/nexus/src/reports/text.rs`: `note.billed`
- Import templates, `crates/core/src/imports.rs`: the **ہدایات** sheet of the fee structure
  template (2 uses in one line) and of the opening balances template (2 uses in one line).
  No column heading and no heading an import recognises used the word, so files already
  filled in are read as before.
- Manual, `docs/manual/manual.yaml`, 22 uses: chapter `settings` 1; chapter `students` 1; chapter `import` 3; chapter `fees` 9; chapter `reports` 1; chapter `admissions` 1; chapter `library` 1; chapter `transport` 1; chapter `hostel` 1; chapter `pa-phone` 1; chapter `rollover` 2
- User guide, `docs/user-guide/ur.md`, 25 uses: 1a. اسکول کا ریکارڈ مدرسہ میں لانا (3); 3. فیس (6); 7. تعلیمی سال کا اختتام (1); 8. رپورٹس (2); 11. داخلے (اسٹینڈرڈ) (2); 13. ٹرانسپورٹ (اسٹینڈرڈ) (1); 14. ہاسٹل (اسٹینڈرڈ) (1); 18. والدین کی ایپ (اسکول نیٹ ورک میں شامل اسکولوں کے لیے) (1); اسکول سرور سے منسلک کمپیوٹر پر (1); آف لائن لی گئی فیس کی رسیدیں (6); 20. مدرسہ کو کی بورڈ سے چلانا (1). The guide was not in the
  owner's list (app, parent app, hub console, manual source) and was switched all the same:
  it describes the same screens.

The manual's screenshots were made again after the switch, so the pictures show the new
words.

Still for a native reader to judge: whether چالان reads right in every one of these
sentences (the word was swapped, the sentences were not rewritten), and "فیس چالان" against
plain "چالان".

## Choices the developer was unsure of

A reader should settle these once, and the same word should then be used everywhere.
Three are settled (above); the rest are open.

| English | Used now | The doubt |
| --- | --- | --- |
| backup | بیک اپ | A loan word throughout. Is "محفوظ نقل" wanted anywhere, at least once as an explanation? |
| restore | بحالی / بحال کرنا | Understood for data? |
| dashboard | ڈیش بورڈ | Loan word. |
| voucher (fee) | چالان | Settled 2026-11-27 (was واؤچر). |
| receipt | رسید | |
| section (of a class) | سیکشن | |
| guardian | سرپرست | Used for father, mother and guardian alike. |
| concession / scholarship | رعایت / وظیفہ | |
| defaulters | نادہندگان | Formal; does a school office say this? |
| sign in / log in | لاگ ان | Settled 2026-11-27: سائن ان is no longer used anywhere. |
| import / export | درآمد / برآمد | Literal. "ایکسل سے لانا" may be plainer. |
| dry run ("what will happen") | آزمائشی جائزہ | Made up for this. |
| Data health | ڈیٹا کی جانچ | The English idiom was avoided; "جانچ" is also used for the backup check and for "check" in general (26 strings). Too many meanings for one word? |
| temporary database | عارضی ڈیٹا بیس | |
| school network / head office (Nexus) | اسکول نیٹ ورک / ہیڈ آفس | Settled 2026-11-27: ہیڈ آفس stays. |
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
