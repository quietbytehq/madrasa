# Madrasa — Product and Technical Specification

Status: draft v0.1 (written during milestone M0). `CLAUDE.md` holds the non-negotiable rules;
this document expands them into product scope, architecture, data model and conventions.
When code and spec disagree, fix one in the same change so they never drift.

## 1. What Madrasa is

Madrasa is a free, open-source (Apache 2.0) desktop School Information System. It targets
schools in Pakistan and similar markets first (PKR, Urdu, intermittent connectivity), but
nothing in it is country-specific beyond defaults.

Design goals, in priority order:

1. **Works offline.** A single PC with no internet must run the full Lite product, including
   fonts, database and backups.
2. **Trustworthy money and records.** Fee and finance data is exact, immutable where it must
   be, and fully audited.
3. **Grows without rewrites.** Lite is one school on one PC. Standard adds a server and
   several clients. Nexus joins many schools into one network. The schema, IDs and API are
   designed for the largest case from day one.
4. **Fast for clerks.** Compact, information-dense, keyboard-friendly UI.

## 1.1 Pilot school profile

The first deployment target (assumed from `docs/TASKS-M1.md`; adjust when the real profile is
known): a K–12 school in Karachi with ~1,200 students in 13 grades (KG, 1–12) of up to three
sections each, ~120 staff, fees in PKR billed monthly, academic year starting in April (August
also supported), Urdu and English speaking staff, one office PC and intermittent internet.

## 2. Editions

| Edition | Deployment | Scope |
| --- | --- | --- |
| Lite | All-in-one desktop install, bundled PostgreSQL | One school. Soft limits: ~1,200 students, ~250 staff accounts. Core modules. |
| Standard | Server (API + PostgreSQL) plus desktop clients | All modules, multi-user over LAN/WAN, campuses. |
| Nexus | Network hub | Joins several schools into one tenant (an "AD forest root"): shared identities, cross-school reporting, parent accounts spanning schools. |

The edition is a value of the `edition` setting (`lite` | `standard` | `nexus`). One
feature-flag module (`madrasa_core::edition`) maps edition to enabled features. Both the API
and the UI consult it; the API is authoritative (a flag hidden in the UI is still enforced by
the backend).

Soft limits are advisory: the UI warns when a Lite school approaches them; nothing is
blocked.

## 3. Modules

M0 ships the shell only. Modules below are the planned Lite set; those marked *later* are
Standard/Nexus.

| Module | Lite | Notes |
| --- | --- | --- |
| Dashboard | yes | Summary cards, today's attendance, fee collection, alerts |
| Students | yes | Admissions, profiles, classes/sections, promotions, custom fields |
| Attendance | yes | Daily and per-period, staff attendance |
| Examinations | yes | Exams, marks entry, grading schemes, report cards |
| Fees & Finance | yes | Fee structures, invoices, receipts, concessions, ledgers, expenses |
| Teachers | yes | Staff profiles, assignments, timetable |
| Reports | yes | Printable reports (always light theme) |
| Backup | yes | Encrypted local backups, restore, recovery key |
| Settings | yes | School profile, academic years, roles, custom fields, edition |
| Campuses | later | Multi-campus school (Standard) |
| Parents portal | later | Parent accounts (Nexus-wide identities) |
| Transport, Library, Hostel, HR/Payroll | later | Standard |
| Network admin | later | Nexus hub console |

Unfinished modules are routable and show a clear "Coming soon" state; they are never
pretended to work.

## 4. Architecture

```
┌─────────────────────────── Desktop (Tauri 2) ───────────────────────────┐
│  React UI  ──HTTP/JSON──▶  Axum API (in-process in Lite)  ──sqlx──▶  PostgreSQL │
│  (apps/desktop/src)        (crates/api)                              (bundled)   │
└──────────────────────────────────────────────────────────────────────────┘
```

- The UI **only** talks to the REST API (`/api/v1`). It never touches the database.
- In Lite the Tauri shell starts the API on `127.0.0.1` on a free port and hands the base URL
  to the UI through a Tauri command. In Standard the same API crate runs as a standalone
  server (`madrasa-server` binary) and clients point at it. Nothing in the UI depends on where
  the API lives.
- `crates/core` holds pure domain logic (permissions, editions, fee and grading maths, crypto
  helpers, HLC). It has no I/O and is exhaustively unit tested.
- `crates/db` owns migrations and all SQL. Every mutation goes through a repository function
  that also writes the audit log inside the same transaction.
- `crates/demo-data` generates a clearly marked fake school. It never runs automatically.

### 4.1 PostgreSQL bundling

Decision recorded in `docs/adr/0001-postgresql-bundling.md`. Summary: Lite embeds PostgreSQL
binaries into the application (the `postgresql_embedded` crate with the `bundled` feature) and
runs a private cluster in the app data directory on a random localhost port with a generated
password. Standard and Nexus point at an external server through `MADRASA_DATABASE_URL`.

### 4.2 Runtime directories

| Purpose | Linux | Windows |
| --- | --- | --- |
| App data (cluster, config, logs) | `$XDG_DATA_HOME/madrasa` (default `~/.local/share/madrasa`) | `%APPDATA%\madrasa` |
| Backups (default) | `~/Documents/Madrasa Backups` | `Documents\Madrasa Backups` |

No secrets are written anywhere except the app data directory, with user-only permissions.

## 5. Data model rules

All tables follow these rules (enforced by migration tests):

1. Primary key `id UUID` generated as UUID v7 by the application.
2. Every school-owned table has `school_id UUID NOT NULL REFERENCES schools(id)`.
3. Every mutable table has `created_at TIMESTAMPTZ`, `updated_at TIMESTAMPTZ`,
   `updated_by UUID` (user id, nullable for system), `deleted_at TIMESTAMPTZ` (soft delete)
   and `hlc TEXT` (hybrid logical clock stamp, see 5.2).
4. Money is `NUMERIC(14,2)`; in Rust `rust_decimal::Decimal`. Never floats.
5. Receipts and other financial documents are immutable; corrections are reversal entries.
6. Core entities have a `custom JSONB NOT NULL DEFAULT '{}'` column. Allowed keys and types
   are declared in `custom_field_definitions`.
7. Deletes are soft (`deleted_at`). Hard deletes are reserved for purge jobs.

### 5.1 Foundation tables (M0)

| Table | Purpose |
| --- | --- |
| `schools` | One row per school. `code` is unique. `is_demo` marks generator output. |
| `users` | Local accounts: `username`, `full_name`, `email`, `password_hash` (Argon2id PHC string), `is_active`, `failed_login_count`, `locked_until`. |
| `roles` | Per-school roles. Built-in: `principal`, `admin`, `accountant`, `teacher`; `parent` reserved (Nexus). `is_system` rows cannot be deleted. |
| `permissions` | Grants: one row per (`role_id`, `code`). Codes are defined in `madrasa_core::permissions`. |
| `user_roles` | Many-to-many users ↔ roles. |
| `sessions` | Session tokens (hash only), expiry, last seen. |
| `settings` | Key/value (`value JSONB`) per school; `school_id` NULL for instance-wide keys such as `edition`. |
| `audit_log` | Append-only: `actor_user_id`, `action`, `entity`, `entity_id`, `old_value`, `new_value`, `at`. Triggers forbid UPDATE/DELETE. |
| `custom_field_definitions` | Registry of custom fields: `entity`, `key`, `label`, `field_type`, `required`, `options`. |

### 5.2 Hybrid logical clock

`hlc` is a 32-character, lexicographically sortable string:
`<unix_ms:013 zero-padded>-<counter:05 hex>-<node:12 hex>`. The node id is generated per
install and stored in settings. There is no sync engine yet; the column exists so that one can
be added without a migration of every table.

### 5.3 Audit log

Every create, update and delete executed by the application writes one `audit_log` row in the
same transaction. `old_value`/`new_value` are JSON snapshots of the row (secrets such as
password hashes are redacted before logging). Money changes are additionally required to
reference the financial document they touch (M1+).

### 5.4 Academic structure, students and guardians (M1 stage 1)

| Table | Purpose |
| --- | --- |
| `academic_years` | `name` (e.g. `2026-27`), `start_date`, `end_date`, `start_month` (4 or 8), `is_active` (at most one live active year per school). |
| `classes` | Grade levels: `name` (configurable, defaults KG, 1–12), `level` for ordering. |
| `sections` | A class in an academic year: `class_id`, `academic_year_id`, `name` (A, B, C), `class_teacher_id` (staff). |
| `subjects` | `name`, `code`; `class_subjects` maps subjects to classes. |
| `staff` | Staff profile: `staff_number`, names, contact, `cnic`, `designation`, `joined_on`, `status`, optional `user_id` link to a login. |
| `section_teachers` | Teaching assignments: `staff_id`, `section_id`, optional `subject_id`. |
| `students` | Profile fields listed in TASKS-M1 stage 1; `student_number` unique per school; `legacy_id` alias; `status` active/left/graduated with `left_on`/`left_reason`; `photo` stored in the database (≤ 512 KB); `custom` JSONB. |
| `enrollments` | Student ↔ section per academic year with `roll_no`; the enrollment in the active year is the current class/section. |
| `guardians` | Shared guardian records (`full_name`, `relationship`-independent contact data, `cnic`, `occupation`). Siblings are students linked to the same guardian. |
| `student_guardians` | Link with `relationship` (father, mother, guardian, other) and `is_primary`. |
| `school_sequences` | Per-school counters, e.g. `student_number:2026`, incremented under a row lock. |

Student numbers are `<prefix>-<year>-<5-digit sequence>`; the prefix defaults to the school code
and is configurable (`settings` key `student_number.prefix`), the year is the admission year.

Identifiers are normalised before storage and matching: CNIC/B-Form to 13 digits for a Pakistani school (elsewhere the national ID is kept as typed, 3–40 characters, upper case, and the phone keeps 7–15 digits with an optional `+`; see §9.1)
(`#####-#######-#`), phone numbers to digits with a leading `0` or `+92`.

