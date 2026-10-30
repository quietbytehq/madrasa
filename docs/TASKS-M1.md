# Milestone M1 — Lite core

Goal: the pilot school can run its daily work in Madrasa Lite: students, classes, fees, attendance, exams and report cards, with encrypted backup and Excel import.

Pilot school: K-12, ~1,200 students, ~120 staff. Details in docs/spec.md ("Pilot school profile").

## How to work
- Work **one stage at a time**, in order. Each stage gets its own branch (`m1-stage-N-name`) and its own pull request into `main`. Do not merge — the owner reviews and merges.
- Stop after opening each stage's PR and wait for the owner, unless told to continue.
- Follow every rule in CLAUDE.md (UUID v7, sync columns, audit log, money as exact decimals, backend permissions, en/ur i18n with RTL, tests).
- Use the demo data generator for all testing. Never use real data.
- Tick `[x]` only when built AND tested. Keep docs/PROGRESS.md updated; log blockers in docs/BLOCKERS.md.

---

## Stage 1 — Academic structure, students and guardians
- [x] Academic years: create, set active; start month configurable (April or August)
- [x] Classes (K-12, configurable names) and sections; class teacher per section
- [x] Subjects per class
- [x] Student record: name, father's name, DOB, gender, B-Form/CNIC, photo, address, admission date, class/section, status (active, left, graduated), board roll/registration number, custom fields (JSONB)
- [x] Student number: school prefix + year + sequence (e.g. KHI-MC-2026-00417); UUID v7 internally; legacy ID alias field
- [x] Guardians: multiple per student, relationship, phone, email, CNIC, occupation; sibling linking via shared guardian
- [x] Duplicate warning on create (B-Form/CNIC, or name + DOB + father's name)
- [x] Student directory: search, filter by class/section/status, sort, paginate; student profile page with tabs (details, guardians, fees, attendance, results — later tabs "Coming soon")
- [x] Add/edit student form with inline validation; withdraw/leave workflow (no hard delete)
- [x] Teacher/staff profiles: basic record, role, subjects and sections assigned
- [x] Permissions: admin manages students; teachers read their own sections only (tested in backend)
- [x] Demo data generator updated for all of the above

## Stage 2 — Fees
- [x] Fee heads (tuition, admission, transport, exam, etc.) configurable
- [x] Fee structure per class per academic year; monthly billing
- [x] Monthly voucher generation for a class or the whole school; voucher numbers unique and sequential
- [x] **Sibling discount:** 10% of the total monthly fee by default; percentage and which siblings qualify (all, or second child onwards) configurable
- [x] Other concessions: staff child, scholarship, custom — with approval and reason
- [x] **Late fee:** 10% of the monthly fee after the 15th by default (configurable); waivable per voucher with reason and approver, logged
- [x] Payment recording: cash, bank, online, 1Bill, Kuickpay (manual entry with reference number); partial payments
- [x] Receipts: immutable once issued; corrections only by reversal entries; reprint allowed
- [x] Student fee ledger: billed, paid, discounts, fines, balance; unpaid balance carried to next month and next year
- [x] Defaulter list (by days overdue, class); daily collection summary
- [x] Printable voucher (bank/1Bill/Kuickpay reference area, school copy + parent copy) and receipt templates; English and Urdu; print always light theme; PDF export
- [x] Permissions: only admin/accountant collect fees; every money change audit-logged
- [x] Tests: discount and late-fee calculations, rounding, reversals, carry-forward (exact decimals)

## Stage 3 — Attendance
- [x] Daily class-wise attendance: everyone present by default, teacher marks only absentees (and late/leave)
- [x] One-screen marking per section, fast keyboard use
- [x] Corrections after submission allowed for admin only, with audit history
- [x] Holidays and school calendar (no attendance on holidays/weekends)
- [x] Student attendance history on profile; monthly register view; absentee report
- [x] Staff attendance (simple daily present/absent/leave)
- [x] Dashboard: today's attendance %, sections not yet marked
- [x] Permissions: teachers mark their own sections only

## Stage 4 — Exams, results and report cards
- [x] Assessment types per academic year, default: Bi-monthly Test 1, Bi-monthly Test 2, Mid-year, Final; weightage configurable (must total 100%)
- [x] Max marks per subject per assessment
- [x] Marks entry grid by teacher for their subjects/sections; absent marking; validation
- [x] Approval workflow: teacher submits, principal approves, results locked after approval
- [x] Grading scheme configurable (marks ranges → grades, pass marks)
- [x] Result calculation: weighted totals, percentage, grade, position in section (optional)
- [x] Report card template (default design, English and Urdu), per student and bulk print/PDF
- [x] Result sheet per section; subject analysis report
- [x] Tests: weighted calculation, grading boundaries, rounding

## Stage 5 — Encrypted backup and restore
- [ ] Scheduled automatic backup (default nightly) to local folder, USB or network path
- [ ] AES-256-GCM encryption; key from backup password via Argon2id; recovery key from setup wizard works
- [ ] Signed/verified backups; restore refuses tampered files
- [ ] One-click restore with password; restore preview (backup date, school, record counts)
- [ ] Backup status on dashboard; warning if last backup is older than 2 days
- [ ] Password change applies to new backups only, with clear warning
- [ ] Audit log for every backup, restore and failed password attempt
- [ ] Tests: backup → restore round trip, wrong password, tampered file

## Stage 6 — Excel/CSV import wizard
- [ ] Downloadable templates: students + guardians, staff, opening fee balances
- [ ] Upload .xlsx/.csv; column mapping UI; saved mappings
- [ ] Cleanup: trim, date formats, phone/CNIC/B-Form normalisation, class name matching, Urdu text
- [ ] Preview with row-level errors and warnings; dry run; fix in grid or download error file
- [ ] Duplicate detection (skip / update / merge)
- [ ] Import as a batch with one-click rollback; audit-logged
- [ ] Export: every list to Excel/CSV
- [ ] Tests: messy sample files (generated, fake data)

## Stage 7 — Pilot readiness
- [ ] Academic year rollover: promote students, carry forward unpaid fees, archive previous year
- [ ] Default roles reviewed: principal, admin, accountant, teacher — permission matrix documented in docs/
- [ ] Dashboard wired to real data: students, today's attendance, fees collected this month, defaulters, backup status
- [ ] Performance check with 1,200 students, 120 staff and a full year of demo fees/attendance: key screens load in under 1 second
- [ ] Short Urdu + English user guide in docs/user-guide/ (setup, students, fees, attendance, exams, backup)
- [ ] Release build: Linux .deb/AppImage and Windows installer via CI

## Done when
A fresh Lite install can be set up for the pilot school, students imported from Excel, a month of fees billed and collected, daily attendance taken, a full exam cycle entered and approved, report cards printed, and data backed up and restored — on Linux and Windows, all tests passing.
