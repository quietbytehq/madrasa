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