Duplicate warning on create: a match on normalised B-Form/CNIC, or on (name, date of birth,
father's name) case-insensitively. The API answers `409 possible_duplicate` with the matches
unless the client passes `force: true`.

Teachers may read only students enrolled in sections they are class teacher of or teach a
subject in. This is enforced in the API (`students.read` + section scope), not in the UI.

### 5.5 Fees (M1 stage 2)

| Table | Purpose |
| --- | --- |
| `fee_heads` | Configurable heads: `name`, `code`, `is_recurring` (monthly, e.g. tuition) or one-time (e.g. admission, charged on a student's first voucher), `position`, `is_active`. |
| `fee_structures` | Amount per `academic_year_id`, `class_id`, `fee_head_id` (`NUMERIC(14,2)`); a zero amount removes the line. |
| `fee_concessions` | Per student: `kind` (staff_child, scholarship, custom), `percent` of the net monthly fee or a fixed `amount`, `reason`, `approved_by`, `approved_at`, `valid_from`, `valid_to`. Requires `fees.approve`. |
| `fee_vouchers` | One per student per `billing_month` (unique while not cancelled): `voucher_no`, `issue_date`, `due_date`, `gross`, `sibling_discount`, `concession_discount`, `net_monthly`, `arrears`, `total_due`, `late_fee`, `late_fee_waived` + `waiver_reason` + `waived_by`, `net_paid`, `status`. |
| `fee_voucher_lines` | The heads and amounts that make up `gross`. |
| `fee_receipts` | Immutable (database trigger refuses UPDATE/DELETE): `receipt_no`, `paid_on`, `amount`, `method` (cash, bank, online, one_bill, kuickpay), `reference`, `note`, `received_by`. A reversal is a new row with a negative `amount`, `reverses_id` and `reversal_reason`. |

Numbering uses `school_sequences`: vouchers `V<YYYY><MM>-<5 digits>` per month, receipts
`R<YYYY>-<6 digits>` per year, both gapless within a school.

**Policy** (`settings` key `fees.policy`, editable with `fees.approve`):
`sibling_discount_percent` (default 10), `sibling_rule` (`all` or `second_child_onwards`),
`concession_base` (`gross`, the default, or `after_sibling_discount`), `late_fee_percent`
(default 10), `due_day` (default 15). Siblings are students who share a guardian; the rank for
`second_child_onwards` is by admission date, then date of birth. Policies stored without
`concession_base` read as `gross`. Every policy change is audit-logged as a `setting` entry
with the old and new values.

**Voucher arithmetic** (`crates/core/src/fees.rs`, exact decimals rounded half away from zero
to 2 places at each step):

1. `gross` = Σ structure lines for the student's class (one-time heads only on the first voucher).
2. `sibling_discount` = `gross` × sibling percent when the policy applies to this student.
3. `concession_discount` = Σ active concessions, capped so `net_monthly` ≥ 0. A fixed amount is taken as is. A percent concession is taken on `gross` when `concession_base` is `gross` (the discounts are independent), or on `gross − sibling_discount` when it is `after_sibling_discount` (the discounts stack). Example with a 3,000 fee, 10 % sibling discount and a 25 % scholarship: on gross 25 % × 3,000 = 750, net 1,950; after sibling discount 25 % × 2,700 = 675, net 2,025. The base in force at generation time is used; existing vouchers are not recalculated.
4. `net_monthly` = `gross − sibling_discount − concession_discount`.
5. `arrears` = the unpaid balance of the student's earlier vouchers (including late fees). Those vouchers become `carried` and are no longer payable, so a balance is never counted twice. Carry-forward crosses month and academic-year boundaries.
6. `total_due` = `net_monthly + arrears`.
7. `late_fee` = `net_monthly` × late percent, assessed once the due date has passed, unless waived. Waiving needs `fees.approve`, a reason, and is audit-logged with the approver. Assessment is automatic: a background pass in the server (and the desktop shell) runs at start and again after every local midnight, adding the fee to every `unpaid` or `partial` voucher whose due date has passed and that has no fee yet (`madrasa_db::fees::assess_due_late_fees`), so dashboards, defaulter lists and ledgers show the amount actually owed without anyone touching the voucher. The same pass runs when a payment or a carry-forward touches an overdue voucher, and by hand from `POST /fees/late-fees/run`. A waived voucher is never re-assessed; a zero late fee percent switches the pass off. Every assessment is an audit entry with action `late_fee` (actor empty when automatic) holding the voucher before and after.

Every "today" in the API (dashboard, defaulters, collections, absentees, attendance edit limits,
default dates, the late fee pass) is `madrasa_api::clock::today()`: the school's calendar date,
which in Lite is the local date of the PC the school runs Madrasa on. The UI's date defaults use
the local calendar date as well (`todayIso` in `src/lib/dates.ts`), never the UTC date.

Voucher status: `unpaid`, `partial`, `paid` (derived from `net_paid` vs `total_due + late_fee`),
`carried` (balance moved to a later voucher), `cancelled` (only while nothing was paid).
Ledger totals: billed = Σ gross, discounts = Σ sibling + concession, fines = Σ late fees,
paid = Σ receipts (reversals net out), balance = billed − discounts + fines − paid.

Every change to money (structure, concession, voucher, waiver, payment, reversal) is written to
`audit_log`. Printable voucher (school copy and parent copy with a bank/1Bill/Kuickpay reference
area) and receipt pages render in light colours only; "PDF export" uses the operating system's
print dialog (Save as PDF), there is no separate PDF generator.

### 5.6 Attendance (M1 stage 3)

| Table | Purpose |
| --- | --- |
| `holidays` | Holidays and closures as inclusive date ranges: `name`, `starts_on`, `ends_on`. Weekly off days are the `settings` key `attendance.weekend_days` (ISO weekday numbers, default `[7]` = Sunday). |
| `attendance_sheets` | One per `section_id` and `date` (unique while not deleted): `academic_year_id`, `marked_by`, `submitted_at` and the counts `present`, `absent`, `late`, `leave`. |
| `attendance_entries` | One per enrolled student on a sheet: `student_id`, `date` (copied from the sheet for fast history), `status` (present, absent, late, leave), `note`. |
| `staff_attendance` | One per `staff_id` and `date`: `status` (present, absent, leave), `note`, `marked_by`. |

Rules:

- **Everyone is present by default.** The marking screen sends only the exceptions; the
  server stores one entry per active student enrolled in the section so history, the register
  and corrections are exact.
- **Submission and corrections.** The first save submits the sheet. A later save is a
  correction and needs `attendance.correct` (principal and admin by default); the API answers
  `403 already_submitted` otherwise. Every changed entry is audit-logged with its old and new
  value (`attendance_entry`), and the sheet's counts change is logged too (`attendance_sheet`).
  The history endpoint returns this trail with the names of the user and student.
- **No attendance on weekends or holidays**, and none for a future date. The calendar is
  checked server-side; the UI explains the reason and hides the submit button.
- **Teacher scope.** A teacher may read and mark only sections they are class teacher of or
  teach in (`CurrentUser::section_scope`); the status, absentee and student history views are
  filtered the same way. Admin and principal see every section.
- **Attendance percentage** = (present + late) / (present + absent + late + leave), one
  decimal; `null` until something is marked. Late counts as attended; leave does not.
- **Staff attendance** is simple daily present / absent / leave, saved by users with
  `teachers.manage` (admin, principal) and audited per staff member.

### 5.7 Exams, results and report cards (M1 stage 4)

| Table | Purpose |
| --- | --- |
| `assessment_types` | Per academic year: `name` (unique per year, case-insensitively), `position`, `weight` (NUMERIC(5,2)). Defaults: Bi-monthly Test 1 (15), Bi-monthly Test 2 (15), Mid-year (30), Final (40). Weights are saved together and must total exactly 100. |
| `assessment_max_marks` | Maximum marks per `assessment_type_id`, `class_id` and `subject_id` (NUMERIC(6,2) > 0). Zero removes the line. Locked once any marks exist for that subject in the class. |
| `mark_sheets` | One per assessment, section and subject with `status` draft → submitted → approved, `submitted_by/at`, `approved_by/at`. |
| `marks` | One per sheet and student: `obtained` (NUMERIC(6,2), null until entered) and `is_absent`. |

The grading scheme is the `settings` key `exams.grading`: bands with an inclusive
`min_percent` and `remarks`, plus `pass_percent`. Default: A+ 90, A 80, B 70, C 60, D 50, E 40,
F 0, pass 40. One band must start at 0; band minimums and grade names are unique.

Rules (`crates/core/src/exams.rs`, exact decimals, every stored or shown number rounded half
away from zero to two places):

- Subject percentage in an assessment = obtained / max × 100. Absent counts as 0.
- Assessment total = Σ obtained / Σ max over the subjects that have a maximum; its grade comes
  from the scheme; the position in the section uses competition ranking (1, 1, 3) on the
  percentage.
- Weighted year result of a subject = Σ weight × percentage / Σ weight over the assessments
  that have marks for that subject (an assessment not yet held is left out). The overall
  weighted result applies the same rule to the assessment totals.
- Marks are validated against the maximum (0 ≤ obtained ≤ max, at most two decimals) and the
  student must be enrolled in the section; the subject must be taught in the section's class.
- Workflow: the teacher saves a draft (the sheet is created on first save), submits once every
  enrolled student has a mark or is absent, the principal approves (`exams.approve`) and may
  reopen. Submitted and approved sheets are locked. Every save (with the list of changed marks)
  and every status change is audit-logged.
- Teachers enter marks only for a (section, subject) they are assigned to, or for any subject
  of a section they are class teacher of; result sheets and report cards follow the section
  scope used by attendance.
- Report cards: default design, one page per student, English and Urdu; "PDF export" is the
  print dialog's Save as PDF, as for fee vouchers. They show every assessment (held or not),
  per-subject weighted percentage and grade, per-assessment totals, percentage, grade and
  position, the overall weighted result with remarks and pass/fail, and the year's attendance.

### 5.8 Backup and restore (M1 stage 5)

| Table | Purpose |
| --- | --- |
| `backup_runs` | Every manual or scheduled run: `trigger`, `status` (running, ok, failed), `started_at`, `finished_at`, `file_path`, `file_size`, `error`. |

Settings: `backup.schedule` (`enabled`, default true; `time`, local `HH:MM`, default 02:00;
`directory`, default `Documents/Madrasa Backups`; `keep`, default 30 files) and `backup.keys`
(the wrapped DEK, see §7, now with the X25519 public key derived from the DEK).

**File format** (`.mbk`, `madrasa_core::backup::archive`): magic, a readable JSON header
(created at, app version, school, applied migration versions, record counts, the wrapped key
set, the wrapped file key), the AES-256-GCM payload (gzip of the dump) with the header as
associated data, and a SHA-256 trailer. The file key is random per backup and wrapped for the
school's X25519 public key (ephemeral Diffie–Hellman, HKDF-SHA256, AES-256-GCM), so scheduled
backups never need the password. Any change to the header or payload is detected (checksum,
then the AEAD tag) and the restore is refused as tampered.

**Dump** (`madrasa_db::backup`): every table except `_sqlx_migrations`, parents before
children, as `COPY … TO STDOUT` text inside one repeatable-read transaction. **Restore**
empties every table and reloads the dump in one transaction with user triggers disabled (so the
append-only audit log and the immutable receipts reload), then re-enables them. A dump made with
a different migration set is refused: update the app to the version that made the backup first.
Sessions are part of the data, so everyone logs in again after a restore.

**Rules**
- Scheduled backups run in the server (and the desktop shell) once a day at the configured time
  when none succeeded that day; a failed attempt is retried after an hour. Older files beyond
  `keep` are removed from the folder after each new backup.
- The dashboard shows the last backup; it is a warning when none succeeded in the last 48 hours.
- Restore needs `backup.manage` and the backup password or the recovery key from setup; the
  preview (date, school, app version, counts in the backup and now, schema match) needs no key.
- Changing the backup password re-wraps the DEK; the recovery key keeps working; backups made
  before still open with the password in use when they were made.
- Audit: every run (start and finish), every restore, and every wrong password or recovery key
  (`backup_password_failed`). A restore replaces the audit log with the backup's and then logs
  the restore itself.
- Key sets written before stage 5 have no public key; the Backup page asks for the password once
  to add it, otherwise scheduled backups are skipped with a warning.

### 5.9 Import and export (M1 stage 6)

| Table | Purpose |
| --- | --- |
| `import_batches` | One per applied import: `kind` (students, staff, opening_balances), `file_name`, `status` (applied, rolled_back), `options` (duplicate policy), row counts, `applied_by/at`, `rolled_back_at`. |
| `import_batch_rows` | One per effect: `row_no`, `action` (created, updated, skipped), `entity` (student, guardian, student_guardian, enrollment, staff, fee_voucher), `entity_id`, `old_value` (the fields an update changed). |

Saved column mappings are the `settings` keys `import.mapping.<kind>` (file headings and the
field → column map), reused when a file has the same headings.

Rules (`madrasa_core::imports` for cleanup, `madrasa_api::imports` for the database checks):

- **Files**: `.xlsx`, `.xlsm`, `.xls`, `.ods` (first sheet unless chosen) or CSV/TSV with a
  BOM, comma, semicolon or tab; the first row holds the headings; blank rows are dropped; at
  most 5,000 rows per import. Templates with the field labels and one example row are
  downloadable for each kind.
- **Cleanup**: trim and collapse spaces, strip zero-width characters, Urdu and Arabic-Indic
  digits → Latin; dates in `YYYY-MM-DD`, `DD/MM/YYYY`, `DD-MM-YYYY`, `D.M.YYYY`, `DD Mon YYYY`,
  `Month D, YYYY` and Excel serial numbers (day first for numeric dates, month first for a country that reads them that way, e.g. the US; `madrasa_core::locale`); phone and CNIC/B-Form
  normalised as in §5.4; gender and relationship words in English and Urdu; money with `Rs`
  and thousands separators; class names matched tolerantly (`Class 1`, `Grade 1`, `1st`, `I`,
  `One`, `جماعت 1`, `KG`, `Nursery`, `Prep`) and `1-A` / `1 A` / `Class 7 (B)` split into class
  and section.
- **Validation** (row by row, errors and warnings): required fields, formats, future dates,
  class and section must exist in the active year (a class with one section is used
  automatically), duplicates inside the file (student number, CNIC, or name + date of birth +
  father's name), and matches with existing records (students by number or the §5.4 duplicate
  rules; staff by number or CNIC). Rows with errors are never imported; the wizard lets the user
  fix cells or download the error rows as CSV.
- **Duplicates**: `skip` (default), `update` (every non-empty value of the row) or `merge`
  (only fields that are empty on the record). Updates store the old values for rollback.
- **Two guardians and families**: a register may carry two numbered guardian column groups
  (`Guardian 1 Name/Relationship/Phone/Email/ID`, `Guardian 2 …`; `Parent 1`, `Primary
  guardian`, `Secondary guardian` and unnumbered headings map to the first) and a `Family ID`
  (`family_id`, also "household", "family no"). The second guardian is linked as a
  non-primary contact. A family id is stored in `custom.family_id` on the student and on the
  guardians created for that family; a later row with the same family id and guardian name
  reuses the guardian record (before the CNIC/phone match), so siblings arrive linked and the
  sibling discount applies.
- **Guardian matching never merges on a phone alone.** An existing guardian is reused when
  the family id and name match, else when the ID number matches, else when the phone **and**
  the name match (case-insensitive). A phone that is on file under a different name, in the
  database or earlier in the same file, yields a preview warning and a separate guardian
  record, so unrelated households sharing a phone never become siblings and never get the
  sibling discount. The student form follows the same rule.
- **Guardians**: a row's guardian is linked to an existing guardian with the same CNIC or
  phone (so siblings share one record), otherwise created; a phone without a name is the
  father's.
- **Opening balances**: by student number or legacy ID; each becomes an unpaid voucher dated
  on the first day of the active year with one line under the "Opening balance" head (created
  on first use), so later months carry it forward like any other balance.
- **Batches and rollback**: an import runs in one transaction and is audited; rollback
  soft-deletes what it created (vouchers are cancelled) and restores the stored old values of
  what it updated, newest effect first, and is audited too.
- **Export**: students, staff, fee vouchers, defaulters, absentees, the monthly attendance
  register and result sheets as `.xlsx` (bold headings, frozen row) or `.csv` (UTF-8 with
  BOM so Excel shows Urdu), with the same filters, permissions and teacher scope as the list.

### 5.10 Year rollover and dashboard (M1 stage 7)

**Rollover** (`madrasa_db::rollover`, `academics.manage`): from the active year, open the
next one (same start month) and, in one transaction:

- promote every active student to the class with the next level, into a section of the same
  name (created on demand with the old section's class teacher and capacity); students of the
  top class are marked `graduated` with `left_on` = old year end; students listed as
  *retain* repeat their class, students listed as *graduate* leave instead;
- copy the fee structure of every class and the assessment types (names, weights, order);
- activate the new year (the old one stays, inactive, with all its data);
- audit the run with the counts.

Unpaid balances need no step of their own: the first voucher of the new year carries them
forward (§5.5). The preview shows each section's destination, the graduating count, the
students with a balance and its total, whether the next year already exists (then the
rollover is refused) and how many months the current year has run (the UI warns under nine).

**Dashboard** (`/dashboard`): today's day kind and attendance (sections marked, percentage,
unmarked sections), active students and staff, fees collected since the first of the month
(net of reversals), defaulters past their due date with the outstanding total, and the backup
status. Each figure is present only for callers with the matching read permission.

**Pilot profile check**: `cargo test -p madrasa-demo-data --test perf -- --ignored --nocapture`
loads 1,200 students, 120 staff and a full year of fees and attendance and asserts that every
key query answers in under one second (results in `docs/PROGRESS.md`).

### 5.11 Report library (M2 stage 1)

Reports are read-only. There is no table; every run is an audit entry with action `report`
(`new_value`: the report code, its parameters and the row count). Queries live in
`madrasa_db::reports` and take the school, the academic year and, for teachers, the section
scope.

A report is built as a `madrasa_api::reports::Report` document: code, language, school
name, title, subtitle, the parameters shown in the header, columns (label, alignment,
stretch), groups (optional heading, rows of strings, optional subtotal), a total row, notes,
orientation and the run date. Renderers read only this document:

- JSON for the screen (`format=json`, the default);
- Excel through the export module (`format=xlsx`): group headings and subtotals become rows;
- PDF (`format=pdf`): typst markup generated from the document and compiled in-process with
  Inter and Noto Nastaliq Urdu compiled into the executable (`crates/api/fonts`, OFL). Cells
  are emitted as string literals, so data is never read as markup. Urdu reports set
  `lang: ur`, `dir: rtl` and Nastaliq first; English ones use Inter with Nastaliq as the
  fallback for Urdu names. A4, portrait or landscape per report, repeated table header, page
  numbers in the footer.

Every heading, column label and fixed value exists in English and Urdu in one table
(`reports/text.rs`); the UI passes `lang`. Dates are `DD-MM-YYYY`.

Student reports (stage 1): `students.list` (active by default, grouped by class and section,
primary guardian and phone, subtotals), `students.strength` (every section of the year with
boys/girls/other/total, class teacher and capacity), `students.movements` (admissions and
withdrawals between two dates), `students.contacts` (one row per guardian link),
`students.birthdays` (a calendar month, by day, with the age turned), `students.missing`
(students lacking CNIC/B-Form, photo, guardian, guardian phone, enrolment, roll number or
address).

Fee, attendance and exam reports (stage 2) use the same document and renderers. Fees
(`reports.read`): `fees.collection` (every receipt in a period by day, reversals as negative
amounts), `fees.monthly`, `fees.defaulters` (the defaulter query with ageing buckets 1–30,
31–60, 61–90, over 90 days as notes), `fees.ledger` (one student by student number: vouchers
on their issue date, receipts on their payment date, running balance), `fees.concessions`,
`fees.vouchers` (a billing month), `fees.late_fees` (assessed and waived), `fees.heads`
(billed vs collected per head; payments are split across a voucher's lines in proportion,
stated in a note). Attendance: `attendance.register` (one section and month, a column per
day: P/A/L/Lv, — for a marked day without an entry, H for weekends and holidays),
`attendance.percent` (per section, attendance % = (present + late) ÷ marked),
`attendance.absentees` (a period, grouped by day), `attendance.below` (students under a
percentage threshold), `attendance.staff` (a month, `reports.read` only). Exams:
`exams.result_sheet`, `exams.merit` (top N across the class's sections, competition
ranking), `exams.analysis`, `exams.grades` (one column per grade band), `exams.failing`
(below the pass percentage, grouped by subject) and `exams.report_cards` (summary table on
screen and in Excel; the PDF typesets one card per page with the same content as the
on-screen card). Exam reports are computed from the exams module's own result sheets and
report cards, so figures match the screen; unapproved sheets add a note.

Access: a report's `access` is `reports_read`, `attendance_read` or `exams_read`. Office
roles with `reports.read` run everything; a teacher runs attendance and exam reports with
the module's read permission, limited to the sections they are class teacher of or teach in
(the catalogue only lists what the caller may run). Dashboard shortcuts open today's
collection and today's absentees with the dates filled in and the report run (`?run`).

### 5.12 Custom report builder (M2 stage 3)

| Table | Purpose |
| --- | --- |
| `report_definitions` | A saved custom report: `name` (unique per school, case-insensitively), `description`, `source`, `definition` (JSONB), `owner_user_id`, `is_shared`, sync columns. |

The definition names a **source** and picks **columns**, **filters**, **sort**, an optional
**group_by** field and a **limit** from a fixed catalogue (`madrasa_api::reports::builder`).
Sources: students, guardians, fee vouchers, fee receipts, fee balances (per student),
attendance entries, attendance summary (per student), marks, staff. Every field has a type
(text, number, money, date, boolean, choice with its allowed values), a label in both
languages and a SQL expression owned by the catalogue. Operators depend on the type (text:
is, is not, contains, starts with, is one of, is empty, is not empty; number/money: is,
greater than, at least, less than, at most, between, …; date: is, before, after, between;
boolean: yes/no; choice: is, is not, is one of). SQL is assembled only from catalogue
expressions; every user value is parsed by the field's type and bound as a parameter, so
nothing typed by the user reaches the database as SQL. Every source is restricted to the
school, the chosen academic year and the caller's section scope (teachers who are granted
`reports.read` see their own sections; staff is never scoped). Grouping adds a heading per
value, a subtotal with the row count and sums of numeric columns, and a total row. The
limit defaults to 500 and is capped at 5000; a report that hits it says so in a note.

Permissions: `reports.read` lists the catalogue, previews and saves private reports, and
runs any report the caller may see (shared ones and their own); `reports.manage` (principal
and admin) saves shared reports and may edit or remove anyone's. Create, update, delete and
every run are audit-logged (`report_definition`).

### 5.13 Examination boards (M2 stage 4)

| Table | Purpose |
| --- | --- |
| `exam_boards` | A board (BISE, Federal Board, AKU-EB, Cambridge…): `name` (unique per school), `short_code`, `country`, `grading_mode` (marks, grades, both), `pass_percent`, `grade_bands` (JSONB `[{grade, min_percent}]`, highest first). |
| `board_exams` | One sitting of a board in an academic year: `name` (unique per board and year), `status` (announced → results_imported → closed). |
| `board_exam_classes` | The classes that sit the exam. |
| `board_exam_subjects` | The exam's papers: `name` (unique per exam), optional `subject_id` (the school subject it corresponds to), `max_marks`, `position`. Results point at these, so renaming keeps results. |
| `board_candidates` | A student registered for an exam with `roll_no` (unique per exam), `registration_no`, `group_name`; pre-filled from the student's `board_roll_no`/`board_registration_no`, and written back when edited. |
| `board_results` | One paper's result per candidate: `obtained`, `max_marks`, `grade`, `status` (pass, fail, absent, withheld), `remarks`, `batch_id` (the import that wrote it). |

Rules (`madrasa_core::boards`): a paper's status comes from the board when the file gives
one, otherwise pass/fail against the board's pass percentage (absent when no marks); the
grade is the board's when given, otherwise the band for the paper's percentage. A candidate
is withheld if any paper is, absent if every paper is, fails if any paper failed or was
missed or the overall percentage is under the pass percentage, and passes otherwise; the
overall grade comes from the bands; positions use competition ranking among candidates with
results. Import: Excel/CSV in either layout (one row per candidate with a column per paper,
or one row per candidate and paper), matched by roll number then registration number;
column matching is guessed from the headers and remembered per board; preview reports every
row's errors (unknown roll, unknown paper, marks outside 0–max, bad numbers) and nothing is
written while a row has an error; an import is an `import_batches` row of kind
`board_results` with one `import_batch_rows` entry per result (created or updated with the
old value), and rolls back like any other batch. Importing moves the exam to
`results_imported`. Removing an exam removes its classes, papers, candidates and results;
a board can only be removed once it has no exams.

Reports (library, `exams.read`): board result sheet (per exam, optional class), board merit
list (top N), board pass rates (by paper and by section), student board result (one
candidate). The student profile's Results tab shows board results under the school's own.

### 5.14 Terms and assessment components (M2 stage 5)

| Table | Purpose |
| --- | --- |
| `terms` | Per academic year: `name` (unique per year, case-insensitively), `position`, optional `starts_on`/`ends_on` (the end may not precede the start), `weight` in the year (NUMERIC(5,2); a year's term weights are saved together and total 100). |
| `assessment_types` | Now the **components**: gains `term_id`, `kind` (`entered` or `calculated`) and `formula` (JSONB, calculated only). Entered components keep maximum marks and mark sheets exactly as before. Calculated components have no sheets and no maximum marks; their marks are evaluated on read. |

Formula (`madrasa_core::exams::Formula`): `{op, inputs: [{assessment_type_id, weight?}],
n?}` with `op` one of `sum` (Σ obtained out of Σ max), `average` (mean of the inputs'
percentages, out of 100), `weighted` (weighted mean of percentages by the inputs' weights,
out of 100) and `best_of` (mean of the best `n` percentages, out of 100). Inputs must be
components of the same term, distinct and not the component itself; a calculated input is
allowed but a cycle through calculated components is refused; an input not yet held is left
out, and the result is empty while no input is held. Every figure is rounded half away from
zero to two places.

Results follow the structure. A component's percentage per subject is as before; a term's
result for a subject is the weighted mean of the term's component percentages over the
components held (weights within a term total 100); the year result is the weighted mean of
the term results by term weight. With one term this is exactly the M1 calculation. Result
sheets and report cards treat a calculated component like any other column; report cards
(screen and PDF) with more than one term show a term header row and a term result row with
the term's percentage and grade.

Migration `0014_terms` creates one term "Year" (weight 100) for every year that has
assessment types and attaches them to it, so every existing setup, sheet, result and report
card is unchanged. `POST /exams/assessments/defaults` takes `terms` (1 or 2, default 1 for
compatibility): two terms gives Term 1 and Term 2 (50 each) with Bi-monthly Test 1 (30) +
Mid-year (70) and Bi-monthly Test 2 (30) + Final (70); the Setup page proposes two terms for
a new year. A term with components cannot be removed.

### 5.15 Subject allocation per student (M2 stage 6)

| Table | Purpose |
| --- | --- |
| `class_subjects` | Gains `is_elective` (default false): a class subject is compulsory unless marked elective. |
| `subject_groups` | A named set of electives per class (`name` unique per class, `position`), e.g. Pre-Medical, Computer Science. |
| `subject_group_subjects` | The electives in a group (only electives of the class). |
| `student_subjects` | The electives allocated to an enrolment (`enrollment_id`, `subject_id`); the enrolment also records the chosen `subject_group_id` as a label. |

Rule (`madrasa_core::subjects::effective_subjects`): a student with no allocation takes
every subject of the class, exactly as before; a student with an allocation (a group or at
least one elective) takes the class's compulsory subjects plus the allocated electives; an
allocated subject the class no longer teaches is ignored. Choosing a group stores the
group's electives; the electives can then be changed one by one. Clearing the allocation
returns the student to "all subjects". Every change is audit-logged with the old and new
allocation.

Exams follow the allocation: marks rosters, the Excel marks template and upload, and
submission consider only the students who take the subject; marks for a student who does not
take the subject are refused; a subject nobody in the section takes has no sheet; result
sheets mark the cells of subjects a student does not take (`takes = false`) and total over
the student's own subjects, with the subject analysis counting takers only; report cards and
the exam reports show the student's own subjects. Removing a group keeps the students'
subjects and drops the label.

Import: an Excel/CSV file with a student number column and a group and/or electives column
(codes or names separated by commas); rows are matched to the active year's enrolment, the
group by name within the student's class, and electives against the class's electives
(compulsory subjects are a warning, unknown ones an error). Nothing is written while a row
has an error; applied rows form an `import_batches` batch of kind `student_subjects` whose
rows hold the previous allocation, so the batch rolls back like any other.

### 5.16 Allocation screens (M2 stage 7)

No new tables. The teacher grid edits `section_teachers` one cell at a time
(`madrasa_db::allocation::set_cell_teacher`: the staff member becomes the only teacher of that
subject in that section, or the cell is cleared; audited as `section_teacher_cell` with the
previous and new staff ids); the class teacher stays on `sections.class_teacher_id`. Bulk
moves re-point the student's enrolment at another section of the same class and year through
`students::enrol` (audited as before); roll numbers are either kept or replaced by the next
free number in the new section. Attendance entries and marks reference the student and the
sheet they were recorded on, so they stay with the student after a move.

Balancing (`madrasa_core::allocation::balance`): given the sections of a class with their
students in roll order, propose moves so that sizes differ by at most one. Every section
gets `total / n` students and the largest sections keep the remainder, so a balanced class
needs no moves; moves are taken from the end of the roll of the most over-full section into
the most under-full one, never touching excluded (ticked) students; the proposal is
deterministic and is shown as a preview before it is applied. Capacity is shown but not
enforced.

### 5.17 Consent manager (M2 stage 8)

| Table | Purpose |
| --- | --- |
| `consent_types` | Defined by the school: `name` (unique per school), `description`, `given_by` (guardian, student, either), `required` (none, all, class_range) with `min_level`/`max_level`, `renewal` (none, yearly), `position`. |
| `consents` | Append-only history per student: `consent_type_id`, `status` (given, refused, withdrawn), `guardian_id` (who gave it, when a guardian did), `academic_year_id` (the year it was recorded in), `recorded_on`, `recorded_by` (the user), `note`. A new status is a new row; the old one stays. Every row is audit-logged. |

Rules (`madrasa_core::consents`): a consent is needed by a student when its type is required
for everyone, or for a class range that includes the student's class level (a student with
no enrolment needs only the consents required for everyone). The current state comes from
the latest row: given, refused or withdrawn as recorded; expired when the type renews yearly
and the latest row belongs to another academic year; missing when nothing is recorded.
Missing, expired and withdrawn consents count as "to collect". A consent given by a guardian
must name the guardian (one linked to the student); a refusal need not.

Reports (library, students module, `reports.read`): missing consents (by section, one row
per student and consent to collect, optional consent type), consents expiring with the year
(yearly consents given this year), and the consent form list (per section, the students with
the consents still to collect, their guardian and phone, and a signature column). Removing a
type hides it and its consents from the screens and reports; the rows remain in the history
tables and the audit log.

### 5.18 Timetable (Standard stage 1)

Standard only (`Feature::Timetable`; every route 403 `feature_unavailable` on Lite; the
tables stay empty there). The scheduling engine is `madrasa_core::timetable`, ported from
quietbytehq/timewizer (Apache-2.0, same owner; ADR 0003): the school's days, shifts, master
data, lessons and rules compile into a model the solver fills by difficulty order with
ejection chains, repair and (optionally) relaxation of soft rules, then an independent
`verify` checks the result. `crates/db::timetable::setup_for_year` maps Madrasa's entities
onto the engine with UUID strings as ids: sections become classes (named "Class Section"),
staff become teachers (active staff plus anyone a lesson still names), subjects, rooms, the
year's lessons and rules, the chosen timetable's cards and the substitutions.

Data (all per school, sync columns, soft delete, audited):

- `timetable_shifts` (per academic year): name, enabled, `days` (seven entries, Monday
  first: `{working, periods}` so Friday can be short), `period_times` (`[{start, end}]`,
  HH:MM), `breaks` (`[{after_period, name, cannot_cross}]`), position. The default morning
  shift is Mon–Thu 7 periods of 40 minutes from 08:00, Fri 5, a break after period 3.
- `rooms`: name, short name, building, capacity (optional; unique name per school).
- `timetable_lessons` (per year): subject, `teacher_ids[]`, `section_ids[]` (several =
  a joint class), `room_ids[]` (the pool; empty = the section's home room or any room),
  `periods_per_card` 1–3, `cards_per_week` 1–40. Lessons can be seeded from the teacher
  allocation grid (`section_teachers` rows with a subject), one per (section, subject)
  without a lesson.
- `timetable_rules` (per year): one row per `kind` (`school` | `teacher` | `section` |
  `room`) and entity. Teachers: `shift_ids` they teach in (empty = all), `limits`
  `{max_per_day, max_per_week, max_consecutive, max_days_per_week}`, `time_off`
  `[{shift_id, day, period, state}]` (1 = avoid, 2 = never). Sections: the shift attended
  (`shift_ids[0]`, default the first enabled shift), `home_room_id`, time-off. Rooms:
  time-off. School: `limits` = `{default_max_per_day, default_max_per_week,
  max_same_subject_per_day, allow_relaxation, improve_seconds}`.
- `timetables` (per year): name, `status` draft | published (at most one published per
  year), `report` (the generator's JSON: placed, unplaced with title/message/fix, quality,
  relaxed rules), `generated_at`; `timetable_cards`: lesson, session number, shift, day
  (0 = Monday), period (1-based), length, room, `locked`.
- `timetable_absences` (per year): staff, `from_date`–`to_date`, `periods` (empty = whole
  days, else `[{shift_id, period}]`), note; `timetable_covers`: per absence, date, lesson
  and session: `cover_staff_id` (NULL = no cover needed), shift/period/length as decided,
  note. Removing an absence removes its covers.

Generation: `POST /timetable/generate` refuses while the pre-check (`check::run`) reports
errors (400 with the findings in `details`), otherwise starts a background run
(`AppState::timetable_jobs`) with a time limit (1–600 s) and an improvement phase; the
screen polls `GET /timetable/generate/{id}` for the phase, progress and, when done, the
report. The result replaces the timetable's cards (locked cards are placed first and never
moved). Manual edits on any timetable: slot options per session (ok / soft with reasons /
blocked), move (with a room), swap options and swap, lock/unlock; every save re-runs the
checker and the view carries its `problems`. Publishing makes a timetable the one the
school runs on; the previously published one goes back to draft; a published timetable
cannot be deleted.

Views: one `TimetableView` (shifts, lessons with names, cards, rooms, sections, teachers,
problems, running job) drawn by section, teacher or room in the UI; `/timetable/current`
is the published one, `/timetable/mine` the logged-in teacher's own week (login linked to
a staff record) with their cover duties for this week and next, shown on the Dashboard.
Printouts (PDF, en/ur, light) through the report engine: section, teacher and room grids
(`/timetable/print`), and the library reports `timetable.section`,
`timetable.teacher_load`, `timetable.free_rooms` (one weekday) and `timetable.cover_sheet`
(a date range), all `timetable.read` and hidden on Lite.

