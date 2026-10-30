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

Identifiers are normalised before storage and matching: CNIC/B-Form to 13 digits
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
7. `late_fee` = `net_monthly` × late percent, assessed once the due date has passed, unless waived. Waiving needs `fees.approve`, a reason, and is audit-logged with the approver.

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
- **M1 Students & Attendance**, **M2 Fees & Finance**, **M3 Examinations & Reports**,
  **M4 Backup/Restore & Packaging polish**, then Standard.

## 13. Open questions

Tracked in `docs/BLOCKERS.md`. Decisions are recorded as ADRs in `docs/adr/`.