Substitutions: `GET /timetable/substitutions?date=` lists, from the published timetable,
every session an absent teacher would teach that day with candidates (free teachers,
same subject first, fewest covers this year first; unsuitable ones carry reasons), the
covers decided, the year's cover counts per teacher and the absences. `PUT /timetable/covers`
accepts only a listed, suitable candidate (or `null` = no cover needed); `DELETE` withdraws
a decision. The cover sheet (`/timetable/cover-sheet`, JSON or PDF) lists per day what is
covered and what is not yet.

Permissions: `timetable.read` (every built-in role) for the views, printouts, substitutions
and reports; `timetable.manage` (principal, admin) for setup, lessons, rules, generation,
edits, publishing, absences and covers.

Not in this stage: attendance per period (a period-marked sheet reusing the daily engine)
would change the Lite attendance tables and is deferred to a later stage.

### 5.19 Leave (Standard stage 2)

Standard only (`Feature::Leave`; every route 403 `feature_unavailable` on Lite). Rules live in
`madrasa_core::leave`: working days of a range (the school's weekend days and holidays are
skipped), half days (a single date, 0.5 day), balances (`granted = entitlement + carried over
+ adjustment`, `available = granted − approved`, `remaining = available − pending`), carry-over
(`min(max(available, 0), carry_over_max)`), request states (pending → approved | rejected |
withdrawn; approved → withdrawn) and overlap checks.

Data (per school, sync columns, soft delete, audited):

- `leave_types`: name, code, `entitlement_days` and `carry_over_max` (NUMERIC(5,1)),
  `needs_approval`, position. A type with pending or approved requests cannot be removed.
- `leave_balances` (per staff member, type and academic year): `entitlement` (NULL = the
  type's), `carried_over`, `adjustment`, note. Balances are listed for every active staff
  member × type even without a row; `used` and `pending` are summed from the requests.
- `leave_requests`: staff, type, year (the one the first date falls in; the range must end
  in it), `from_date`–`to_date`, `half_day` (am | pm), `days`, reason, attachment note,
  status, requested_by, decided_by/at, decision note. A request is refused when it
  overlaps the staff member's pending or approved requests, falls only on weekends or
  holidays, or takes more than `remaining`. A type without approval is approved at once.
- Approval writes `staff_attendance` rows (`leave`, note "Leave: <type>", `leave_request_id`)
  for every working day, audited; hand marking never overwrites such a row; withdrawing an
  approved request soft-deletes them (the days are unmarked again).
- `student_leave_requests`: student, year, dates, reason, guardian (must be linked),
  status, recorded_by, decided_by/at, note; overlaps are refused. Approval marks the
  student's entry `leave` with the note "Leave (approved)" and `leave_request_id` on every
  existing sheet of the range and recounts the sheets; sheets saved later for those dates
  mark the student `leave` whatever they say (`attendance::save_sheet` consults
  `leave::approved_student_leave_on`; the table is empty on Lite); a correction keeps it;
  withdrawal turns the marked entries into absences.
- Staff attendance: `late` with `arrived_at` (TIME) is accepted only when the feature is on;
  the monthly staff register (`/leave/staff-register`) shows a cell per day with the code,
  the arrival time and whether an approved request wrote it.
- Setting `leave.approver_staff_id`: a staff member who may decide staff requests besides
  `leave.approve` holders. Nobody decides their own request without `leave.approve`.

Permissions: `leave.read` (every role: own requests and balances, student leave for own
sections for teachers, the register); `leave.manage` (office: types, balances, carry-over,
requests for others, student requests, withdrawals); `leave.approve` (principal, admin:
staff decisions). Teachers decide student leave for the sections they are class teacher of
or teach. Reports (`leave.read`, hidden on Lite): leave balances, staff leave register
(date range, optional status) and student leave register (class/section, date range).

### 5.20 Admissions pipeline (Standard stage 3)

Standard only (`Feature::Admissions`; 403 `feature_unavailable` on Lite). Rules live in
`madrasa_core::admissions`: stage kinds (`open`, `admitted`, `rejected`, `withdrawn`), the
default pipeline (enquiry → applied → test scheduled → interviewed → offered → admitted |
rejected | withdrawn), which moves are allowed (an admitted application stays admitted;
rejected and withdrawn ones can be reopened; a converted application is frozen) and the
funnel (per stage: applications that reached it and that sit in it now).

Data (per school, sync columns, soft delete, audited):

- `admission_stages`: code (a slug of the name), name, kind, position; created with the
  defaults on first use, then renamed, reordered, added or removed (only when empty).
- `admission_applications`: `application_no` (`APP-<year>-<seq>`), the session applied for
  (`academic_year_id`), `class_id`, `stage_id`, enquiry date and source, the applicant
  (same shape as a student: name, father, date of birth, gender, CNIC, address, phone,
  email, previous school), the guardian (name, relationship, phone, CNIC, email,
  occupation), test (date, marks, total), interview (date, notes), offer (date, fee,
  valid until), notes, `custom`, and after conversion `student_id` and `admitted_on`.
- `admission_events`: every stage change dated, with the note and who made it (the
  creation is the first event). Moves are audited too.
- Setting `admissions.fee_head_id`: the fee head of the admission fee voucher.

Conversion (`POST /admissions/applications/{id}/convert`, `admissions.manage` +
`students.manage`): only from an admitted stage; needs the date of birth and the father's
name; checks duplicates like the student form (409 `duplicate` with the matches unless
`force`); creates the student (student number allocated or given, `custom` carries the
application number and previous school), the enrolment in the chosen section of the
application's year, the guardian (an existing one matched by CNIC/phone/name is reused and
linked), and, when a fee head is configured and the application has an offer fee, a
one-off voucher (`fees::create_one_off_voucher`: issued and due on the admission date);
then links the student to the application. A converted application cannot be moved,
edited in its identity, deleted or converted again.

Enquiries import: `ImportKind::Enquiries` in the import engine (`/imports/*` with kind
`enquiries`, `admissions.manage`, feature-gated): applicant, father, date of birth,
gender, class applied for (must exist), enquiry date, source, phone, email, address,
previous school, guardian name/phone/CNIC, notes; rows become applications in the first
open stage with source `import`; the batch rolls back by removing them.

Screens: pipeline board (a column per stage with the applications, counts in stage and
reached), applications list (search, stage and class filters, ticks and a bulk move), the
application dialog (applicant, guardian, test and interview, offer, history, move, offer
letter PDF en/ur, delete, convert with section, roll number, admission date and student
number), stages and the fee head (`admissions.manage`). Reports (`admissions.read`, hidden
on Lite): admissions funnel, applications by class, pending tests.

### 5.21 Library (Standard stage 4)

Standard only (`Feature::Library`; 403 `feature_unavailable` on Lite). Rules live in
`madrasa_core::library`: member kinds (`student`, `staff`), loan rules per kind (max books,
loan days, fine per day, max renewals; validated so a loan always has a due date), the due
date, days late, the fine for a return (days late × fine per day, zero when the loan has a
waiver covering it), whether a member may borrow (under the book limit, nothing overdue),
renewal (from the current due date, refused past the renewal limit or when overdue) and the
waiver cap (never more than the fine).

Data (per school, sync columns, soft delete, audited):

- `library_books`: title, author, publisher, ISBN, category, language, subject, year,
  `custom`.
- `library_copies`: `book_id`, `accession_no` (unique per school, case-insensitive; the
  barcode text), shelf, condition (`good`/`fair`/`poor`), status (`available`/`on_loan`/
  `lost`/`withdrawn`).
- `library_loans`: copy, member kind and id, issued and due dates, returned date,
  renewals, who issued and returned, `lost`, `fine`, `fine_waived`, `waiver_reason`,
  `waived_by`, `fine_voucher_id`, `fine_settled_on`.
- Settings `library.rules` (the two rule sets; defaults 2 books / 14 days / 5 per day / 1
  renewal for students and 5 / 30 / 0 / 2 for staff) and `library.fine_fee_head_id`.

Desk flow: look up a copy by accession number (`GET /library/copies/lookup`) and get it
with its open loan; issue it to a member (refused when the copy is not available, the
member is at the limit or has an overdue book); renew; return (the fine is computed on
return; a lost return marks the copy lost and charges the replacement amount given); waive
part or all of a fine with a reason; settle a fine (for a student with a fee head
configured, a one-off voucher is raised through `fees::create_one_off_voucher` and linked;
otherwise the fine is marked settled at the desk). Members are students (active
enrolment) and staff; the member page lists their loans, counts, fines due and the rules
that apply. Stock take: counts by category and every copy with its status, which
`library.manage` can set to available, lost or withdrawn (never on a copy that is out).

Permissions: `library.read` (principal, admin, accountant, teacher, librarian),
`library.manage` (principal, admin, librarian). The `librarian` login role exists on
Standard and Nexus only. Screens: Desk, Catalogue (search, book dialog with copies by
accession number, shelf and condition), Loans (open, overdue, returned, fines due; return,
waive and settle), Stock, Settings. Reports (`library.read`, hidden on Lite): overdue
books, fines, most borrowed, stock by category.

### 5.22 Transport (Standard stage 5)

Standard only (`Feature::Transport`; 403 `feature_unavailable` on Lite). Rules live in
`madrasa_core::transport`: which months an assignment is charged for (any month it touches
between its from and to dates), the monthly charge (the assignment's own amount, else the
stop's, else the route's), and the capacity warning of a route (`full` when every seat is
taken, `over` by the number of students beyond the vehicle's seats; none without a vehicle
or capacity).

Data (per school, sync columns, soft delete, audited):

- `transport_vehicles`: registration number (unique per school, case-insensitive), make and
  model, capacity, driver and helper with phones, active, notes, `custom`.
- `transport_routes`: name (unique per school), code, vehicle, default `monthly_fee`,
  active, notes.
- `transport_stops`: route, name, position, pickup and drop times, optional `monthly_fee`
  override. Saved as an ordered list with the route: existing ids are kept, new ones added,
  missing ones removed unless a student is still assigned to them (409).
- `transport_assignments`: student, academic year, route, stop, from and to dates, optional
  `amount` override, notes. One open assignment per student and year: assigning a student
  again ends the previous assignment the day before the new one starts (or replaces it when
  it had not started), so the history stays.
- Setting `transport.fee_head_id`: the fee head of the monthly charge.

Fees: `fees::generate_vouchers` asks `transport::monthly_charge` for every student; when a
fee head is set and the student has an assignment covering the billing month, the charge
joins the voucher as a line under that head (and counts as structure, so a student with
transport but no class fee still gets a voucher). The charge is the assignment in force
latest in the month. Lite installs have no assignments, so nothing changes there.

Screens: Routes (list with vehicle, stops, students against capacity, monthly fee; a dialog
with the route and its stops in order), Assign (students of a year and section with their
stop and charge, tick several and put them on a stop from a date, end a student's
transport), Roster (one route by stop with student numbers, names, classes and guardian
phones, printable, with the capacity badge), Vehicles, Settings (fee head,
`transport.manage`). Reports (`transport.read`, hidden on Lite): route manifests (every
active route grouped with its stops), students without transport (class or section
optional), transport fee by route for a month.

## 15. Standard alpha (overview)

The stages, their order and their acceptance are in `docs/TASKS-STANDARD.md`; each stage adds
its data model and endpoints to this document when it is built. Shared decisions:

- **Edition gating.** Every Standard module has a feature code in `madrasa_core::edition`
  (`timetable`, `leave`, `admissions`, `library`, `transport`, `hostel`, `inventory`, `hr`,
  `directory_login`, `readonly_api`, plus `campuses`), listed in `STANDARD_ONLY`. Lite
  enables exactly its nine core modules; Standard and Nexus enable the rest. Every Standard
  router checks `AppState::require_feature` (403 `feature_unavailable` on Lite) and the UI
  hides the module from the sidebar. Migrations run on every edition; the tables stay empty
  on Lite and no Lite screen or endpoint changes.
- **Edition switch.** Settings → Edition (`settings.manage`, `GET/PUT /settings/edition`)
  moves an instance between Lite and Standard. The API reloads its flags at once and gives
  the built-in roles the default grants they lack for the new edition (`roles::sync_defaults`
  never removes a grant). Nexus is an installation choice and is not switched here. Every
  switch is audited (`edition`, old and new).

## 6. API conventions

- Base path `/api/v1`. JSON request and response bodies. `snake_case` keys.
- Authentication: `Authorization: Bearer <session token>`. Tokens are 32 random bytes,
  base64url; the server stores only a SHA-256 hash. Default lifetime 12 hours, sliding.
- Errors: `{ "error": { "code": "...", "message": "..." } }` with an appropriate HTTP status
  (`400 validation`, `401 unauthenticated`, `403 forbidden` / `feature_unavailable`,
  `404 not_found`, `409 conflict`, `423 locked`, `500 internal`). Messages are plain language.
- Every endpoint except `/health`, `/setup/*` (before setup is complete), `/auth/login` and
  `/system/features` requires a session and a permission check in the handler. Permission
  checks live in the backend; the UI only hides things.
- Feature-gated endpoints return `403 feature_unavailable` when the edition does not enable
  the feature.
- Pagination: `?page=1&page_size=50` with `{ items, page, page_size, total }`.

### 6.1 M0 endpoints

| Method | Path | Auth | Purpose |
| --- | --- | --- | --- |
| GET | `/health` | none | `{ status: "ok", version, database: "ok" }` |
| GET | `/system/features` | none | `{ edition, features: [...] }` |
| GET | `/setup/status` | none | `{ needs_setup }` |
| POST | `/setup` | none, only while `needs_setup` | Creates school, settings, admin account, backup keys; returns recovery key once |
| POST | `/auth/login` | none | `{ username, password }` → `{ token, expires_at, user }` |
| POST | `/auth/logout` | session | Revokes the token |
| GET | `/auth/me` | session | Current user, roles, permissions |
| GET | `/users` | `users.manage` | List users (admin only) |
| GET | `/schools/current` | `school.read` | School profile |
| GET | `/audit` | `audit.read` | Recent audit entries |
| GET | `/nexus/status` | `nexus.admin`, feature `nexus_hub` | Stub, proves feature gating |

### 6.2 M1 stage 1 endpoints

| Method | Path | Permission | Purpose |
| --- | --- | --- | --- |
| GET/POST | `/academic-years`, POST `/academic-years/{id}/activate` | `academics.read` / `academics.manage` | Academic years |
| GET/POST/PATCH | `/classes`, `/classes/{id}`; GET/PUT `/classes/{id}/subjects` | `academics.read` / `academics.manage` | Classes and their subjects |
| GET/POST/PATCH | `/sections`, `/sections/{id}` | `academics.read` / `academics.manage` | Sections, class teacher |
| GET/POST/PATCH | `/subjects`, `/subjects/{id}` | `academics.read` / `academics.manage` | Subjects |
| GET/POST/PATCH | `/staff`, `/staff/{id}`; GET/PUT `/staff/{id}/assignments` | `teachers.read` / `teachers.manage` | Staff profiles and teaching assignments |
| GET | `/students` | `students.read` (teachers: own sections) | Directory: `q`, `class_id`, `section_id`, `status`, `sort`, `page`, `page_size` |
| POST | `/students` | `students.manage` | Create with guardians and enrollment; duplicate check |
| POST | `/students/check-duplicates` | `students.manage` | Matches for the add form |
| GET/PATCH | `/students/{id}` | `students.read` scope / `students.manage` | Profile (guardians, enrollment, siblings) |
| POST | `/students/{id}/withdraw` | `students.manage` | Status left/graduated (no hard delete) |
| GET/PUT | `/students/{id}/photo` | scope / `students.manage` | Photo (JSON base64, ≤ 512 KB) |
| POST/DELETE | `/students/{id}/guardians`, `/students/{id}/guardians/{guardian_id}` | `students.manage` | Link/unlink guardians |
| GET/POST/PATCH | `/guardians`, `/guardians/{id}` | `students.read` / `students.manage` | Guardian search (phone, CNIC, name) and edit |
| GET/PUT | `/settings/school` | `settings.manage` | Student number prefix |

### 6.3 M1 stage 2 endpoints

| Method | Path | Permission | Purpose |
| --- | --- | --- | --- |
| GET/PUT | `/fees/policy` | `fees.read` / `fees.approve` | Sibling discount, sibling rule, late fee percent, due day |
| GET/POST/PATCH | `/fees/heads`, `/fees/heads/{id}` | `fees.read` / `fees.approve` | Fee heads |
| GET/PUT | `/fees/structure` | `fees.read` / `fees.approve` | Amounts per class and head for an academic year |
| GET/POST, DELETE | `/fees/students/{id}/concessions`, `/fees/concessions/{id}` | `fees.read` / `fees.approve` | Concessions with reason and approver |
| GET | `/fees/vouchers` | `fees.read` | List: `month`, `class_id`, `section_id`, `status`, `q` |
| POST | `/fees/vouchers/generate` | `fees.manage` | Generate for a month, whole school or a class/section; idempotent |
| GET | `/fees/vouchers/{id}` | `fees.read` | Voucher with lines, receipts and print header |
| POST | `/fees/vouchers/{id}/payments` | `fees.manage` | Record a (partial) payment; issues a receipt |
| POST | `/fees/vouchers/{id}/waive-late-fee`, `/cancel` | `fees.approve` | Waive with reason; cancel an unpaid voucher |
| POST | `/fees/late-fees/run` | `fees.manage` | Run the automatic late fee pass now (`as_of`, never in the future); returns vouchers assessed and the total |
| GET | `/fees/receipts/{id}` | `fees.read` | Receipt for reprint |
| POST | `/fees/receipts/{id}/reverse` | `fees.approve` | Reversal entry with reason |
| GET | `/fees/students/{id}/ledger` | `fees.read` | Totals, vouchers, receipts, concessions |
| GET | `/fees/defaulters` | `fees.read` | `as_of`, `class_id`, `min_days` |
| GET | `/fees/collections` | `fees.read` | Daily summary by method and receipts for a `date` |

Role defaults: accountant has `fees.read` + `fees.manage`; admin and principal also have
`fees.approve`; teachers have no fee permissions.

### 6.4 M1 stage 3 endpoints

| Method | Path | Permission | Purpose |
| --- | --- | --- | --- |
| GET | `/attendance/calendar` | `attendance.read` | Weekend days and holidays (`from`, `to`) |
| PUT | `/attendance/calendar/weekend` | `academics.manage` | Set weekend days (1 = Monday … 7 = Sunday) |
| POST, DELETE | `/attendance/calendar/holidays`, `/attendance/calendar/holidays/{id}` | `academics.manage` | Holidays and closures |
| GET | `/attendance/sheet` | `attendance.read` + section scope | Roster with the day's entries, day kind, `can_edit`, `is_correction` |
| PUT | `/attendance/sheet` | `attendance.manage` + scope; corrections `attendance.correct` | Submit or correct a sheet (exceptions only) |
| GET | `/attendance/sheet/history` | `attendance.read` + scope | Audit trail of a sheet |
| GET | `/attendance/status` | `attendance.read` | Every section's marking state for a date, with totals and percentage |
| GET | `/attendance/absentees` | `attendance.read` | Absent, late and on-leave students for a date (`class_id`), with guardian phone |
| GET | `/attendance/register` | `attendance.read` + scope | Students × days of a month for a section |
| GET | `/attendance/students/{id}` | `attendance.read` + scope | Summary and days for a student (`from`, `to`; default from the year start) |
| GET/PUT | `/attendance/staff` | `teachers.read` / `teachers.manage` | Staff attendance for a date |
| GET | `/attendance/staff/{id}` | `teachers.read` | Staff history |
| GET | `/dashboard` | any user | Today's day kind and, with `attendance.read`, the attendance totals and the sections not yet marked |

### 6.5 M1 stage 4 endpoints

| Method | Path | Permission | Purpose |
| --- | --- | --- | --- |
| GET | `/exams/assessments` | `exams.read` | Assessments of a year (`academic_year_id`) |
| POST | `/exams/assessments`, `/exams/assessments/defaults` | `academics.manage` | Add one, or the four defaults |
| PATCH, DELETE | `/exams/assessments/{id}` | `academics.manage` | Rename or reorder; delete only while it has no sheets |
| PUT | `/exams/assessments/weights` | `academics.manage` | Weights for every assessment of the year, totalling 100 |
| GET/PUT | `/exams/grading` | `exams.read` / `academics.manage` | Grading scheme |
| GET/PUT | `/exams/max-marks` | `exams.read` / `academics.manage` | Maximum marks per subject (`assessment_id`, `class_id`) |
| GET | `/exams/sheets` | `exams.read` + section scope | Subjects of a section in an assessment with sheet state and `can_enter` |
| GET/PUT | `/exams/sheet` | `exams.read` / `exams.manage` + subject scope | Marks grid; save a draft |
| POST | `/exams/sheets/{id}/submit` | `exams.manage` + scope | Submit a complete sheet |
| POST | `/exams/sheets/{id}/approve`, `/reopen` | `exams.approve` | Lock, or send back to draft |
| GET | `/exams/results/section` | `exams.read` + scope | Result sheet with totals, grades, positions, subject analysis, `complete` |
| GET | `/exams/results/section/report-cards` | `exams.read` + scope | Report cards of every student of a section |
| GET | `/exams/results/student/{id}` | `exams.read` + scope | Report card for the active (or given) year |

Role defaults: teachers have `exams.read` and `exams.manage`; principal also `exams.approve`;
admin has `academics.manage` for setup but not `exams.approve` unless granted.

### 6.6 M1 stage 5 endpoints

| Method | Path | Permission | Purpose |
| --- | --- | --- | --- |
| GET | `/backup/status` | any user | Last backup, staleness, schedule, folder, last error |
| GET/PUT | `/backup/settings` | `backup.manage` | Schedule, folder, files to keep |
| POST | `/backup/run` | `backup.manage` | Back up now |
| GET | `/backup/runs`, `/backup/files` | `backup.manage` | Run history; `.mbk` files in the folder |
| POST | `/backup/preview` | `backup.manage` | Header of a file (`path`), no key needed |
| POST | `/backup/restore` | `backup.manage` | Restore a file with `password` or `recovery_key` |
| POST | `/backup/password` | `backup.manage` | Change the backup password (current password or recovery key) |
| POST | `/backup/unlock` | `backup.manage` | Add the public key to a pre-stage-5 key set |

`/dashboard` now carries the backup status as well.

### 6.7 M1 stage 6 endpoints

| Method | Path | Permission | Purpose |
| --- | --- | --- | --- |
| GET | `/imports/fields/{kind}` | kind's permission | Importable fields |
| GET | `/imports/templates/{kind}` | kind's permission | Template (`format=xlsx|csv`) |
| POST | `/imports/parse` | kind's permission | Read an uploaded file (base64): sheets, headings, rows, guessed or saved mapping |
| GET/PUT | `/imports/mappings/{kind}` | kind's permission | Saved column mapping |
| POST | `/imports/preview` | kind's permission | Dry run: row outcomes and summary |
| POST | `/imports/apply` | kind's permission | Import as a batch |
| GET | `/imports/batches` | `students.manage` or `teachers.manage` | Import history |
| POST | `/imports/batches/{id}/rollback` | kind's permission | Undo a batch |
| GET | `/export/students`, `/export/staff`, `/export/fees/vouchers`, `/export/fees/defaulters`, `/export/attendance/absentees`, `/export/attendance/register`, `/export/exams/results` | the list's permission | Download as `.xlsx` or `.csv` (`format`), with the list's filters |

Students and opening balances need `students.manage`; staff needs `teachers.manage`.

### 6.8 M1 stage 7 endpoints

| Method | Path | Permission | Purpose |
| --- | --- | --- | --- |
| GET | `/academic-years/rollover/preview` | `academics.manage` | What the rollover will do |
| POST | `/academic-years/rollover` | `academics.manage` | Roll over (`retain`, `graduate`: student ids) |
| GET | `/dashboard` | any user | Now also students, staff, fees collected this month, defaulters |

### 6.9 International (after M1)

| Method | Path | Permission | Purpose |
| --- | --- | --- | --- |
| GET | `/system/locale` | none | Countries, currencies and time zones to choose from |
| PATCH | `/schools/current` | `settings.manage` | Change name, city, country, currency or time zone; the summary now carries `country`, `time_zone`, `currency_symbol`, `id_label`, `date_order` |

`POST /setup` takes `country` (default `PK`), optional `currency` and `time_zone` (country
defaults), and any `academic_year_start_month` 1–12.

### 6.12 Pre-pilot polish

- **Version.** `madrasa_api::VERSION` is the Cargo version, or `MADRASA_RELEASE` (set at
  compile time by the Release workflow from the tag, `v0.1.3-alpha` → `0.1.3-alpha`). The
  workflow fails if the tag's numeric part differs from `Cargo.toml`; a unit test keeps
  `package.json`, `apps/desktop/package.json` and `tauri.conf.json` equal to Cargo;
  `scripts/set-version.sh X.Y.Z` bumps all four. `/health` and `/system/features` report it
  and the app shows it under Help → About.
- **Users.** `GET /users` (`users.manage`) lists every login with roles, the linked staff
  record (`staff_id`, `staff_name`), `is_active` and `last_login_at`; `POST /users` creates a
  login for a staff member (`staff_id`, `username`, `password`, `role`); `PATCH /users/{id}`
  changes the role or `is_active`; `POST /users/{id}/password` resets the password. These share
  the staff-record login logic and rules (one role, no self-disable, audited). Settings → Users
  is the page.
- **Demo school.** `POST /setup/demo` (`admin_username`, `admin_password`, `backup_password`,
  optional `size: full | small`) loads the demo school (`is_demo = true`, "DEMO School", the
  `demo.*` staff logins with the demo password) in its own transaction, then finishes setup
  like the wizard: the given administrator with principal + admin roles, backup keys,
  settings, setup completed. Refused once setup is done or a real school exists. The wizard's
  first step offers "Load the demo school instead", clearly marked as demo data; every screen
  shows the DEMO banner afterwards.
- **GitHub Actions** run on Node 24: `actions/checkout@v5`, `actions/setup-node@v5`,
  `actions/cache@v5`, `actions/upload-artifact@v5`, `pnpm/action-setup@v5`.

### 6.13 LAN mode (Lite) and marks via Excel

**Mode.** The desktop shell asks on a fresh install how the PC takes part and stores the
answer in `<data_dir>/mode.json` (`mode: standalone | server | client`, `server_url`,
`fingerprint`, `listen_port`). An install from before modes existed (no `mode.json` but a
`pg` database folder) runs as standalone without asking. Tauri commands: `get_mode` (the
current mode, `null` only on a fresh install), `set_mode` (validates, saves; starts the
backend on a fresh install, otherwise stops the running backend cleanly and restarts Madrasa
in the new mode after replying), `api_base_url` (the loopback URL the UI talks to; errors
with `mode_not_chosen` until a mode is saved), `network_info`, `discover_servers`,
`probe_server`. The browser build is always standalone. **Settings → Network**
(`settings.manage`) shows the current mode, a server's addresses, port, fingerprint and
firewall note, or a client's server and fingerprint, and a **Change** section with the same
chooser as the first-start screen; records on the PC are kept whichever mode is chosen.

- **Standalone / server**: the shell runs the bundled database and the API on a random
  loopback port as before. In **server** mode the same router is also served over TLS on
  `0.0.0.0:<listen_port>` (default `8471`) by `axum-server` with a self-signed certificate
  (`rcgen`, ECDSA P-256) created once and stored in `<data_dir>/tls/{cert,key}.der`
  (0600). The SHA-256 fingerprint of the certificate is shown under Settings → Network
  together with the PC's addresses and the port. A UDP beacon on port `47121` answers
  `MADRASA_DISCOVER v1` with `{name, version, port, fingerprint}` (name = school name).
- **Client**: no database. The shell runs a loopback HTTP proxy (`lan::serve_proxy`) that
  forwards every request to `https://<server>:<port>` with a pinned-certificate verifier:
  the connection is accepted only if the server certificate's fingerprint equals the one
  saved at setup, whatever its name or expiry. A mismatch or an unreachable server is
  returned to the UI as `502 {error: {code: "server_unreachable"}}` with a plain message.
  The webview therefore never sees a self-signed certificate and the UI code is unchanged.
- **First connect**: the client broadcasts for servers (and offers manual entry; `host`,
  `host:port`, `https://host:port` are accepted, default port 8471), then `probe_server`
  fetches `/system/features` and `/setup/status` accepting any certificate and returns the
  fingerprint it saw. The user must confirm that it matches the server's screen before
  `set_mode` saves it. Later connections are pinned; the fingerprint only changes if the
  server is reinstalled, in which case the client must be set up again.
- **Security**: logins, sessions and permissions are the existing ones; every client user
  authenticates with their own account and the backend enforces permissions exactly as on
  one PC (`crates/api/tests/lan.rs` checks eight teachers at once through TLS and the proxy,
  with 403s on fees, settings and other teachers' sections). The TLS private key never
  leaves the server. The firewall step (Windows: allow Madrasa on private networks, or the
  `netsh advfirewall` command shown on screen) is documented in the mode page, Settings →
  Network and `TESTING.md`; the installer does not change firewall rules.
- `lan::init()` installs the `ring` crypto provider for rustls once per process.

**Marks via Excel.**

- `GET /exams/sheet/template?assessment_id&section_id&subject_id` (`exams.read`): an
  `.xlsx` named `marks-<class>-<section>-<subject>-<assessment>.xlsx` with the columns
  *Student number, Student name, Roll no, Marks (max N), Absent (Y)*, one row per enrolled
  student, existing marks filled in.
- `POST /exams/sheet/upload` `{assessment_id, section_id, subject_id, file_name,
  content_base64}` (`exams.manage`, the teacher's own sections, sheet not submitted or
  approved): parses `.xlsx/.xlsm/.xls/.csv` with the import parser, matches rows by student
  number, reads marks and the absent flag (`Y`, `yes`, `A`, `absent`, `1`), and validates
  exactly as on-screen entry (0 ≤ marks ≤ max, two decimals, absent clears marks, blank
  marks are left untouched). Any issue returns `{sheet: null, saved: 0, issues: [{row_no,
  message}]}` and nothing is saved; otherwise the marks go through the same `save_sheet`
  as `PUT /exams/sheet` and the result is the draft sheet (`{sheet, saved, issues: []}`).
  Submission and approval are unchanged, so uploaded marks follow the same workflow.

### 6.14 M2 stage 1 endpoints

| Method | Path | Permission | Notes |
| --- | --- | --- | --- |
| GET | `/reports?lang=en\|ur` | `reports.read` | Catalogue by module: code, title, description, orientation, parameters (`name`, `kind`, `label`, `required`) |
| GET | `/reports/{code}?format=json\|pdf\|xlsx&lang=en\|ur&…` | per report (`access`) | Runs a report; parameters by `kind`: `academic_year_id` (default: active year), `class_id`, `section_id`, `status`, `month` (`YYYY-MM`), `from`/`to`/`as_of` (`YYYY-MM-DD`), `assessment_id`, numbers (`top`, `threshold`, `min_days`), `student_number`. Unknown code → 404; bad parameter or format → 400; outside the caller's access or section scope → 403 |

### 6.15 M2 stage 3 endpoints

| Method | Path | Permission | Notes |
| --- | --- | --- | --- |
| GET | `/reports/custom/sources?lang=` | `reports.read` | The field catalogue: sources, typed fields with labels, choices and operators, operator labels, limits |
| GET | `/reports/custom` | `reports.read` | Saved reports visible to the caller (shared + own) with `can_edit` |
| POST | `/reports/custom` | `reports.read` (`reports.manage` to share) | `{name, description, definition, is_shared}`; 400 for anything outside the catalogue; 409 on a duplicate name |
| POST | `/reports/custom/run` | `reports.read` | Preview a definition without saving: `{definition, name?, lang?, academic_year_id?, limit?}` → report document |
| GET | `/reports/custom/{id}?format=json\|pdf\|xlsx&lang=` | `reports.read` | Run a saved report (404 unless shared or own); audit-logged |
| PUT / DELETE | `/reports/custom/{id}` | owner or `reports.manage` | Edit or remove |

### 6.16 M2 stage 4 endpoints

| Method | Path | Permission | Notes |
| --- | --- | --- | --- |
| GET / POST | `/boards` | `exams.read` / `academics.manage` | Boards; `{name, short_code, country?, grading_mode?, pass_percent?, grade_bands}` |
| PUT / DELETE | `/boards/{id}` | `academics.manage` | 409 when removing a board that has exams |
| GET / POST | `/boards/exams?board_id&academic_year_id` | `exams.read` / `academics.manage` | Exams with board and year names, classes, papers, candidate and result counts; `{board_id, academic_year_id?, name, class_ids, subjects:[{name, subject_id?, max_marks}]}` |
| GET / PUT / DELETE | `/boards/exams/{id}` | `exams.read` / `academics.manage` | PUT also takes `status` |
| GET / POST | `/boards/exams/{id}/candidates` | `exams.read` / `academics.manage` | POST registers `{student_ids?, class_id?, section_id?, group_name?}` (active students; already registered are kept) |
| PUT / DELETE | `/boards/candidates/{id}` | `academics.manage` | `{roll_no, registration_no, group_name}`; 409 on a duplicate roll number |
| GET | `/boards/exams/{id}/results` | `exams.read` | The computed sheet: rows per candidate with paper cells, totals, grade, status, position; per-paper and overall pass rates |
| POST | `/boards/exams/{id}/results/parse` | `academics.manage` | `{file_name, content_base64, sheet?}` → headers, rows, guessed or remembered mapping and layout |
| POST | `/boards/exams/{id}/results/preview` | `academics.manage` | `{headers, rows, mapping, layout}` → planned results per row with errors and warnings |
| POST | `/boards/exams/{id}/results/apply` | `academics.manage` | Same body (+ `file_name`, `save_mapping`); 400 while any row has an error; returns `batch_id` and the summary |
| POST | `/imports/batches/{id}/rollback` | `academics.manage` for `board_results` batches | Reverts the import |
| GET | `/students/{id}/board-results` | `exams.read` (teacher scope) | The student's board results across exams |

### 6.17 M2 stage 5 endpoints

| Method | Path | Permission | Notes |
| --- | --- | --- | --- |
| GET / POST | `/exams/terms?academic_year_id` | `exams.read` / `academics.manage` | Terms of a year; `{academic_year_id, name, position, starts_on?, ends_on?, weight?}` |
| PUT / DELETE | `/exams/terms/{id}` | `academics.manage` | Rename, reorder, dates; 409 when removing a term that has components |
| PUT | `/exams/terms/weights` | `academics.manage` | `{academic_year_id, weights:[{id, weight}]}`, totalling 100 |
| GET | `/exams/structure?academic_year_id` | `exams.read` | Terms in order, each with its components in order, plus `unassigned` components |
| POST | `/exams/assessments` | `academics.manage` | Now takes `term_id`, `kind` and `formula`; a calculated component is validated against its term |
| POST | `/exams/assessments/defaults` | `academics.manage` | Takes `terms` (1 or 2) |
| PATCH | `/exams/assessments/{id}` | `academics.manage` | Also moves a component to another term (`term_id`) or changes its `formula` |
| PUT | `/exams/assessments/weights` | `academics.manage` | Weights are validated per term (each term's components total 100) |

Sheet, template, upload and maximum-marks endpoints answer 400 for a calculated component.
`/exams/results/*` and the report cards return `terms` (id, name, weight, component ids,
percent, grade) and `term_percents` per subject; `assessments` entries carry `term_id` and
`kind`.

### 6.18 M2 stage 6 endpoints

| Method | Path | Permission | Notes |
| --- | --- | --- | --- |
| GET / PUT | `/classes/{id}/subjects` | `academics.read` / `academics.manage` | Now returns `elective_ids`; PUT takes `elective_ids` (optional; omitted keeps the flags) |
| GET / POST | `/classes/{id}/subject-groups` | `academics.read` / `academics.manage` | `{name, position?, subject_ids}`; 409 for a non-elective subject or a duplicate name |
| PUT / DELETE | `/subject-groups/{id}` | `academics.manage` | Removing a group clears the label on enrolments that chose it |
| GET / PUT | `/sections/{id}/subject-allocations` | `students.read` / `students.manage` (teacher scope) | Class subjects with elective flags, groups, and every active student with `group_id`, `allocated`, `allocated_ids`, `subject_ids`; PUT `{rows:[{enrollment_id, group_id?, subject_ids?}]}` (a group with no subjects uses the group's) |
| GET / PUT | `/students/{id}/subjects?academic_year_id?` | `students.read` / `students.manage` (teacher scope) | The student's subjects with `taken` flags and the class's groups; PUT `{academic_year_id?, group_id?, subject_ids?}` |
| GET | `/subject-allocations/import/template?section_id&format?` | `students.read` | Excel (or CSV) sheet with the section's students, group and electives |
| POST | `/subject-allocations/import/parse` | `students.manage` | `{file_name, content_base64, sheet?}` → headers, rows, guessed mapping (`student_number`, `group`, `subjects`) |
| POST | `/subject-allocations/import/preview` | `students.manage` | `{headers, rows, mapping}` → planned rows with errors, warnings and `changed` |
| POST | `/subject-allocations/import/apply` | `students.manage` | Same body; 400 while any row has an error; returns `batch_id` |
| POST | `/imports/batches/{id}/rollback` | `students.manage` for `student_subjects` batches | Restores the previous allocations |

`/exams/results/*` rows carry `takes` per subject cell; `/exams/sheets` counts the takers of
each subject as `students`.

### 6.19 M2 stage 7 endpoints

| Method | Path | Permission | Notes |
| --- | --- | --- | --- |
| GET | `/allocation/teachers?academic_year_id?&class_id?` | `teachers.read` | Subjects (union over the shown classes), sections with class teacher and one cell per taught subject (`staff_id`, `staff_name`, `extra` further teachers), and every active staff member's load (`cells`, `class_teacher_of`) across the year |
| PUT | `/allocation/teachers/cell` | `teachers.manage` | `{section_id, subject_id, staff_id?}`; 400 for a subject not taught in the class |
| PATCH | `/sections/{id}` | `academics.manage` | Class teacher (existing endpoint) |
| GET | `/allocation/sections?class_id&academic_year_id?` | `students.manage` | The class's sections with size, gender split, capacity, class teacher and students in roll order |
| POST | `/allocation/balance` | `students.manage` | `{class_id, academic_year_id?, exclude_student_ids}` → proposed moves and sizes before/after |
| POST | `/allocation/moves` | `students.manage` | `{class_id, academic_year_id?, moves:[{student_id, to_section_id}], roll_numbers: keep \| reassign}`; 400 for a section of another class; returns the count and the class view |

### 6.20 M2 stage 8 endpoints

| Method | Path | Permission | Notes |
| --- | --- | --- | --- |
| GET / POST | `/consent-types` | `students.read` / `academics.manage` | `{name, description?, given_by, required, min_level?, max_level?, renewal, position?}`; 400 for a class-range type without levels |
| PUT / DELETE | `/consent-types/{id}` | `academics.manage` | Delete hides the type (soft delete) |
| GET / POST | `/students/{id}/consents?academic_year_id?` | `students.read` / `students.manage` (teacher scope) | Every type with `needed`, `current` and the latest row, the linked guardians, and the full history; POST `{consent_type_id, status, guardian_id?, recorded_on?, note?, academic_year_id?}` |
| GET / POST | `/sections/{id}/consents` | `students.read` / `students.manage` (teacher scope) | The section's students × types with `required`, `current`, date and guardian, and the count of consents to collect; POST `{rows:[{student_id, consent_type_id, status, guardian_id?, note?}], recorded_on?}` records them all |
| GET | `/reports/students.consents_missing`, `students.consents_expiring`, `students.consent_forms` | `reports.read` | Parameters: year, class, section; the missing report also takes `consent_type_id` (`consent_type` parameter kind) |

### 6.21 v0.2.0-alpha test report fixes (v0.2.1)

- **B1** `GET /backup/status` needs `backup.manage` like every other backup route (a teacher
  gets 403).
- **B2/B3** Vouchers and report cards are typeset by the report PDF engine: `GET
  /fees/vouchers/{id}/pdf?lang=`, `GET /fees/vouchers/pdf?ids=a,b,c&lang=` (one voucher per
  page, school and parent copies side by side, every amount prefixed with the school's
  currency symbol) and `GET /exams/report-cards/pdf?section_id=|student_id=&lang=`
  (`exams.read`, same scope as the JSON cards). The Print buttons on the voucher and report
  card pages download these PDFs; the on-screen voucher shows the currency on every amount.
- **B5** The payment-reference block on vouchers follows the school's country:
  `madrasa_core::fees::payment_reference_default` gives "Bank / 1Bill / Kuickpay reference"
  for Pakistan and "Payment reference" elsewhere; Settings → School (`/settings/school`)
  edits `payment_reference_label` and `payment_reference_note` (setting key
  `fees.payment_reference`; an empty heading restores the country default). `VoucherDetail`
  carries `payment_reference {label, note, reference}`.
- **B7** The demo school is generated for the install date (`DemoOptions.today`): the active
  year is the one running now, fees and attendance run to today, and assessments are held
  once their usual date (mid-June, mid-September, mid-October, mid-March) has passed. Every
  screen of a demo school shows the DEMO banner (`SchoolSummary.is_demo`).
- **O1** `Formula.absent` (`zero` or `skip`; default `skip` for best-of and average, `zero`
  for sum and weighted average) says how an absence in an input counts; the setup editor
  offers it.
- **O5** Year rollover creates, for every class, the sections it had last year (same names,
  capacity, and the class teacher unless they moved up with the promoted section), so the
  lowest class has sections for new admissions.
- **Polish** The import wizard shows a required field filled from other columns as "from
  First name + Last name" / "from Guardian 1" (B4); allocation responses carry `warnings`
  naming compulsory subjects that stay allocated (B6); an update import that re-links an
  existing guardian records the link as an update, so rolling the batch back restores it
  instead of removing it (O3); the defaulters list counts only unpaid or partly paid
  vouchers as unpaid months, matching the balance (O2); guardian search by phone is covered
  by a test (O8).

### 6.22 Standard stage 0 endpoints

| Method | Path | Permission | Notes |
| --- | --- | --- | --- |
| GET / PUT | `/settings/edition` | `settings.manage` | `{edition, available, features, standard_only}`; PUT `{edition: lite \| standard}` reloads the flags, syncs the role defaults and audits; 400 for Nexus |

### 6.23 Standard stage 1 endpoints (feature `timetable`)

| Method | Path | Permission | Notes |
| --- | --- | --- | --- |
| GET | `/timetable/setup?academic_year_id=` | `timetable.read` | Shifts, rooms, lessons (with names), rules, sections, teachers, subjects, timetables, `published_id`, the pre-check report, the load summary, the school rules |
| POST | `/timetable/shifts`, `/timetable/shifts/default`; PUT/DELETE `/timetable/shifts/{id}` | `timetable.manage` | Shifts; 409 on a bad shift (plain message) |
| GET/POST | `/timetable/rooms`; PUT/DELETE `/timetable/rooms/{id}` | read / manage | Rooms |
| POST | `/timetable/lessons`, `/timetable/lessons/seed`; PUT/DELETE `/timetable/lessons/{id}` | `timetable.manage` | Lessons; seeding returns `{created}`; deleting removes the lesson's cards |
| PUT | `/timetable/rules` | `timetable.manage` | Upsert `{kind, entity_id, shift_ids, home_room_id, time_off, limits}` for the year |
| POST | `/timetable/generate` | `timetable.manage` | `{timetable_id?, name?, time_limit_seconds, improve_seconds?, seed?}` → 202 `TimetableJob`; 400 with the check findings when the setup has errors; 409 while a run is in progress |
| GET / POST | `/timetable/generate/{id}`, `/timetable/generate/{id}/cancel` | read / manage | Poll a run; stop it (the best timetable so far is kept) |
| POST | `/timetable/timetables`; GET/PATCH/DELETE `/timetable/timetables/{id}`; POST `…/publish` | read / manage | Timetables; GET is the full view with `problems`; a published one cannot be deleted |
| GET | `/timetable/timetables/{id}/options?lesson_id&number`, `…/swap-options` | `timetable.read` | Slot options (`ok` / `soft` / `blocked` with reasons and free rooms); sessions that can swap |
| POST | `/timetable/timetables/{id}/move`, `…/swap`, `…/lock` | `timetable.manage` | Manual edits; 400 with the engine's reason when not possible (locked, clash) |
| GET | `/timetable/current?academic_year_id=`, `/timetable/mine` | `timetable.read` | The published view; the caller's own week (404 without a staff record) with `covers` and `today` |
| GET | `/timetable/print?kind=section\|teacher\|room&id=&lang=&timetable_id?` | `timetable.read` | PDF grid |
| GET | `/timetable/substitutions?date=` | `timetable.read` | Needs with candidates, absences, cover counts, teachers, shifts |
| POST / DELETE | `/timetable/absences`, `/timetable/absences/{id}` | `timetable.manage` | Absences (whole days or periods); deleting removes the covers |
| PUT / DELETE | `/timetable/covers` | `timetable.manage` | Decide (`cover_staff_id` or null) or withdraw a cover; 400 for an unsuitable or unlisted teacher |
| GET | `/timetable/cover-sheet?from&to`, `/timetable/cover-sheet/pdf` | `timetable.read` | The cover sheet (at most two months) |
| GET | `/reports/timetable.section`, `.teacher_load`, `.free_rooms`, `.cover_sheet` | `timetable.read` | Library reports; the module is hidden from the catalogue on Lite |

### 6.24 Standard stage 2 endpoints (feature `leave`)

| Method | Path | Permission | Notes |
| --- | --- | --- | --- |
| GET/POST | `/leave/types`; PUT/DELETE `/leave/types/{id}` | read / manage | Leave types; 409 when a type still has live requests |
| GET/PUT | `/leave/settings` | read / manage | `{approver_staff_id, approver_name}` |
| GET | `/leave/balances?academic_year_id&staff_id` | `leave.read` | Balances with `granted`, `available`, `remaining`; teachers see their own |
| PUT | `/leave/balances` | `leave.manage` | `{staff_id, leave_type_id, entitlement?, carried_over, adjustment, note}` |
| POST | `/leave/balances/carry-over` | `leave.manage` | `{from_academic_year_id, to_academic_year_id}` → `{written}` |
| GET | `/leave/requests?academic_year_id&staff_id&status&from&to` | `leave.read` | `{requests, can_decide, own_staff_id}`; teachers see their own |
| POST | `/leave/requests` | `leave.read` (own) / `leave.manage` (others) | `{staff_id?, leave_type_id, from_date, to_date?, half_day?, reason, attachment_note}`; 409 on overlap, weekend-only range or more than the balance |
| POST | `/leave/requests/{id}/decide`, `…/withdraw` | approve or approver / owner or manage | `{decision: approved \| rejected, note}`; withdrawal unmarks approved days |
| GET | `/leave/students?academic_year_id&student_id&section_id&status&from&to` | `leave.read` | `{requests, decide_sections, can_manage}`; teachers: own sections |
| POST | `/leave/students`; POST `/leave/students/{id}/decide`, `…/withdraw` | manage / section teacher or manage / manage | Student requests; approval marks sheets, withdrawal turns the days into absences |
| GET | `/leave/staff-register?month=YYYY-MM` | `leave.read` | Per staff member: a cell per day (`status`, `arrived_at`, `approved_leave`) and counts |
| PUT | `/attendance/staff` | `teachers.manage` | Entries accept `status: late` with `arrived_at: "HH:MM"` on Standard (400 on Lite); rows written by approved leave are left alone |
| GET | `/reports/leave.balances`, `leave.register`, `leave.students` | `leave.read` | Library reports; the module is hidden from the catalogue on Lite |

### 6.25 Standard stage 3 endpoints (feature `admissions`)

| Method | Path | Permission | Notes |
| --- | --- | --- | --- |
| GET/POST | `/admissions/stages`; PUT/DELETE `/admissions/stages/{id}` | read / manage | Stages (defaults created on first GET); 409 when removing a stage with applications |
| GET/PUT | `/admissions/settings` | read / manage | `{fee_head_id, fee_head_name}` |
| GET | `/admissions/board?academic_year_id&class_id&q` | `admissions.read` | `{academic_year, stages, applications, funnel}` |
| GET | `/admissions/applications?academic_year_id&class_id&stage_id&q&from&to` | `admissions.read` | List with stage, class and student number |
| POST | `/admissions/applications`; GET/PUT/DELETE `/admissions/applications/{id}` | manage / read / manage | The application with its `events`; 409 on a converted one |
| POST | `/admissions/applications/{id}/move` | `admissions.manage` | `{stage_id, note}`; 409 when the rules refuse |
| POST | `/admissions/applications/move` | `admissions.manage` | `{ids, stage_id, note}` → `{moved, refused: [[id, reason]]}` |
| POST | `/admissions/applications/{id}/convert` | manage + `students.manage` | `{section_id, roll_no?, admission_date?, student_number?, force?}` → `{application, student_id, student_number, guardian_id, voucher_id}`; 400 unless admitted; 409 `duplicate` |
| GET | `/admissions/applications/{id}/offer-letter?lang=` | `admissions.read` | PDF |
| * | `/imports/*` with kind `enquiries` | `admissions.manage` | Enquiry import through the import engine (403 `feature_unavailable` on Lite) |
| GET | `/reports/admissions.funnel`, `.by_class`, `.pending_tests` | `admissions.read` | Library reports; hidden on Lite |

### 6.26 Standard stage 4 endpoints (feature `library`)

| Method | Path | Permission | Notes |
| --- | --- | --- | --- |
| GET | `/library/books?q&category&subject` | `library.read` | Books with copy counts (`copies`, `available`, `on_loan`) |
| POST | `/library/books` | `library.manage` | Book plus `accession_nos` and `shelf` for its first copies; 409 on a used accession number |
| GET/PUT/DELETE | `/library/books/{id}` | read / manage | `BookDetail` = book, `copy_list`, open `loans`; delete refused while a copy is out |
| POST | `/library/books/{id}/copies` | `library.manage` | `{accession_nos, shelf}` |
| GET | `/library/copies?status` | `library.read` | Every copy with its title |
| GET | `/library/copies/lookup?accession_no` | `library.read` | The copy and its open loan; 404 when unknown |
| PUT | `/library/copies/{id}` | `library.manage` | `{shelf, condition, status}`; status cannot change on a copy on loan |
| GET | `/library/members?kind&q` | `library.read` | Students and staff who can borrow |
| GET | `/library/members/{kind}/{id}` | `library.read` | `{member, loans, open, overdue, fines_due, rules}` |
| GET | `/library/loans?state&member_kind&member_id&from&to` | `library.read` | `state`: open, overdue, returned, fines |
| POST | `/library/loans` | `library.manage` | `{copy_id or accession_no, member_kind, member_id, issued_on?}` → the member page; 409 when the rules refuse |
| POST | `/library/loans/{id}/renew` | `library.manage` | 409 past the renewal limit or when overdue |
| POST | `/library/loans/{id}/return` | `library.manage` | `{returned_on?, lost?, lost_charge?}` → the loan with its fine |
| POST | `/library/loans/{id}/waive` | `library.manage` | `{amount, reason}`; capped at the fine |
| POST | `/library/loans/{id}/settle` | `library.manage` | → `{loan, voucher_id}`; a voucher for students when the fee head is set |
| GET/PUT | `/library/settings` | read / manage | `{rules: {student, staff}, fine_fee_head_id, fine_fee_head_name}` |
| GET | `/library/stock` | `library.read` | Counts by category |
| GET | `/reports/library.overdue`, `.fines`, `.most_borrowed`, `.stock` | `library.read` | Library reports; hidden on Lite |

### 6.27 Standard stage 5 endpoints (feature `transport`)

| Method | Path | Permission | Notes |
| --- | --- | --- | --- |
| GET/POST | `/transport/vehicles`; PUT/DELETE `/transport/vehicles/{id}` | read / manage | 409 on a duplicate registration or deleting a vehicle a route uses |
| GET/POST | `/transport/routes` | read / manage | `RouteView` with vehicle, stop and student counts; POST takes the route with its `stops` |
| GET/PUT/DELETE | `/transport/routes/{id}` | read / manage | `RouteDetail` = route + `stop_list`; PUT replaces the stops in order; 409 dropping a stop with students or deleting a route with students |
| GET | `/transport/routes/{id}/roster?academic_year_id` | `transport.read` | `{...route, academic_year_id, by_stop: [{stop, students}], total, seats_left, warning}` |
| GET | `/transport/students?academic_year_id&class_id&section_id&q&without` | `transport.read` | Active students of the year with their open assignment; `without=true` keeps the ones without |
| GET | `/transport/assignments?academic_year_id&route_id&stop_id&section_id&student_id&open` | `transport.read` | Assignment history with names and the charge |
| POST | `/transport/assignments` | `transport.manage` | `{student_id, academic_year_id, stop_id, from_date, amount?, notes?}` → `{assignment, route_students, seats_left, warning}`; ends the previous open assignment |
| POST | `/transport/assignments/bulk` | `transport.manage` | `{student_ids, academic_year_id, stop_id, from_date, amount?}` → `{assigned, refused: [[id, reason]], route_students, seats_left, warning}` |
| PUT/DELETE | `/transport/assignments/{id}` | `transport.manage` | `{stop_id?, to_date?, amount?, notes?}` (null `to_date` reopens) |
| GET/PUT | `/transport/settings` | read / manage | `{fee_head_id, fee_head_name}` |
| GET | `/reports/transport.manifest`, `.without_stop`, `.fee_by_route` | `transport.read` | Library reports; hidden on Lite |

### 6.11 Second alpha test report fixes

- `POST /setup` takes `first_year` (create and activate the academic year that is running now,
  from the chosen start month: this year once the month has passed, else last year) and
  `default_classes` (the country's preset, KG–12 or K–12, each with a section A). The wizard
  always sends `first_year` and offers the classes as a ticked checkbox, so an import works
  straight after setup. The "New academic year" dialog defaults to the school's start month
  and that year.
- `GET /staff` and `GET /staff/{id}` blank `phone`, `email` and `cnic` for callers without
  `teachers.manage`.
- Import fields: students `status` (active / left / graduated, with common spellings), `city`,
  `state`, `postal_code` (kept in `custom`); staff `staff_type`, `department` ("Subject /
  department"), `grades` (kept in `custom`). `ImportPreview.unmapped` lists file columns mapped
  to nothing so the wizard can show them before importing.
- The UI hides sidebar modules the role cannot use (`NavItem.permission`) and shows "You don't
  have access" in place of a guarded page; all fee amounts carry the school's currency symbol;
  rollback asks for confirmation; an import in progress survives visiting other pages; linked
  guardians can be edited from the student form; the desktop shell refuses to start as root
  with a plain message.

### 6.10 Alpha test report fixes

| Method | Path | Permission | Purpose |
| --- | --- | --- | --- |
| POST | `/staff/{id}/login` | `users.manage` | Create a login for a staff member: `username`, `password`, `role` (principal, admin, accountant, teacher). The account takes the staff member's name and email |
| PATCH | `/staff/{id}/login` | `users.manage` | `role` (replaces the user's roles) and/or `is_active`; nobody can disable their own account |
| POST | `/staff/{id}/login/password` | `users.manage` | Set a new password; clears lockouts; audited as `password_reset` (hashes never reach the log) |

`StaffView` carries `username`, `roles` and `login_active`. `POST /imports/parse` takes an
optional `sheet`; without one the sheet named after the import kind is read ("Students",
"Staff", "Opening balances"), else the first sheet that is not a read-me or instructions
sheet. The response lists `sheets` and the `sheet` read, and the wizard asks the user to
confirm when there are several.

## 7. Security

- Passwords: Argon2id (`argon2` crate, OWASP-recommended parameters: m=19 MiB, t=2, p=1),
  PHC string stored in `users.password_hash`.
- Lockout: after 5 consecutive failed logins the account is locked for 15 minutes
  (`users.locked_until`); successful login resets the counter. Lockouts are audit-logged.
- Backup encryption: at setup a random 256-bit data key (DEK) is generated. It is wrapped
  twice with AES-256-GCM: once with a key derived (Argon2id) from the backup password, once
  with a key derived from a random recovery key. The recovery key is shown once, printable, as
  8 groups of 4 Crockford-base32 characters. Only wrapped keys and salts are stored (in
  `settings`); the DEK and the passwords are never stored.
- Backup files: see §5.8. The X25519 key pair for unattended backups is derived from the DEK;
  only the public key is stored in clear.
- All crypto via audited crates: `argon2`, `aes-gcm`, `sha2`, `rand`, `x25519-dalek`, `hkdf`.
  No hand-rolled crypto.
- No real data in the repository. Tests and screenshots use the demo generator.
- LAN mode (§6.13): TLS 1.2/1.3 via rustls (`ring`), self-signed server certificate pinned by
  SHA-256 fingerprint on each client after a one-time visual check; private key on the
  server only, mode 0600; clients hold no database and reach the server only through the
  pinned proxy. Discovery answers carry the fingerprint but it is never trusted from the
  network: the user must confirm it against the server's screen.

## 8. UI

### 8.1 Theme (v1)

Light, compact, information-dense. No hero banners. Tokens are CSS variables (light and dark)
exposed through Tailwind:

| Token | Light |
| --- | --- |
| `--bg` | `#F7F9FC` |
| `--card` | `#FFFFFF` |
| `--navy` | `#142F50` |
| `--blue` | `#2864C5` |
| `--selection` | `#E8F1FF` |
| `--success` / `--success-bg` | `#19835A` / `#DDF3E8` |
| `--warning` / `--warning-bg` | `#D99A27` / `#FFF0D5` |
| `--lavender` | `#E8E1FA` |
| `--text` | `#172B4D` |
| `--muted` | `#64748B` |
| `--border` | `#DFE7F1` |

Font: Inter (Latin), Noto Nastaliq Urdu (Urdu), both bundled. Thin borders, subtle shadows,
restrained radii. Dark mode via the same tokens (follow OS, user override). Printouts are
always light.

### 8.2 Layout

- Left sidebar (light blue-grey) listing modules; collapsible.
- Top bar 56–64 px: global search (Ctrl+K), campus selector (Standard+), notifications, help,
  user menu (profile, language, theme, logout).
- Main workspace with breadcrumbs.
- Must be usable at 1366×768 and 1920×1080. Visible focus rings; everything reachable by
  keyboard.

### 8.3 Internationalisation

All strings go through `t()` from the i18n layer (`apps/desktop/src/i18n`). Languages:
English (`en`, LTR) and Urdu (`ur`, RTL). Switching language sets `<html lang dir>` and the
layout uses logical CSS properties so it mirrors correctly. Numbers and dates are formatted
with `Intl` for the active locale.

## 9. Setup wizard (first run)

Shown when `/setup/status` reports `needs_setup`. Steps:

1. School: name, code (short, unique, uppercase), city.
2. Academic year start month (April or August), currency (PKR, fixed in M0).
3. Admin account: full name, username, password (min 10 chars).
4. Backup password and recovery key: user sets a backup password; the app generates and shows
   a printable recovery key which must be acknowledged before finishing.

Completing the wizard creates the school, settings, default roles with permissions, the admin
user (roles `principal` and `admin`) and the wrapped backup keys, all in one transaction, and
audit-logs the creation.

### 9.1 Schools outside Pakistan

Pakistan is the first market, but nothing is hard-coded to it. The school row carries
`country` (ISO 3166-1 alpha-2, default `PK`), `currency` (ISO 4217) and `time_zone` (IANA
name), chosen in the setup wizard and editable under Settings → School profile
(`PATCH /schools/current`, `settings.manage`). `GET /system/locale` (public) lists the
countries, currencies and time zones to choose from; the tables live in
`madrasa_core::locale` and a new country is one row there.

What the country decides, each changeable afterwards:

- **Currency** default (`PKR` for Pakistan, `USD` for the US…). Only currencies with two
  minor units are offered because money is `NUMERIC(14,2)`. Printed vouchers and receipts
  show the school's currency code.
- **Time zone** default (`Asia/Karachi`, `America/New_York`…). `madrasa_api::clock::today()`
  uses it for every "today" (dashboard, defaulters, attendance limits, the late fee pass).
- **Weekend**: setup writes the country's usual school weekend into
  `attendance.weekend_days` (Sunday for Pakistan; Saturday and Sunday for the US, UK, India…;
  Friday and Saturday for the Gulf and Bangladesh; Saturday for Nepal; Friday for
  Afghanistan). Attendance → Calendar edits it as before.
- **Identity numbers**: Pakistan keeps the 13-digit CNIC / B-Form rule; elsewhere the field
  is labelled with the country's name for it (`National ID`, `Emirates ID`, `Aadhaar`…) and
  accepts 3–40 characters. The field is optional everywhere.
- **Phones**: Pakistan keeps the 11-digit rule; elsewhere 7–15 digits with an optional `+`.
- **Dates in imports**: numeric dates are read month-first for the US and the Philippines.
- **Academic year start** may be any month (1–12); the wizard suggests August for the US
  and Canada, April for Pakistan.

Grade names are free text; the class preset is `KG`, `1`–`12` or, for the US and Canada,
`K`, `1`–`12`. The import treats `K`, `KG`, `Kindergarten` and `Grade N` / `N` as the same
class keys, so a US register with a "Grade" column and a "Homeroom" column maps onto classes
named `K`–`12` with sections. Western registers are also accepted with separate first and
last name columns (joined into the full name) and a single "Parent/Guardian Name" column
(which stands in for the father's name, which the data model keeps as the primary contact).

## 10. Demo data

`crates/demo-data` builds a fake school ("DEMO School") with ~1,200 students, ~120 staff and
classes K–12 (sections A/B/C) from a fixed seed, so output is reproducible. Everything is
visibly marked DEMO (school name, `is_demo = true`, staff usernames prefixed `demo.`).

It runs only when explicitly invoked (`madrasa-demo load --yes`) and refuses to run when the
database already contains a non-demo school.

## 11. Testing

| Area | How |
| --- | --- |
| Domain logic (core) | Rust unit tests, no I/O |
| Migrations and repositories (db) | Integration tests against PostgreSQL (skipped when `MADRASA_TEST_DATABASE_URL` is unset); each test gets its own schema |
| API | Axum router tests with `tower::ServiceExt`, same DB rule |
| UI helpers and components | Vitest + Testing Library, including RTL rendering |
| CI | GitHub Actions on Linux and Windows: lint, format, type-check, tests, Tauri build |

## 12. Milestones

- **M0 Foundation** (`docs/TASKS-M0.md`): installs, setup wizard, login, themed empty
  dashboard, CI builds.
- **M1 Lite core** (`docs/TASKS-M1.md`): students and guardians, fees, attendance, exams and
  report cards, encrypted backup, Excel import, year rollover; then the alpha fixes, LAN mode
  and marks via Excel.
- **M2 Lite complete** (`docs/TASKS-M2.md`, §14): report library, custom report builder,
  Board Creator with external result import, terms and assessment components, subject
  allocation per student, allocation screens, consent manager.
- Then **Standard** (server and clients) and **Nexus**.

## 13. Open questions

Tracked in `docs/BLOCKERS.md`. Decisions are recorded as ADRs in `docs/adr/`.

## 14. M2 — Lite complete (overview)

The stages, their order and their acceptance are in `docs/TASKS-M2.md`; each stage adds its
data model and endpoints to this document when it is built. Shared decisions:

- **Reports** are described once as a `Report` document (title, parameters, columns, rows,
  groups, totals) and rendered three ways: JSON for the screen, Excel through the existing
  export module, and PDF typeset on the server with embedded Inter and Noto Nastaliq Urdu
  fonts, so Urdu reports are shaped and right-to-left without a browser print dialog. Every
  report exists in English and Urdu from one backend string table. The library is
  `GET /reports` and `GET /reports/{code}` (`reports.read`); teachers run exam and attendance
  reports for their own sections only. Every run is audit-logged with its parameters.
- **Custom reports** are built only from a fixed field catalogue per data source; the user's
  choices select catalogue entries and parameters, never SQL text. Saved definitions are
  rows in `report_definitions` (JSONB) and run like built-in reports.
- **Boards** model external examinations: `exam_boards` (the board and its rules),
  `board_exams` (one sitting, its classes and subjects), `board_candidates` (students with
  roll and registration numbers) and `board_results` (per candidate and subject). Results
  are imported through the import wizard engine as a batch with rollback, matched by roll
  number. The student's own `board_roll_no` and `board_registration_no` remain the defaults.
- **Terms and components** keep `assessment_types` as the entered components (mark sheets
  stay attached to them) and add `terms` plus calculated components with a formula (sum,
  average, weighted average, best of N) evaluated in `madrasa_core::exams`. A migration wraps
  every existing year in one term so results do not change.
- **Subject allocation** adds `student_subjects` per enrolment with subject groups per
  class; absent any allocation a student takes every class subject, as today.
- **Allocation screens** and the **consent manager** are UI and small tables over existing
  entities (`section_teachers`, `enrollments`, new `consent_types` and `consents`), with
  every change audit-logged.

