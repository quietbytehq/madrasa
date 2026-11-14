# Progress log

Running log of what was done, what is tested and what is left. Newest entries at the bottom
of each milestone section. See `docs/TASKS-M0.md` for the checklist.

## M0 — Foundation

### 2026-10-30 — Task 1: Repo
- Done: README (editions, status pre-alpha), CONTRIBUTING stub, `.gitignore`, `.editorconfig`,
  Cargo workspace (`crates/core`, `crates/db`, `crates/api`, `crates/demo-data`,
  `apps/desktop/src-tauri`), pnpm workspace (`apps/*`), `docs/spec.md` v0.1, ADR folder.
- Tested: `cargo check --workspace` passes with stub crates; pnpm workspace resolves.
- Left: everything else in M0.

### 2026-10-30 — Task 2: App skeleton
- Done: `apps/desktop` Tauri 2 shell (`src-tauri`, icons generated from `assets`-free source
  via `tauri icon`), React 19 + TypeScript (strict) + Vite 7 + Tailwind 3.4. Scripts: `lint`
  (eslint 9 flat config, react-hooks, typescript-eslint), `format`/`format:check` (prettier +
  tailwind plugin), `typecheck` (`tsc -b`), `test` (vitest + Testing Library, jsdom). Rust:
  `rustfmt.toml`, clippy with `-D warnings` clean.
- Tested: `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm test`, `pnpm build`,
  `cargo build -p madrasa-desktop`, `pnpm tauri build --debug --no-bundle`; the debug binary
  launches under Xvfb and runs until killed (only harmless libEGL warnings).
- Left: in-process API start-up in the shell (task 5), bundling targets in CI (task 10).

### 2026-10-30 — Task 3: Design system
- Done: tokens in `src/styles/tokens.css` (light, dark, print-forces-light) exposed through
  `tailwind.config.ts`; `ThemeProvider` (system / light / dark, persisted). Inter and Noto
  Nastaliq Urdu bundled via `@fontsource` (woff2 shipped in `dist`, no network). Components in
  `src/components/ui`: Button, Input, Select, Table (sort, filter, paginate), Card, Badge,
  Dialog (focus trap, Escape, restore focus), Toast (provider + `useToast`, aria-live),
  EmptyState, Spinner, Breadcrumbs. i18n layer `src/i18n` with `en` and `ur` dictionaries,
  `t()` with placeholders, `Intl` number/date formatting, `<html lang dir>` switching.
- Tested: 28 vitest tests: dictionary parity (every key, same placeholders), provider
  behaviour, every component, and an RTL suite that renders all components under Urdu and
  asserts `dir="rtl"`, translated chrome and no physical left/right utilities.
- Left: nothing for M0.

### 2026-10-30 — Task 4: App shell
- Done: `AppShell` (skip link, collapsible sidebar, top bar, main region), `Sidebar` driven by
  `src/app/navigation.ts` and edition flags (`FeaturesProvider` reads `/system/features`,
  Lite defaults until the API answers), `TopBar` with Ctrl+K search stub, notifications stub,
  help dialog, user menu (language, theme, logout). `SessionProvider` (token in localStorage,
  `/auth/me` on start), `RequireAuth`, `SetupGate` (routes to `/setup` when the API says
  setup is needed), `Dashboard` with four placeholder cards and an empty activity list,
  `ComingSoon` page for every unfinished module, `Login` page. Typed API client in `src/api`.
- Tested: 8 shell tests (edition-based visibility for Lite vs Nexus, dashboard placeholders,
  Coming soon navigation, Ctrl+K, language switch + logout, collapse + keyboard order,
  anonymous redirect). Playwright screenshots of the built UI at 1366×768 and 1920×1080 in
  en/ur × light/dark: no console errors, no horizontal scroll, RTL mirrored correctly.
- Left: real setup wizard page (task 8); login against the real API (task 6).

### 2026-10-30 — Tasks 5–7: Backend, database, auth, edition flags (one commit; the crates
depend on each other and were built together)
- Done, `crates/core` (pure): `edition` (Edition/Feature flags, soft limits), `permissions`
  (20 permission codes, built-in roles with default grants, `PermissionSet`), `hlc` (hybrid
  logical clock stamps), `ids` (UUID v7), `password` (Argon2id, lockout policy 5 tries/15 min),
  `session_token` (random token, SHA-256 stored), `backup` (DEK wrapped by Argon2id-derived keys
  from the backup password and a Crockford-base32 recovery key, AES-256-GCM).
- Done, `crates/db`: migration `0001_foundation.sql` (schools, users, roles, permissions,
  user_roles, sessions, settings, audit_log, custom_field_definitions; UUID v7 keys,
  `school_id`, sync columns, soft delete, JSONB custom, append-only triggers on audit_log),
  repositories that write the audit log in the same transaction, `testing::TestDb` (throwaway
  database per test, skipped when `MADRASA_TEST_DATABASE_URL` is unset).
- Done, `crates/api`: Axum router under `/api/v1`: `/health`, `/system/features`,
  `/setup/status`, `/setup`, `/auth/login|logout|me`, `/users` (admin), `/schools/current`,
  `/audit`, `/nexus/status` (feature-gated stub). `CurrentUser` extractor loads the session and
  effective permissions; handlers call `require(permission)`; `AppState::require_feature`.
  Bootstrap: external `MADRASA_DATABASE_URL` or bundled PostgreSQL (feature
  `embedded-postgres`, ADR 0001). `madrasa-server` binary for Standard/dev.
- Done, desktop shell: starts database + API on a background runtime, exposes `api_base_url`
  command, stops both when the window closes.
- Tested: core 27 unit tests; db 9 integration tests (schema rules for every table, append-only
  audit log, create/update/delete audited with old/new values and no secrets, unique school
  codes, settings upsert + redaction, default roles, sessions, lockout counters); api 6
  end-to-end tests (health, setup once + validation + key material, login/me/logout, lockout
  after 5 failures incl. audit, teacher 403 on `/users` and `/audit` while admin 200, inactive
  account refused and existing session dies, edition flags: `/nexus/status` 403
  `feature_unavailable` in Lite, 403 `forbidden` without `nexus.admin` after switching to
  Nexus, 200 once granted). `madrasa-server` built with `embedded-postgres` started bundled
  PostgreSQL 16.15.1 from scratch, migrated, answered `/health`, completed `/setup` and
  `/auth/login` (run as a non-root user; PostgreSQL refuses to run as root).
- Notes: the bundled archive is downloaded by the `postgresql_embedded` build script from
  GitHub releases (cached under `~/.theseus/postgresql`); this sandbox's proxy blocks the GitHub
  API, so the archive was pre-seeded into the cache by direct download. CI has normal access.
- Left: Windows run of the bundled database (CI build only, not executed here).

### 2026-10-30 — Task 8: First-run setup wizard
- Done: `POST /setup` (one transaction: school, default roles + grants, admin user with
  principal + admin roles, settings incl. academic year month, currency PKR, wrapped backup
  keys, setup marker; returns the recovery key once). UI wizard `src/pages/Setup.tsx`: steps
  School → Academic year → Administrator → Backup security (+ summary) → Recovery key (print
  button with print-only stylesheet, acknowledgement checkbox, go to login). `SetupGate` routes
  to `/setup` while setup is needed.
- Tested: api `setup_runs_once_and_validates_input`; vitest: step validation, request mapping,
  full wizard walk-through with mocked API (validation messages, recovery key shown, ack gate,
  navigation to login), server-side error surfaced.
- Left: nothing for M0.

### 2026-10-30 — Task 9: Demo data
- Done: `crates/demo-data`: deterministic generator (seeded) for "DEMO Gulshan Model School"
  (code `DEMO01`, `is_demo = true`): 1,200 students across KG–12 × sections A/B/C (39
  classes, 30–31 each, plausible ages, unique `DEMO-nnnnn` admission numbers), 120 staff
  (1 principal, 2 admins, 2 accountants, 115 teachers with subjects, usernames `demo.*`).
  Loader inserts the school, default roles and staff accounts (shared documented password)
  in one transaction and marks setup complete; students and classes are written as JSON
  until their tables arrive in M1. CLI `madrasa-demo generate --out <dir>` /
  `madrasa-demo load --yes [--out <dir>]`.
- Safety: the loader errors without `confirmed`/`--yes`, when a non-demo school exists, and
  when demo data is already present. Nothing calls it from the app.
- Tested: 3 generator unit tests (determinism, sizes/classes/roles, DEMO marking and
  uniqueness), 2 loader integration tests (loads into an empty DB and verifies accounts,
  refuses with a real school present and inserts nothing). CLI run manually against a
  scratch database: refused without `--yes`, loaded 120 accounts with it.
- Left: persisting students/classes once M1 adds their tables.

### 2026-10-30 — Task 10: CI and packaging
- Done: `.github/workflows/ci.yml` (frontend lint/format/types/tests/build; Rust fmt, clippy
  incl. the `embedded-postgres` feature, tests with a PostgreSQL 16 service on Linux and
  skipped DB tests on Windows; desktop bundles `deb,appimage` on Linux and `nsis` on Windows
  uploaded as artifacts) and `release.yml` (tag `v*` → draft pre-release with unsigned
  installers via `tauri-apps/tauri-action`). Bundled-PostgreSQL archive cached in CI.
- Tested locally (Linux): `pnpm tauri build --bundles deb,appimage` produced
  `Madrasa_0.1.0_amd64.deb` (45 MiB, depends on libwebkit2gtk-4.1-0 and libgtk-3-0) and
  `Madrasa_0.1.0_amd64.AppImage` (120 MiB); the `.deb` installs with `dpkg -i` (binary,
  desktop entry, icons). Full flow driven with xdotool under Xvfb on a fresh data directory:
  bundled PostgreSQL starts → setup wizard (5 steps) → recovery key page → login → themed
  dashboard. Screenshots reviewed at 1366×768.
- Verified in CI after the push: all jobs green on Linux and Windows, NSIS installer
  artifact produced. The release workflow (tag `v*`) has not been exercised yet.

### M0 status
- Linux: "fresh install → setup wizard → login → themed dashboard" works in the packaged app;
  all tests pass (core 27, db 9, api 6, demo-data 5, vitest 45); fmt, clippy, eslint,
  prettier and tsc are clean.
- Windows: CI run 41022747057 passed: fmt, clippy, unit tests, the bundled-PostgreSQL server
  build and the NSIS desktop installer all succeed on windows-latest. Together with the Linux
  verification this meets M0's "both Linux and Windows CI builds" condition. (The installed
  Windows app has not been exercised by hand; that is the first thing to do on a Windows PC.)
- Fixes found during verification: the setup gate used to redirect to login before the
  recovery key could be read (fixed and tested); the embedded database start now stops a
  stale or orphaned server first; `madrasa-server` handles SIGTERM so the bundled database
  shuts down cleanly.

## M1 — Lite core

### 2026-10-30 — Stage 1: Academic structure, students and guardians (branch `m1-stage-1-academic-students`)
- Done, `crates/core`: `students` module (student number format `<prefix>-<year>-<5 digits>`,
  CNIC/B-Form and phone normalisation, duplicate rules, status/relationship enums); new
  permissions `academics.read` / `academics.manage` with role defaults (teachers also get
  `teachers.read`).
- Done, `crates/db`: migration `0002_academics_students.sql` (academic_years, classes,
  subjects, class_subjects, staff, sections, section_teachers, students, enrollments,
  guardians, student_guardians, school_sequences; all with UUID v7 keys, `school_id`, sync
  columns, soft delete; `custom` JSONB on students, guardians, staff; grants the new
  permissions to existing system roles). Repositories `academics`, `staff`, `students`
  (numbering under a row lock, directory query with filters/sort/paging/teacher scope,
  duplicate finder, photo stored in the DB and never selected with the row), `guardians`
  (shared records, sibling query, search by name or digit prefix). Every mutation audited.
- Done, `crates/api`: `/academic-years`, `/classes` (+ `/subjects`), `/sections`,
  `/subjects`, `/staff` (+ `/assignments`), `/students` (list, create with guardians and
  enrollment, `check-duplicates`, get, patch, `withdraw`, `enrol`, `photo`, guardian
  link/unlink), `/guardians`, `/settings/school` (student number prefix). Teacher section
  scope enforced in `CurrentUser::student_scope`: a teacher sees only sections they are class
  teacher of or teach in, and gets 403 on other students. School codes may now contain hyphens
  (so `KHI-MC` works as the number prefix).
- Done, UI: Students directory (server-side search/filter/sort/paging, 50 per page), add/edit
  form (inline validation, live duplicate warning, 409 dialog with "Save anyway", guardians
  sub-form that links existing records by phone/CNIC), profile page (tabs Details, Guardians,
  Fees/Attendance/Results "Coming soon"; enrollment history, siblings, photo upload/remove,
  withdraw/re-admit dialog), Teachers & staff page (list, edit dialog with assignments),
  Settings page (student number prefix + academic structure: years, classes with subjects,
  sections with class teacher, subjects). All strings in English and Urdu.
- Done, demo data: generator adds guardians shared by siblings (~900 for 1,200 students),
  subjects per grade, B-Forms, addresses, admission dates, staff phones/CNICs; loader writes
  the academic year, 13 classes, 39 sections with class teachers, subject assignments, 1,200
  students with enrollments and guardian links (≈18 s locally).
- Tested: core 32 unit tests; db 16 integration tests (7 new: years, classes/sections/
  subjects/assignments, numbering, duplicates/withdraw/photo/audit, guardians/siblings/
  directory, staff logins, migration grants); api 11 end-to-end tests (5 new incl.
  `teachers_read_only_their_own_sections`); demo-data 6; vitest 54 (9 new for directory,
  form, duplicate dialog, profile, withdraw). Screenshots of the new pages in en/ur at
  1366×768 with a mocked API: no console errors, no horizontal scroll.
- Not done / notes: photos are stored in PostgreSQL (≤ 512 KB) rather than on disk so backups
  stay self-contained; the Fees/Attendance/Results tabs are placeholders by design; the demo
  loader links each student to one guardian (father) only.

### 2026-10-30 — Stage 2: Fees (branch `m1-stage-2-fees`)
- Branch note: at the time of writing GitHub still showed the Stage 1 pull request open, so this
  branch was started from the Stage 1 tip (`ae1258f`) rather than `main`; once Stage 1 merges
  the pull request diff reduces to the fees commits.
- Done, `crates/core`: `fees` module with exact-decimal arithmetic (`rust_decimal`, 2 places,
  half away from zero): fee policy (sibling percent + rule, late percent, due day),
  `compute_voucher` (gross → sibling discount → capped concessions → net monthly → arrears →
  total due), `late_fee` after the due day unless waived, voucher status derivation incl.
  `carried`, ledger totals, carry-forward and month helpers, payment methods. New permission
  `fees.approve` (admin, principal). 12 unit tests covering rounding, both sibling rules,
  concession caps, the 15th/16th boundary, waiver, partial and reversed payments, carry-forward
  across months and academic years.
- Done, `crates/db`: migration `0003_fees.sql` (`fee_heads`, `fee_structures`,
  `fee_concessions`, `fee_vouchers`, `fee_voucher_lines`, `fee_receipts`; receipts are made
  immutable by a trigger, reversals are negative rows with a reason). Repository `fees`:
  policy, heads, structure, concessions, voucher generation under the school sequence lock
  (`V202605-00001`, one-time heads on the first voucher only, idempotent per student and
  month), arrears assessment that marks earlier vouchers `carried`, late-fee assessment,
  waivers, cancellation, payments with over-payment and closed-voucher checks, reversals,
  ledger, defaulters, daily collection summary. Every money change is audit-logged.
- Done, `crates/api`: `/fees/*` routes (see spec §6.3); permissions enforced server-side.
- Done, UI: Fees page with tabs Vouchers (filters, totals footer, generate dialog), Collect
  (lookup and pay), Defaulters, Collections (daily summary by method), Setup (policy, heads
  with one-click defaults, structure grid per class). Student profile Fees tab shows the
  ledger with pay / waive / cancel / reverse / concession actions gated by permission.
  Printable voucher (school + parent copies, reference box) and receipt pages; print CSS keeps
  the light theme; "PDF" is the print dialog's Save as PDF. English and Urdu strings.
- Done, demo data: Tuition/Exam/Admission heads, structure per grade, April and May vouchers
  with ~55 % paid on time, ~15 % paid late (incl. Kuickpay and half payments): 2,400 vouchers,
  851 receipts (≈28 s locally).
- Tested: core 44 unit tests (12 new); db 20 integration tests (4 new: policy/structure/
  generation/sibling rules, payments/reversals/immutability, late fee/waiver/carry-forward
  chain, defaulters/collections); api 13 (2 new incl. accountant-cannot-approve and
  teacher 403); demo-data 6; vitest 69 (15 new: money helpers, ledger permissions and
  actions, pay dialog validation and late warning, voucher list totals and generation, policy
  editing). fmt, clippy, eslint, prettier and tsc clean.
- Review follow-up: the base for percentage concessions is now a policy option,
  `concession_base` = "On gross fee" (default, the original behaviour) or "After sibling
  discount" (stacked). Setup tab shows a short explanation and a worked example that follows
  the entered sibling percent, in English and Urdu. Changes are audit-logged with old and new
  values (asserted in a db test). Tests: core 2 new (both modes for a student with a sibling
  discount and a concession, fixed amounts unaffected, 100 % cap, old stored policies still
  read as gross), db 1 new (generation in both modes, April voucher untouched after the
  switch), api assertions for default/round-trip/invalid value, vitest 2 new (worked example
  and save, Urdu rendering).
- Not done / notes: the `collections` summary covers one day at a time (no date range yet);
  voucher PDF files are produced through the print dialog rather than generated server-side;
  bank/1Bill/Kuickpay integrations are manual reference entry only, as specified.

### 2026-10-30 — Stage 3: Attendance (branch `m1-stage-3-attendance`)
- Branch note: GitHub still showed pull requests #1–#3 open and `main` at the initial commit
  when this stage started, so the branch was cut from the Stage 2 tip (`975a8c0`). Once those
  merge, the pull request diff reduces to the attendance commits.
- Done, `crates/core`: `attendance` module (statuses with present as the default, staff
  statuses, summaries with the attendance percentage, holiday spans, weekend days, day kind,
  month and school-day helpers, weekend validation); new permission `attendance.correct`
  (admin, principal). 5 unit tests.
- Done, `crates/db`: migration `0004_attendance.sql` (`holidays`, `attendance_sheets`,
  `attendance_entries`, `staff_attendance`, grant of `attendance.correct`). Repository
  `attendance`: weekend setting, holidays, section roster, sheet submission with one entry per
  enrolled student in a single bulk insert, corrections that update only what differs and
  audit each changed entry, sheet history query, student history and summary, monthly section
  entries, absentee and daily status reports, staff attendance upserts with audit. 5
  integration tests.
- Done, `crates/api`: `/attendance/*` and `/dashboard` (spec §6.4). Teachers are limited to
  their own sections; corrections need `attendance.correct` (teachers get
  `403 already_submitted`); weekends, holidays and future dates are refused; staff attendance
  needs `teachers.manage`. 3 end-to-end tests.
- Done, UI: Attendance page with tabs Mark (section + date, everyone present, keyboard
  ↑ ↓ / P A L V / Enter, note per exception, summary line, submit or correct, history dialog),
  Monthly register (students × days, weekends and holidays shaded, totals and %), Absentees
  (date and class, guardian phone), Staff (present/absent/leave with note), Calendar (weekend
  days, holidays). Student profile Attendance tab (summary cards and days). Dashboard card
  shows today's percentage, "x of y sections marked" and chips linking to each unmarked
  section. English and Urdu strings.
- Done, demo data: Sunday off, Labour Day and Eid holidays, every section marked on each of
  the 47 school days of April and May (≈4 % absent, 2 % late, 1 % leave) by its class
  teacher, staff attendance for the same days.
- Tested: core 51 unit tests (5 new); db 26 integration tests (5 new); api 16 (3 new); demo
  data 6; vitest 79 (8 new: keyboard marking and exception-only payload, locked sheet for
  teachers, admin correction with history, holiday notice, staff save, calendar edits,
  dashboard card and no-school day, student view). fmt, clippy, eslint, prettier and tsc clean.
- Not done / notes: per-period attendance is out of scope for Lite v1 (daily only, as the
  task list specifies); the register and absentee views are on screen only, printing and
  Excel export arrive with Stage 6; the dashboard's other cards remain placeholders until
  Stage 7.

### 2026-10-30 — Stage 4: Exams, results and report cards (branch `m1-stage-4-exams`)
- Started from `main` after pull requests #1–#4 were merged (main `2cd360a`).
- Done, `crates/core`: `exams` module (default assessments whose weights must total 100,
  grading scheme with inclusive bands, pass mark and validation, percentage and totals with
  half-away-from-zero rounding kept at two decimals, weighted year result over the assessments
  held, competition-ranked positions, mark validation, sheet workflow states); new permission
  `exams.approve` (principal). 8 unit tests.
- Done, `crates/db`: migration `0005_exams.sql` (`assessment_types`, `assessment_max_marks`,
  `mark_sheets`, `marks`, grant of `exams.approve`). Repository `exams`: assessments, weights,
  grading (settings key), maximum marks locked once marks exist, marks entry with validation
  and change-level audit, submit (complete sheets only) → approve → reopen with audit, raw mark
  rows for results. 3 integration tests.
- Done, `crates/api`: `/exams/*` (spec §6.5). Setup needs `academics.manage`; teachers enter
  marks only for their assigned subjects/sections or as class teacher; approval needs
  `exams.approve`; result sheets with analysis and positions; report cards per student and per
  section with weighted year results and attendance. 1 end-to-end test covering the whole flow.
- Done, UI: Examinations page with Marks entry (subject list with status and progress, grid
  with validation, absent tick, Enter to move, save / submit / approve / reopen by permission),
  Results (result sheet, subject analysis, print), Report cards (list, print one or all) and
  Setup (assessments and weights with live total, maximum marks per class, grading scheme).
  Student profile Results tab. Report card and result sheet print pages reuse the light print
  CSS; one card per page. English and Urdu strings.
- Done, demo data: default assessments, maximum marks for every class and subject (20, 20,
  50, 100), Test 1 and Mid-year marks for every section and subject (≈2 % absent), all
  submitted by the class teacher and approved by the principal.
- Tested: core 59 unit tests (8 new); db 29 integration tests (3 new); api 17 (1 new); demo
  data 6; vitest 84 (5 new: marks validation, save payload and submit, approve/reopen for the
  principal, result sheet rendering, report card rendering, weights total check). fmt, clippy,
  eslint, prettier and tsc clean.
- Not done / notes: positions are computed for every assessment (the task list marks them
  optional); report cards and result sheets print through the print dialog, there is no
  server-side PDF; Excel export of result sheets arrives with Stage 6.

### 2026-10-30 — Stage 5: Encrypted backup and restore (branch `m1-stage-5-backup`)
- Started from `main` after pull request #5 was merged (main `d04630e`).
- Done, `crates/core`: `backup::archive` (`.mbk` format: readable header, AES-256-GCM payload
  with the header as associated data, per-file key wrapped for the school's X25519 public key
  via ephemeral Diffie–Hellman + HKDF-SHA256, SHA-256 trailer), key set v2 with the public key
  derived from the DEK, `change_password` (recovery key kept; applies to new files only),
  `with_public_key` for pre-stage-5 key sets. 5 new unit tests (round trip with password and
  recovery key, wrong password and wrong DEK, byte flips in header/payload/trailer, a forged
  header with recomputed checksum, truncation, password change semantics).
- Done, `crates/db`: migration `0006_backup.sql` (audit actions `backup`, `restore`,
  `backup_password_failed`; `backup_runs`). Repository `backup`: dependency-ordered COPY dump
  in a repeatable-read transaction, restore that truncates and reloads every table in one
  transaction with user triggers disabled and re-enabled, schema version check, record counts,
  run history with audit at start and finish. 2 integration tests.
- Done, `crates/api`: backup service (settings, manual run, pruning, preview, restore, password
  change, unlock, status) and `/backup/*` routes (spec §6.6); scheduler task started by
  `madrasa-server` and the desktop shell (once a minute: run when past the time and none
  succeeded today; retry failures after an hour); dashboard carries the backup status. 1 unit
  test for the due rule, 2 end-to-end tests (round trip with data changed in between, wrong
  password and wrong recovery key audited, tampered and non-backup files refused with 422,
  password change, pruning, permissions).
- Done, UI: Backup page (status with 2-day warning and last error, Back up now, schedule and
  folder settings, file list with Restore → preview dialog showing date, school, app version,
  counts in the backup versus now and the schema check → confirm with password or recovery key →
  logout; restore from any path; password change with the "new backups only" warning; run
  history; one-time unlock form when the key set predates stage 5). Dashboard card "Last
  backup". English and Urdu. 6 component tests.
- Tested: core 64 unit tests (5 new); db 31 integration tests (2 new); api 19 (2 new) + 1 unit;
  demo data 6; vitest 90 (6 new). fmt, clippy, eslint, prettier and tsc clean.
- Not done / notes: the folder is a path typed or pasted into the settings (no native folder
  picker yet; the Tauri dialog plugin is not bundled); a restore made with an older app version
  is refused rather than migrated forward; the scheduler runs only while the app is open (Lite
  has no system service), which the nightly default assumes.

### 2026-10-30 — Stage 6: Excel/CSV import wizard (branch `m1-stage-6-import`)
- Started from `main` after pull request #6 was merged (main `733a31b`).
- Done, `crates/core`: `imports` module (fields for students + guardians, staff and opening
  balances with English and Urdu heading aliases, header auto-mapping, cleanup of text,
  digits, dates, phones, CNICs, gender, relationship, money and class names, row validation,
  in-file duplicate keys). 6 unit tests with messy inputs.
- Done, `crates/db`: migration `0007_imports.sql` (`import_batches`, `import_batch_rows`);
  repository `imports` (batches, effects, rollback that soft-deletes created rows, cancels
  vouchers and restores old values); `fees::create_opening_balance`; student and staff
  lookups by number, legacy ID and CNIC. 1 integration test.
- Done, `crates/api`: import service (xlsx/csv parsing with calamine and csv, templates,
  saved mappings, preview and apply decided against classes, sections, existing records and
  in-file duplicates, skip/update/merge, guardians linked by phone or CNIC, enrollments,
  opening-balance vouchers, rollback) and `/imports/*` routes; `/export/*` routes writing
  `.xlsx` (rust_xlsxwriter) or `.csv` with BOM for seven lists. 2 end-to-end tests using
  generated messy files (Urdu digits, day-first dates, odd headings, blank rows, duplicates,
  unknown class, bad B-Form, semicolon CSV with BOM).
- Done, UI: Import page (type → file → columns → check → done, template downloads, mapping
  table with sample values and remembered mappings, duplicate policy, preview grid with
  editable cells on error rows and an error-row CSV download, import, history with roll
  back) and Export to Excel/CSV buttons on students, staff, fee vouchers, defaulters,
  absentees, attendance register and result sheets. English and Urdu strings. 2 component
  tests.
- Tested: core 70 unit tests (6 new); db 32 integration tests (1 new); api 21 (2 new) + 1
  unit; demo data 6; vitest 92 (2 new). fmt, clippy, eslint, prettier and tsc clean.
- Not done / notes: files are read in the browser and sent as base64 (fine up to the 5,000-row
  limit; no streaming); a merged duplicate does not change the student's section; exports are
  one list at a time (no "export everything").

### 2026-10-31 — Stage 7: Pilot readiness (branch `m1-stage-7-pilot`)
- Started from `main` after pull request #7 was merged (main `1905ca8`).
- Done, year rollover: `madrasa_db::rollover` (preview and apply: promote one class up, top
  class graduates, chosen students repeat or leave, sections recreated with class teachers,
  fee structure and assessment types copied, new year activated, old year kept, audited;
  unpaid balances carry into the new year's first voucher as before), `/academic-years/rollover`
  endpoints, and a "Roll over year" dialog under Settings → Academic years (preview table,
  carried balance, warning when the year is young, exceptions by student number). 1 db test,
  1 api test, 2 vitest tests.
- Done, permissions: `docs/permissions.md` lists every permission against the four roles and
  a core unit test (`matrix_matches_docs`) fails if the document and the code ever differ.
- Done, dashboard: active students and staff, fees collected since the first of the month
  (net of reversals), defaulters past their due date with the outstanding total, today's
  attendance and the last backup, each shown only with the matching read permission.
- Done, performance check: `madrasa-demo load --yes --full-year` and the ignored test
  `crates/demo-data/tests/perf.rs` load the pilot profile (1,200 students, 120 staff, a full
  year of fees and attendance) and time the key queries. Measured on this build machine
  (debug build, local PostgreSQL 16):

  | Data loaded | Count |
  | --- | --- |
  | Students / staff | 1,200 / 120 |
  | Fee vouchers / receipts | 8,400 / 5,071 |
  | Attendance sheets (179 school days × 39 sections) | 6,981 |
  | Marks sheets | 642 |
  | Load time | 140 s |

  | Query | Time |
  | --- | --- |
  | Students directory, page of 50 | 17 ms |
  | Students search by name | 10 ms |
  | Fee vouchers for a month, all classes | 49 ms |
  | Defaulters as of today | 80 ms |
  | Student ledger totals / vouchers | 2 ms / 1 ms |
  | Daily collection summary | 1 ms |
  | Attendance status of every section | 60 ms |
  | Register entries for a section and month | 14 ms |
  | Absentees today | 69 ms |
  | Student attendance summary for the year | 1 ms |
  | Section marks / student marks for the year | 5 ms / 6 ms |
  | Full backup dump | 2.1 s |

  Every key screen is served by queries well under the one-second target; the UI adds only
  rendering time.
- Done, user guide: `docs/user-guide/en.md` and `docs/user-guide/ur.md` (setup, students and
  import, fees, attendance, exams, backup, year end).
- Done, release build: the existing tag-triggered Release workflow builds the Linux `.deb`
  and AppImage and the Windows NSIS installer; it now also runs manually
  (`workflow_dispatch`) and uploads the installers as artifacts. The procedure is in
  `docs/RELEASING.md`. CI still builds all bundles on every push.
- Tested: core 71 unit tests (1 new); db 33 integration tests (1 new); api 22 (1 new) + 1
  unit; demo data 6 + 1 ignored performance test; vitest 94 (2 new). fmt, clippy, eslint,
  prettier and tsc clean.
- Not done / notes: the rollover keeps section names and class teachers but does not move
  subject assignments to the new sections (teachers reassign them under Teachers); a
  rollover cannot be undone (restore a backup instead); installers are unsigned.

### 2026-10-31 — Automatic late fees (branch `m1-auto-late-fees`)
- Started from `main` after pull request #8 was merged (main `4c5ba9e`).
- Done: `fees::assess_due_late_fees` adds the policy's late fee to every unpaid or partly paid
  voucher past its due date that has no fee yet, in one pass per school, idempotent; the
  api `late_fees` module runs it at start and after every local midnight (date checked once a
  minute so a PC that slept through midnight catches up; a failed pass retries after ten
  minutes), from the server binary and the desktop shell; `POST /fees/late-fees/run`
  (`fees.manage`, optional `as_of`, never in the future) runs it by hand and returns the count
  and total. Waived vouchers are never re-assessed; paid, carried and cancelled ones are left
  alone; a zero late fee percent switches the pass off.
- Done: every late fee assessment (automatic, at payment or at carry-forward) is now its own
  audit action `late_fee` with the voucher before and after; automatic ones have no actor.
  Migration 0008 extends the audit CHECK and adds a partial index for the daily scan.
- Done: spec §5.5 and §6.3, user guide (en, ur), TS client (`runLateFees`, `LateFeeReport`).
- Tested: db `automatic_late_fees_apply_once_after_the_due_date_and_respect_waivers` (due day
  adds nothing, day after adds 10% to both vouchers, second pass adds nothing, audit trail
  `create, late_fee` by the system actor with old and new amounts, defaulters and ledger show
  the fee, waiver stands through later passes, partial payment keeps its one fee, zero percent
  adds nothing); api `late_fee_pass_adds_fees_once_keeps_waivers_and_is_audited` (403 for a
  teacher, future date refused, same figures through the API, `/audit` holds two `late_fee`
  entries); api unit `runs_at_start_and_once_per_day`. fmt, clippy, eslint, prettier and
  tsc clean.
- Done: every "today" in the API now comes from one place, `madrasa_api::clock::today()`
  (the PC's local calendar date, which is the school's in Lite): dashboard, defaulters,
  collections, absentees, attendance edit limits, payment and reversal dates, default
  admission and leaving dates, import defaults, and the late fee pass. Before this the
  routes used the UTC date, which is yesterday until 05:00 in Pakistan. The UI's date defaults
  (`src/lib/dates.ts` `todayIso`) use the local date too instead of `toISOString()`, which
  had the same fault. A school time zone setting will replace the machine's zone in `clock`
  without touching the callers. vitest test for `todayIso`.
- Not done / notes: no UI button for the manual run (the pass runs on its own; the endpoint is
  there if one is wanted).

### 2026-10-31 — Schools outside Pakistan (branch `m1-international`)
- Started from the head of pull request #9 (`bf5224c`); to be rebased onto `main` once #9 is
  merged (the branch carries no other changes).
- Done, school profile: `country`, `time_zone` columns on `schools` (migration 0009, which
  also allows any academic year start month 1–12), `madrasa_core::locale` tables of
  countries (25), currencies (25, two minor units only) and IANA time zones (`chrono-tz`),
  `GET /system/locale`, `PATCH /schools/current`, Settings → School profile card. The API
  keeps the profile in `AppState` and the clock takes the school's zone from it.
- Done, setup wizard: country select (sets currency, time zone and the usual year start),
  month select 1–12, currency and time zone selects; the country's school weekend is written
  at setup.
- Done, country rules: Pakistan keeps the 13-digit CNIC / B-Form and 11-digit phone rules;
  elsewhere the ID field takes the country's name (`National ID`, `Emirates ID`…) and 3–40
  characters, phones 7–15 digits with optional `+`. Applied in the student and staff
  endpoints and in the import. The ID field was already optional.
- Done, grades and imports: `K`, `KG`, `Kindergarten`, `Grade N` share class keys; the class
  preset is K–12 for US/CA; imports accept first + last name columns, a single
  parent/guardian column (stands in for the father's name), month-first numeric dates for
  the US, "Homeroom", "Enrollment date", "Job title", "Hire date" and similar headings;
  guardian email is imported. Required-field checks (API and wizard) know the alternatives.
- Done, printing: vouchers and receipts show the school's currency code instead of `PKR`.
- Tested: core 78 unit tests (locale 4, imports 3 new); api `international.rs`:
  `us_school_setup_defaults_and_country_rules` (country validation, US defaults, weekend
  Sat+Sun, free-form ID and US phone accepted, bad phone refused, profile change, locale
  lists) and `maple_ridge_register_imports_in_full` (a generated US-style register of 1,123
  students and 40 staff: every column auto-mapped, preview 1,123 creates and 0 errors, apply
  creates all, month-first dates, names joined, parent as father, K and "Grade 10" matched,
  staff titles, +1 phones and hire dates); vitest 96 (setup wizard country logic). fmt,
  clippy, eslint, prettier and tsc clean.
- Not done / notes: the owner's `Madrasa_Demo_MapleRidge_USA.xlsx` did not reach this session
  (no attachment arrived in the sandbox), so the test builds an equivalent workbook with the
  same shape and counts (`crates/api/tests/international.rs`); run the wizard on the real
  file and look at the preview before trusting the mapping. UI labels still say "Class" and
  "CNIC" in Urdu for Pakistani schools; only the ID label follows the country. Currency
  symbols are not yet used in list screens (amounts stay unlabelled there).

### 2026-10-31 — v0.1.0-alpha test report fixes (branch `fix/alpha-test-report`)
- Started from `main` after #9, #10 and #11 were merged (`da005a8`, tagged v0.1.0-alpha).
- Done, staff logins: `POST /staff/{id}/login` (username, password, role), `PATCH` (role,
  on/off) and `POST …/login/password`, all behind `users.manage` (principal, admin); the
  Staff dialog's *Login account* box offers Create login, role select, "Can log in" switch
  and Reset password. `roles::set_role` replaces a user's roles (soft-deletes the others,
  audited); `users::set_password` is audited as `password_reset` (migration 0010); a user
  cannot disable their own account. `StaffView` gains `login_active`.
- Done, imports: numbered guardian headings (`Guardian 1 …`, `Parent 1 …`, `Primary
  guardian …`) map to the first guardian and `Guardian 2 …` / `Secondary …` to a new second
  guardian (linked as non-primary); `Family ID` is a field, stored in `custom.family_id` on
  the student and the guardians it creates, and a later row with the same family id and
  guardian name reuses the guardian, so siblings arrive linked (index in migration 0010).
- Done, sheet selection: `POST /imports/parse` picks the sheet named after the import kind
  over a read-me sheet (`pick_sheet`); with several sheets the wizard shows a picker before
  the mapping step, preselected the same way (`src/lib/sheets.ts`), and re-parses if the
  user picks another sheet.
- Done, Linux compatibility: CI and Release build the Linux bundles on `ubuntu-22.04` so
  they run on Debian 12/13 and Ubuntu 22.04+; a workflow step fails if the binary needs a
  glibc newer than 2.35. TESTING.md states the minimum versions.
- Tested: api `staff_logins` (accountant refused; validation of role/username/password;
  teacher enters marks but 403 on fees, settings and users; role change to accountant flips
  that; principal resets the password, old one fails; disable blocks login, self-disable
  refused; audit holds `password_reset` and never a hash); `international` Maple Ridge test
  rebuilt with a Read Me sheet first, Guardian 1/2 groups and Family IDs (1,123 students,
  "Students" sheet picked, every column mapped, 0 errors, second guardian linked
  non-primary, siblings share one guardian record and list each other, staff read from the
  same workbook, explicit `sheet` honoured); core `numbered_guardian_headers_…` and api
  `pick_sheet` unit tests; vitest `pickSheet` and an ImportPage sheet-picker test.
- Not done / notes: no UI for listing all user accounts (Settings → Users) yet; logins are
  managed from the staff record only. Family IDs link siblings only when the guardian name
  matches across rows (a misspelt name makes a second guardian record).

### 2026-10-31 — Second alpha test report (v0.1.0) fixes (branch `dev/alpha2-test-report`)
- Started from `main` after #12 was merged (`8fad981`).
- 1 Guardian matching: `guardians::find_matching` now needs the name with the phone (ID
  number alone still matches); family id + name first; `find_by_phone` feeds a preview
  warning when a phone is on file under another name (database or earlier rows of the same
  file); the student form uses the same rule. Test: `guardians_never_merge_on_phone_alone`
  (four students, one phone: family and name+phone link, a different name does not; the
  warning appears; siblings and the sibling discount follow the corrected guardians; the
  student form does the same).
- 2 Setup: `first_year` creates and activates the running academic year
  (`first_year_start`), `default_classes` the country's classes with section A; the wizard
  ticks the classes box by default; the year dialog offers all months and defaults to the
  school's start month and the current year (`src/lib/months.ts`). Tests: api
  `setup_creates_the_first_year_and_default_classes_so_import_works_at_once` (import right
  after setup), `setup_without_the_flags_creates_no_year`, unit tests for the year rule and
  the preset, vitest for the month helpers and the wizard request.
- 3 Teacher UI: `NavItem.permission` hides Fees, Settings, Backup and Import (and anything else
  the role lacks) from the sidebar; routes are wrapped in `RequirePermission` showing a "You
  don't have access" page; the staff API blanks phone, email and ID for callers without
  `teachers.manage`. Tests: api `teachers_do_not_see_colleagues_contact_details`, vitest
  `visibleNavItems`.
- 4 Currency: `useMoney()` formats every amount with the school's symbol (`Rs 2,500.00`,
  `$ 40.00`); the `Amount` component, dashboard, pay dialog, policy example, rollover dialog
  and printouts use it. Tests: `withCurrency` unit test; the fee screen tests now expect the
  symbol.
- 5 Import: rollback asks for confirmation (dialog); new fields status, city, state, postal
  code (students) and staff type, subject/department, grades (staff) are stored (status on
  the row, the rest in `custom`); the preview lists unmapped columns; the wizard's state
  lives in `importDraft` and survives navigating away; the problem-row filter and the error
  download keep their tests. Tests: api
  `new_import_fields_are_stored_and_unmapped_columns_are_reported`, core field test, vitest
  for the draft store and the confirmation dialog.
- 6 "No policy yet": not present on `main` (the policy card loads the defaults); nothing to
  change.
- 7 Small items: linked guardians are editable on the student form (`guardianDrafts`,
  `guardianPatches`, PATCHed on save; new guardians added while editing are linked); the
  desktop shell refuses to start as root with a plain message (`root_check`); TESTING.md no
  longer says PKR only and follows the wizard's steps and the classes checkbox.
- Not done / notes: no Settings → Users list; the import draft is kept for the app session
  only (not across restarts); teacher redaction applies to the staff API, the attendance
  screens never showed contact details.

### 2026-11-01 — Pre-pilot polish (branch `dev/pre-pilot-polish`)
- Started from `main` after #13 was merged (`4b7a3e7`, tagged v0.1.2-alpha).
- 1 Version: versions bumped to 0.1.3 everywhere; `madrasa_api::VERSION` = Cargo version or
  `MADRASA_RELEASE` from the tag at build time; the Release workflow checks the tag against
  `Cargo.toml` and sets the variable; `scripts/set-version.sh`; `/system/features` carries
  `version`; Help → About shows version and edition. Tests: unit test that package.json,
  tauri.conf.json and Cargo agree; api test that `/health` and `/system/features` report
  `VERSION`; shell test for the About panel.
- 2 Settings → Users: `GET /users` with staff link and last login, `POST /users`,
  `PATCH /users/{id}`, `POST /users/{id}/password`, sharing the staff-record login helpers
  (`routes/staff.rs`); `UsersCard` with role select, "Can log in" switch, Reset password and
  Create login (staff without a login only). Tests: api
  `users_page_lists_logins_with_staff_links_and_manages_them`; vitest `UsersCard` (list,
  role/disable/reset, create).
- 3 GitHub Actions on Node 24: checkout, setup-node, cache, upload-artifact and
  pnpm/action-setup moved to their v5 lines in CI and Release.
- 4 Demo school in the wizard: `POST /setup/demo` loads the demo school and finishes setup
  with the chosen administrator; the wizard's first step offers it, marked as demo data.
  Tests: api `demo_school_setup_loads_sample_data_and_finishes_setup` (small size: 60
  students, 12 staff, active year, is_demo, setup done once); vitest wizard path.
- Not done / notes: the demo loader takes a minute or two on a real install (1,200 students,
  debug figures were ~140 s for the full-year variant); the wizard shows a spinner. The
  version string on dev builds is the plain Cargo version.

### 2026-11-05 — LAN mode and marks via Excel (branch `dev/lan-mode`)
- Started from `main` after #14 was merged.
- 1 LAN mode (ADR 0002, spec §6.13): `crates/api/src/lan.rs` (self-signed identity with
  SHA-256 fingerprint, TLS listener via `axum-server`, pinned-certificate reqwest client,
  loopback proxy, UDP discovery beacon and `discover`/`probe`); desktop shell `mode.rs`
  (`mode.json`: standalone | server | client) and `backend.rs` (server: database + loopback
  API + TLS on `0.0.0.0:8471` + beacon on UDP 47121; client: pinned proxy only); Tauri commands
  `get_mode`, `set_mode`, `network_info`, `discover_servers`, `probe_server`; UI `ModeGate` +
  `ModePage` (three choices, port and firewall note for the server, discovery list / manual
  address / fingerprint confirmation for the client) and Settings → Network. Tests: `lan`
  unit tests (fingerprint, URL normalisation, identity persistence, discovery round trip);
  `crates/api/tests/lan.rs`: pinning (wrong fingerprint refused, right one accepted, 401 and
  502 relayed) and eight teachers logging in and saving marks and attendance at once through
  TLS and the proxy with 403s on fees, settings and other sections; desktop `mode.rs` unit
  test; vitest `ModePage` (standalone, server with port, client discovery + confirm, manual
  address error).
- 2 Marks via Excel: `GET /exams/sheet/template` (students, roll numbers and max marks
  pre-filled) and `POST /exams/sheet/upload` (same validation and `save_sheet` as on-screen
  entry; any issue → nothing saved and `issues[{row_no, message}]`; otherwise a draft to
  submit and approve). Marks entry has **Download Excel sheet** and **Upload filled sheet**.
  Tests: `crates/api/tests/marks_excel.rs` (template → bad row rejected with row number →
  fixed sheet saved → submit → approve); vitest MarksTab download/upload.
- Docs: TESTING.md §6 (server PC, Windows Firewall `netsh` command, client PCs) and tasks 16
  and 17; user guide en/ur; spec §6.13 and §7.
- Not done / notes: the NSIS installer does not add a firewall rule (the hook could not be
  written in this session); Windows shows its own "allow on private networks" prompt on first
  listen and the exact `netsh` command is on the mode page, under Settings → Network and in
  TESTING.md. Multi-PC use is tested in CI against a real TLS listener and proxy on one
  machine; a real two-PC Wi-Fi check is on the hand-test list.

### 2026-11-05 — Network settings for existing installs, demo password error (branch `dev/network-settings`)
- Started from `main` after #15 was merged (v0.1.4).
- Report: an install upgraded from 0.1.3 showed neither the mode screen nor a Network card.
  The card only rendered in server or client mode, and an upgraded install had no saved mode
  at all. Now `ModeConfig::current` treats an install with a `pg` folder but no `mode.json`
  as standalone (no question on upgrade), and Settings → Network is shown in every mode for
  `settings.manage`: current mode, a server's addresses/port/fingerprint/firewall note, a
  client's server/fingerprint, and **Change**, which opens the same chooser as the first
  start (`ModeChooser`, shared with `ModePage`). `set_mode` on a running install stops the
  backend cleanly (database included, `Backend::stopped`) and restarts Madrasa in the new
  mode after replying, so the card can say so. Tests: desktop
  `an_upgraded_install_with_a_database_is_standalone_until_changed`; vitest `NetworkCard`
  (standalone → server switch with restart notice, server details, client details, absent
  in the browser build); `ModePage` tests kept.
- Setup wizard: the demo-school dialog validated both passwords with one message under the
  backup field; it now shows the error under the field that is short. Test: vitest
  `shows a short demo password error under the field it belongs to`.
- Version 0.1.5 (`scripts/set-version.sh`).

## M2 — Lite complete

### 2026-11-06 — Stage 1: Report engine and student reports (branch `dev/m2-stage-1-reports`)
- Started from `main` after #16 (v0.1.5). Migration `0011_reports` (audit action `report`).
- Report document model (`madrasa_api::reports`), catalogue with typed parameters, and
  three renderers: JSON, Excel (export module) and PDF typeset in-process with typst 0.15 and
  Inter + Noto Nastaliq Urdu compiled in (`crates/api/fonts`, OFL). Urdu PDFs are shaped and
  right-to-left; tested by rendering.
- `GET /reports` and `GET /reports/{code}` (`reports.read`), every run audit-logged.
- Student reports: list, class strength, admissions and withdrawals, guardian contacts,
  birthdays, missing data; English and Urdu from one string table (`reports/text.rs`).
- Reports page: library by module, parameter form built from the catalogue (year, class →
  section, status, month, date range), on-screen table with groups, subtotals and total,
  PDF and Excel downloads in the current language, print.
- Tests: unit (text table complete in both languages; catalogue; PDF: data never markup,
  pagination, Urdu RTL source and render), integration `crates/api/tests/reports.rs` (every
  student report against seeded data, Urdu labels, xlsx and pdf bytes, 400/403/404, audit),
  vitest `reports.test.tsx` (library, parameters, run, downloads, Urdu RTL); shell test
  updated now that Reports is a real module.
- Notes: PDF rendering adds typst to the API crate (longer compile, ~2 MB of fonts in the
  binary). Teachers do not hold `reports.read`; stage 2 opens exam and attendance reports to
  them for their own sections.

### 2026-11-06 — Stage 2: Fees, attendance and exams reports (branch `dev/m2-stage-2-reports`)
- Started from stage 1 (rebased on `main` once #18 merged). No migration.
- Fees: collection by day and method, monthly collection, defaulters by class with ageing,
  student ledger, concessions register, voucher register, late fees, billed vs collected by
  head (proportional split, noted). Attendance: monthly register (day columns), attendance
  by section, absentees for a period, below-threshold list, staff attendance. Exams: result
  sheet, merit list, subject analysis, grade distribution, failing by subject, report cards
  (one page per student in the PDF, same content as the on-screen card).
- Access per report: office roles with `reports.read` run everything; teachers run attendance
  and exam reports for their own sections with `attendance.read`/`exams.read` (catalogue
  filtered, 403 elsewhere). New parameter kinds: assessment, single date, number, student
  number; the Reports page builds the controls and seeds them from the URL, and a `?run`
  link runs at once (dashboard shortcuts for today's collection and today's absentees).
- Tests: `crates/api/tests/reports_stage2.rs` (every report against seeded fees, attendance
  and marks, figures cross-checked with `/fees/collections`, `/fees/defaulters`,
  `/attendance/register` and `/exams/results/section`; Urdu; teacher scope and 403s); unit
  tests for money formatting, the text table and report-card PDFs (three cards → three pages
  in both languages); vitest for the new controls, URL seeding and auto-run.
- Notes: payments are not recorded per fee head, so the head-wise report allocates each
  voucher's payments across its lines in proportion and says so.

### 2026-11-06 — Stage 3: Custom report builder (branch `dev/m2-stage-3-builder`)
- Migration `0012_report_definitions` (table + `reports.manage` granted to principal and
  admin); new permission in `madrasa_core::permissions` and `docs/permissions.md`.
- `madrasa_api::reports::builder`: field catalogue for nine sources (students, guardians,
  fee vouchers, fee receipts, fee balances, attendance entries, attendance summary, marks,
  staff), typed fields with bilingual labels, operators per type, assembly of parameterised
  SQL from catalogue expressions only, grouping with subtotals, limits.
- Endpoints: catalogue, list/create/update/delete saved reports (sharing needs
  `reports.manage`), preview without saving, run a saved report as JSON/PDF/Excel; runs and
  changes audit-logged. Section scope applies to every source but staff.
- UI: builder page (`/reports/custom/new`, `/reports/custom/:id/edit`): data source,
  columns with ordering, filter rows with type-aware controls, sort, group by, limit, share
  switch (hint for those who cannot share), preview, save; the library lists custom reports
  with run/edit/remove and the runner downloads PDF/Excel. Reports is now open to teachers
  (attendance and exam reports only); the custom section is hidden from them.
- Tests: unit (catalogue texts, parameterised assembly incl. an injection attempt, refusals
  for unknown source/field/operator/value); integration `crates/api/tests/reports_builder.rs`
  (catalogue in Urdu, preview rows and grouping, validation, private vs shared saving and
  listing, run as xlsx/pdf, audit, edit/delete rights, every source runs, teacher scope);
  vitest `builder.test.tsx` (build/preview/save, edit without share rights, library with
  edit/remove/download, hidden from teachers); `backup` migration count → 12.

### 2026-11-06 — Stage 4: Board Creator and board result import (branch `dev/m2-stage-4-boards`)
- Migration `0013_boards` (six tables; `import_batches.kind` gains `board_results`).
- `madrasa_core::boards`: bands, paper outcomes, candidate totals and status, positions.
- `madrasa_db::boards`: boards, exams (classes, papers matched by name so results survive
  edits), candidates (pre-filled roll numbers, written back to the student), results upsert.
- API `routes/boards.rs`: CRUD, bulk registration, the computed result sheet, result import
  in both layouts with guessed/remembered mapping, preview with row errors, apply as an
  import batch, rollback through the existing batch endpoint; `/students/{id}/board-results`.
- Reports: board result sheet, merit list, pass rates, student board result (new `board_exam`
  parameter kind). Demo data: BISE Karachi with SSC Part I (grade 9) and Part II (grade 10),
  every student registered with results.
- UI: Examinations → Boards tab (boards with bands, exams with classes and papers,
  candidates with editable roll numbers, result import: file → layout → column matching →
  check → import, undo); board results on the student profile; Reports pick a board exam.
- Tests: core unit tests (bands, outcomes, totals, positions); route unit tests (header
  guessing, marks cells); integration `crates/api/tests/boards.rs` (boards, exams, candidates,
  wide import with three kinds of error, long re-import that replaces and is rolled back,
  sheet figures, student results, four reports incl. Urdu PDF, deletes); vitest
  `boards.test.tsx`; `backup` migration count → 13; demo setup test covers the loader.

### 2026-11-06 — Stage 5: Terms and assessment components (branch `dev/m2-stage-5-terms`)
- Migration `0014_terms`: `terms` table; `assessment_types` gains `term_id`, `kind`
  (entered/calculated) and `formula`; every existing year gets one term "Year" (100) holding
  its assessments, so nothing changes for existing schools.
- `madrasa_core::exams::Formula` (sum, average, weighted, best of N) with validation (same
  term, distinct, no self-reference, no cycles) and evaluation; term and year weights
  validated per group.
- `madrasa_db::exams`: terms CRUD and weights, `create_component`/`update_component`,
  `create_default_structure` (one or two terms), per-term weight validation.
- API `routes/exam_terms.rs` (`/exams/terms`, `/exams/terms/weights`, `/exams/structure`);
  `routes/exams.rs` evaluates calculated components on read (`marks_for`/`evaluate_rows`),
  refuses sheets, templates, uploads and maximum marks for them, and builds report cards
  with `terms` and per-subject `term_percents` (year = weighted terms). PDF report cards show
  the term header and term result rows when a year has more than one term.
- UI: Examinations → Setup → *Terms and assessment components*: terms with weights in the
  year, add/remove terms, components grouped by term with kind, weight and "move to" term,
  defaults for one or two terms, add entered component, calculated component editor
  (formula, N, inputs, input weights). Marks entry and maximum marks list entered components
  only; report cards show term columns and term results.
- Tests: core unit tests (formula ops, validation); integration `crates/api/tests/exam_terms.rs`
  (defaults, per-term weights, best-of and weighted components, sheet refusal, computed
  result sheet, report card term/year percentages, two-term defaults, term weights, delete
  and move, PDF); vitest for the setup card (defaults select, formula editor) and the
  two-term report card; `backup` migration count → 14; the schema rules test lists the new
  tables of stages 3–5.

### 2026-11-06 — Stage 6: Subject allocation per student (branch `dev/m2-stage-6-subjects`)
- Migration `0015_subject_allocation`: `class_subjects.is_elective`, `subject_groups`,
  `subject_group_subjects`, `student_subjects`, `enrollments.subject_group_id`; batch kind
  `student_subjects`.
- `madrasa_core::subjects`: the effective-subjects rule (no allocation → every class subject;
  otherwise compulsory + allocated electives) and the subject-list splitter.
- `madrasa_db::subjects`: electives per class, groups CRUD, allocations per section and
  enrolment, `set_allocation` (audited with old/new); `exams::subject_roster`; rollback arm
  for `student_subjects`.
- API `routes/subjects.rs`: groups, section allocations (bulk), one student's subjects, Excel
  template, import parse/preview/apply as a batch. `routes/exams.rs`: rosters, sheet lists,
  save/submit, result sheets (`takes` per cell, totals over own subjects, analysis over
  takers) and report cards follow the allocation; `routes/academics.rs` class subjects carry
  `elective_ids`.
- UI: Settings → Academics: Elective toggle per class subject and a Subject groups card;
  Students → Subject allocation page (class/section pickers, group per student, elective
  checkboxes, set group for everyone, clear, save; Excel sheet download; import with column
  matching, check, import, undo); student profile Subjects tab; result sheet shows "—" for a
  subject a student does not take.
- Demo data: grades 9–12 have Physics, Chemistry, Biology and Computer as electives with the
  groups Pre-Medical and Computer Science; students alternate between them, one per section
  left unallocated; marks are entered for takers only.
- Tests: core unit tests; integration `crates/api/tests/subject_allocation.rs` (electives,
  groups, default-to-all, bulk and single allocation, roster/sheet/submit filtering, result
  totals and analysis, report cards, import with errors, apply, rollback, group removal);
  vitest `allocation.test.tsx`; schema rules test and `backup` migration count → 15.

### 2026-11-07 — Stage 7: Allocation screens (branch `dev/m2-stage-7-allocation`)
- `madrasa_core::allocation::balance`: the section-balancing proposal (sizes within one,
  remainder with the largest, moves from the end of the roll, excluded students untouched,
  deterministic).
- `madrasa_db::allocation`: teacher cells of a year, `set_cell_teacher` (audited), class
  rosters with gender, next free roll number.
- API `routes/allocation.rs`: teacher grid with loads, cell update, class sections view,
  balance preview, moves (keep or reassign roll numbers) through `students::enrol`.
- UI: Teachers → **Teacher allocation** (sections × subjects grid with a select per cell,
  class teacher column, class filter, teaching load list); Students → **Section moves**
  (section cards with size, gender split and capacity, tick students, move to a section with
  the roll-number choice, **Balance sections** preview dialog and apply).
- Tests: core unit tests; integration `crates/api/tests/allocation.rs` (grid, cell set/replace/
  clear, loads, agreement with the staff assignment list, refusal of an untaught subject,
  audit entries, sections view, balance preview with an excluded student, apply with
  reassigned and kept roll numbers, cross-class refusal, teacher 403); vitest
  `moves.test.tsx`.

### 2026-11-07 — Stage 8: Consent manager (branch `dev/m2-stage-8-consents`)
- Migration `0016_consents`: `consent_types` and the append-only `consents` history.
- `madrasa_core::consents`: who needs a consent (all or a class range) and the current state
  from the latest row with yearly renewal (given, refused, withdrawn, expired, missing).
- `madrasa_db::consents`: types CRUD, `record` (audited), history, latest row per student
  and type.
- API `routes/consents.rs`: types, one student's consents with guardians and history,
  section matrix with the count to collect, bulk recording; guardian required for a given
  guardian consent. Reports: missing consents, consents expiring with the year, consent
  form list (new `consent_type` parameter kind).
- UI: Settings → Academics → Consent types card; Students → **Consents** page (section
  matrix, state per cell, guardian per student, recorded-on date, save); student profile
  Consents tab (states, record form, history); Reports pick a consent type.
- Demo data: Photos (once), Trips (yearly) and Data use (either) with most students given,
  a few refused and some missing.
- Tests: core unit tests; integration `crates/api/tests/consents.rs`; vitest
  `consents.test.tsx`; schema rules test and `backup` migration count → 16.

### 2026-11-07 — Stage 9: M2 close-out (branch `dev/m2-stage-9-closeout`)
- Demo data: a shared saved custom report ("Girls by section") so the Reports page shows a
  custom report out of the box; with boards, terms (one "Year" term), subject groups and
  consents already loaded, every M2 feature has demo data.
- docs/permissions.md: descriptions refreshed for allocations, consents, boards and terms (no
  new permissions in stages 5–8; the matrix test still passes).
- Spec sections 5.11–5.17 and 6.14–6.20 checked against the code; TESTING.md tasks 18–25
  and the user guides cover every stage.
- The owner summary below lists what to test by hand and the decisions left open.

### 2026-11-07 — v0.2.0-alpha test report fixes (branch `dev/alpha3-fixes`, v0.2.1)
- B1 backup status restricted to `backup.manage`; B2/B3 vouchers and report cards typeset
  by the report PDF engine with the currency on every amount (`/fees/vouchers/{id}/pdf`,
  `/fees/vouchers/pdf`, `/exams/report-cards/pdf`); B5 payment-reference block by country,
  editable in Settings → School; B7 demo data relative to the install date with the DEMO
  banner on every screen; O1 absence rule per calculated component; O5 rollover keeps every
  class's sections; B4, B6, O3, O2, O8 polish. Spec §6.21.
- Tests: backup (teacher 403), voucher PDF unit and route tests, report-card PDF route,
  payment reference defaults (PK and US) and setting, demo year relative to today, formula
  absence rule (core and route), rollover sections, import rollback keeps re-linked
  guardians, defaulters unpaid months, guardian phone search, allocation warnings; vitest for
  the banner, the absence option, the print buttons and the derived import hint.

## Standard alpha

### 2026-11-07 — Stage 0: Standard foundation (branch `dev/std-stage-0-foundation`)
- `madrasa_core::edition`: feature codes `timetable`, `leave`, `admissions`, `inventory`,
  `directory_login`, `readonly_api`; `hr` replaces `hr_payroll`; `STANDARD_ONLY` lists what
  Standard adds; Lite's list is unchanged (nine modules, asserted by a test).
- API `routes/edition.rs`: `GET/PUT /settings/edition`; `roles::sync_defaults` grants the
  built-in roles their missing defaults on a switch; audited.
- UI: Settings → Edition card (current edition, what Standard adds, switch); sidebar items
  for the Standard modules ("Coming soon" until built), hidden on Lite; feature codes and
  strings (en/ur).
- Tests: core edition tests, `tests/api.rs` (switch both ways, Nexus refused, features
  reload), vitest for the card and the sidebar in both editions.

### 2026-11-07 — Stage 1: Timetable (branch `dev/std-stage-1-timetable`)
- `madrasa_core::timetable`: the timewizer engine vendored (ADR 0003) with its 135 tests;
  kept close to upstream (`#[rustfmt::skip]`, a few clippy allows) so fixes carry across.
- Migration 0017: `timetable_shifts`, `rooms`, `timetable_lessons`, `timetable_rules`,
  `timetables` + `timetable_cards`, `timetable_absences` + `timetable_covers`; permissions
  `timetable.read` (every role) and `timetable.manage` (principal, admin), granted to
  existing roles only on Standard/Nexus installs (Lite roles get them on the edition switch).
- `crates/db::timetable`: CRUD for the above, seeding lessons from the allocation grid,
  `setup_for_year` (Madrasa entities → engine setup), cards replaced per generation/edit.
- API `routes/timetable.rs` + `timetable_jobs.rs`: setup view with pre-check and load,
  background generation with progress and cancel, manual moves/swaps/locks with the
  engine's reasons, publish, views (current, by timetable, own week with cover duties),
  PDF printouts, substitutions (absences, candidates, covers, counts, cover sheet). Reports
  `timetable.section`, `teacher_load`, `free_rooms`, `cover_sheet` (en/ur), hidden on Lite.
- UI: Timetable module (tabs Timetable, Substitutions, and for `timetable.manage` Setup,
  Lessons, Generate); grid by section/teacher/room with a move/swap/lock dialog; shift
  editor (days, periods, times, break); rooms; school rules; per-teacher/section/room
  availability grid; lessons with seeding and load; generation progress and report;
  Dashboard card "My timetable" for teachers. Strings en/ur.
- Tests: core (231 incl. the ported solver), db unit tests, `tests/timetable.rs` (Lite
  refusal, full Standard flow: shifts, lessons, rules, generation on a 4-section school
  with 84 sessions, collision assertions, move/blocked/lock, publish, own week, PDFs,
  reports, absence → candidates → cover → cover sheet, lesson deletion, back to Lite),
  report unit tests, vitest for the grid helper, the page tabs and the Dashboard card.
- Deferred: attendance per period (behind a setting): it changes the Lite attendance
  tables; left for a later stage (noted in TASKS-STANDARD.md).

### 2026-11-07 — Stage 2: Leave (branch `dev/std-stage-2-leave`)
- `madrasa_core::leave`: working days, half days, balances and carry-over, request states,
  overlap checks (unit tests). `StaffStatus::Late` added (accepted by the API on Standard).
- Migration 0018: `leave_types`, `leave_balances`, `leave_requests`,
  `student_leave_requests`; `staff_attendance` gains `late`, `arrived_at` and
  `leave_request_id`; `attendance_entries` gains `leave_request_id`. Permissions
  `leave.read` (every role), `leave.manage`, `leave.approve` (principal, admin); granted to
  existing roles only on Standard installs.
- `crates/db::leave`: types, balances (computed used/pending), carry-over, staff requests
  (create with checks, decide, withdraw) that write and remove `staff_attendance` rows,
  student requests that mark and unmark attendance entries and recount sheets, the hook in
  `attendance::save_sheet`, the approver setting, the staff marks for the register.
- API `routes/leave.rs`: types, settings, balances, carry-over, requests (own or for others,
  decide by `leave.approve` or the configured approver, withdraw), student requests (decide
  by the office or the section's teachers), staff register; `/attendance/staff` takes
  `late` + `arrived_at` on Standard. Reports `leave.balances`, `leave.register`,
  `leave.students` (en/ur).
- UI: Leave module (Staff leave, Student leave, Balances, Staff register; Leave types +
  approver for `leave.manage`), student profile "Leave" tab, staff attendance "Late" with
  time, leave status report parameter. Strings en/ur.
- Tests: core, `tests/leave.rs` (Lite refusal incl. late marks; full Standard flow: types,
  balances, own request, overlap, scope, decisions, attendance marked/protected/unmarked,
  no-approval type, balance limit, weekend-only refusal, approver setting, student leave
  with sheets before and after approval, correction, absentee note, withdrawal, register
  with late time, carry-over, adjustment, reports, type deletion, back to Lite), vitest
  for the page and the register, existing suites updated for the new fields.

### 2026-11-08 — Stage 3: Admissions pipeline (branch `dev/std-stage-3-admissions`)
- `madrasa_core::admissions`: stage kinds, the default pipeline, move rules, the funnel
  (unit tests). `ImportKind::Enquiries` with its field list.
- Migration 0019: `admission_stages`, `admission_applications`, `admission_events`;
  permissions `admissions.read` (principal, admin, accountant) and `admissions.manage`
  (principal, admin), granted to existing roles only on Standard installs.
- `crates/db::admissions`: stages (defaults on first use), applications with filters,
  stage moves recording dated events, linking the created student, the funnel.
  `fees::create_one_off_voucher` for the admission fee.
- API `routes/admissions.rs`: stages, settings (fee head), board, applications CRUD,
  single and bulk moves, conversion (student + enrolment + guardian + voucher in one
  transaction, duplicate check with `force`), offer letter PDF (en/ur). Import engine:
  enquiries preview, apply and rollback. Reports `admissions.funnel`, `by_class`,
  `pending_tests`.
- UI: Admissions module (Pipeline board, Applications list with bulk move, Stages + fee
  head for `admissions.manage`), application dialog with history, move, offer letter and
  the convert dialog; "Admission enquiries" kind on the Import page. Strings en/ur.
- Tests: core, `tests/admissions.rs` (Lite refusals; Standard: default and custom
  stages, applications, validation, moves and history, bulk move, board funnel, offer
  letters, conversion with voucher and guardian, refusals after conversion, duplicate
  check and force, enquiry import with a class error and rollback, reports, back to
  Lite), vitest for the board, the dialog conversion and the reader view.

### 2026-11-08 — Stage 4: Library (branch `dev/std-stage-4-library`)
- `madrasa_core::library`: member kinds, loan rules per kind with validation, due date,
  days late, fine, may-borrow check, renewal rule, waiver cap (unit tests).
- Migration 0020: `library_books`, `library_copies`, `library_loans`; permissions
  `library.read` and `library.manage`, granted to existing roles only on Standard installs.
  New `librarian` login role (Standard/Nexus only) with the library grants plus read access
  to the school, academics, students and staff.
- `crates/db::library`: books with copy counts, copies by accession number, loans (issue,
  renew, return with lost handling, waive, settle to a one-off voucher for students), stock
  by category, most borrowed.
- API `routes/library.rs`: books and copies, member search and member page, the desk
  (lookup, issue, renew, return, waive, settle), settings, stock. Reports
  `library.overdue`, `fines`, `most_borrowed`, `stock`.
- UI: Library module (Desk, Catalogue, Loans, Stock, Settings for `library.manage`),
  librarian role offered in the login dialogs on Standard. Strings en/ur.
- Tests: core, `tests/library.rs` (Lite refusals incl. the librarian role; Standard:
  catalogue and copies, duplicate accession, member search, issue/renew/return with fines,
  limits and overdue refusals, lost copy, waiver, settle to voucher, stock, reports, back to
  Lite), vitest for the desk flow and the catalogue.

### 2026-11-08 — Stage 5: Transport (branch `dev/std-stage-5-transport`)
- `madrasa_core::transport`: month coverage of an assignment, the monthly charge
  (assignment → stop → route), capacity warnings and seats left (unit tests).
- Migration 0021: `transport_vehicles`, `transport_routes`, `transport_stops`,
  `transport_assignments` (one open assignment per student and year); permissions
  `transport.read` (principal, admin, accountant) and `transport.manage` (principal, admin),
  granted to existing roles only on Standard installs.
- `crates/db::transport`: vehicles, routes with their ordered stops (kept, renamed, added or
  removed unless students are on them), assignments that end the previous one, students of a
  section with their stop, rosters, the monthly charge for a student and month, fee by route.
  `fees::generate_vouchers` adds the transport line under the configured fee head.
- API `routes/transport.rs`: vehicles, routes, roster with the capacity warning, students,
  single and bulk assignment with the warning, settings. Reports `transport.manifest`,
  `without_stop`, `fee_by_route`.
- UI: Transport module (Routes with the route and stops dialog, Assign by section with bulk
  assignment, printable Roster, Vehicles, Settings for `transport.manage`). Strings en/ur.
- Tests: core, `tests/transport.rs` (Lite refusals; Standard: vehicles, routes and stops,
  assignment with route, stop and own amounts, a route change that ends the previous
  assignment, students without transport, roster with an over-capacity warning, stop
  reordering and refusals, the voucher line after a fee head is set, reports, back to Lite),
  vitest for the routes dialog, the bulk assign flow and the roster.

### 2026-11-08 — Stage 6: Hostel (branch `dev/std-stage-6-hostel`)
- `madrasa_core::hostel` (and `periods`, shared with transport): monthly charge
  (allocation → room → hostel), bed numbers, occupancy, the leave-out consent rule, overdue
  returns (unit tests).
- Migration 0022: `hostels`, `hostel_rooms`, `hostel_allocations` (one open allocation per
  student and year, one resident per bed), `hostel_register`; permissions `hostel.read`
  (principal, admin, accountant) and `hostel.manage` (principal, admin), granted to existing
  roles only on Standard installs.
- `crates/db::hostel`: hostels and rooms with occupancy, allocations that end the previous
  one (moves keep the history), the board, the monthly charge for a student and month,
  residents with fees outstanding, the register (open with consent checks, close).
  `fees::generate_vouchers` adds the hostel line under the hostel's fee head.
- API `routes/hostel.rs`: hostels, rooms, board with occupancy, residents, allocations,
  register. Reports `hostel.occupancy`, `residents`, `outstanding`.
- UI: Hostel module (Board with the allocate / move dialog, Residents, Register with
  leave-outs and visitors, Hostels with rooms and the fee head). Strings en/ur.
- Tests: core, `tests/hostel.rs` (Lite refusals; Standard: hostel and rooms, double booking
  and missing bed refused, room shrink and deletes refused, the board, a leave that frees a
  bed and a move that keeps the history, register with consent, already-out and
  non-resident refusals, close once, the voucher lines, reports, back to Lite), vitest for
  the board allocation, reader view and the register.

### 2026-11-08 — Stage 7: Inventory (branch `dev/std-stage-7-inventory`)
- `madrasa_core::inventory`: movement kinds and direction, the balance after a movement
  (never negative), recipients, reorder check, purchase cost (unit tests).
- Migration 0023: `inventory_categories`, `inventory_locations`, `inventory_items`,
  `inventory_movements`, `inventory_assets`; permissions `inventory.read` (principal,
  admin, accountant) and `inventory.manage` (principal, admin), granted to existing roles
  only on Standard installs. New `storekeeper` login role (Standard/Nexus only).
- `crates/db::inventory`: categories, locations, items with stock on hand and the reorder
  flag, stock per location, movements with the balance check under a row lock, transfers,
  removal of wrong entries, the period summary, assets.
- API `routes/inventory.rs`: categories, locations, items with detail, stock, movements,
  transfers, recipients (staff, students, rooms), assets. Reports `inventory.stock`,
  `reorder`, `movements`.
- UI: Inventory module (Items with the item dialog, Movements with the movement dialog,
  Stock, Assets, Setup). Store keeper role offered in the login dialogs on Standard.
  Strings en/ur.
- Tests: core, `tests/inventory.rs` (Lite refusals incl. the role; Standard: setup and
  duplicates, items, an issue before stock refused, purchase with cost, issue needing a
  recipient, the reorder flag, a transfer refused over the balance then done in two halves,
  return and write-off, item detail, removing a used purchase refused, deletes refused,
  assets with a duplicate tag and a status change, reports), vitest for the items list and
  reader view, recording an issue with the recipient search and the low-stock warning,
  adding a location.

### 2026-11-08 — Stage 8: Directory login (branch `dev/std-stage-8-directory`)
- `madrasa_core::directory` (settings validation, filter escaping, group-to-role mapping,
  the disabled flag) and `madrasa_core::secrets` (AES-256-GCM sealing); unit tests.
- Migration 0024: `users.auth_source`, `directory_dn`, `directory_synced_at`. Settings
  `directory.config`, sealed `directory.bind_password` and the global `instance.key`
  (created on first use), all redacted in the audit log (ADR 0004).
- `crates/db::directory` (settings, sealed password) and `users::upsert_directory_user`.
- API: `directory.rs` with the `DirectoryClient` trait, `LdapDirectory` (`ldap3`, TLS,
  system roots) and `StubDirectory`; `AppState::with_directory`; the login handler tries
  the directory for non-local usernames with lockout, disabled and unmapped handling;
  `GET /auth/options`; `routes/directory.rs` settings and test connection; `UserSummary`
  carries `auth_source`; 503 `directory_unavailable`.
- UI: Settings → Directory login card (fields, group mapping rows, test connection, hint),
  the login screen hint, a Directory badge in Users. Strings en/ur.
- Tests: core, `tests/directory.rs` against the stub (Lite refusals; settings validation
  incl. TLS, password-before-enable and unknown roles; test connection with a wrong and the
  right bind password and a user lookup; first login creates the account with the mapped
  role and permissions; second mapping order; wrong password, disabled, unmapped, unknown;
  lockout; local admin unaffected; default role; switched off; back to Lite), vitest for the
  card and the login hint.
- Not covered by automated tests: the LDAP wire protocol itself (`LdapDirectory`), which
  needs a real server; see the owner summary for the manual test.

### 2026-11-08 — Stage 9: Read-only API (branch `dev/std-stage-9-readonly-api`)
- `madrasa_core::api_keys`: scopes, usability, the IP allow-list with CIDR matching and
  validation, the rate window, key generation and prefix (unit tests).
- Migration 0025: `api_keys` and the append-only `api_key_calls`.
- `crates/db::api_keys` (create returning the secret once, list, revoke, log a call, the
  call log) and `crates/db::ro` (students paged, sections, attendance per day, fee
  balances, assessments, approved results, the published timetable of a section).
- API: `ro` module served at `/api/ro/v1` with one guard middleware (feature, key, expiry,
  allow-list from the connection's peer address, rate limit with `Retry-After`, ETag /
  `If-None-Match`, call log) and the endpoints above plus `/openapi.json`;
  `routes/api_keys.rs` for Settings; `AppState.ro_limits`; both servers now serve with
  connection info so the allow-list sees the caller.
- UI: Settings → API card (keys, new key dialog showing the secret once, revoke, call log,
  endpoint reference with `curl` examples). Strings en/ur.
- Tests: core, `tests/readonly_api.rs` (Lite has no prefix and refuses the settings;
  Standard: key validation, creation and the hidden secret, no key / wrong key / wrong
  address / good address with an ETag and 304, `X-Api-Key`, paging, sections, a missing
  scope, attendance, balances, timetable, index, OpenAPI, the call log, the rate limit,
  expiry, revocation, a key without an allow-list, results and loans, back to Lite), vitest
  for the card (create and show once, revoke, the reference).

### 2026-11-08 — Stage 10: Close-out (branch `dev/std-stage-10-closeout`)
- Demo data for every Standard module (`crates/demo-data/src/standard.rs`, loaded with the
  demo school and visible once the install is switched to Standard): a morning shift, six
  rooms, lessons seeded from the teacher allocations and a generated, published timetable
  (skipped when the small demo has no lessons or the engine cannot place every lesson in ten
  seconds); three leave types with balances for every teacher, six staff requests (approved,
  rejected, pending) and three approved student leaves; the default admission stages, the
  admission fee head and twelve applications spread along the pipeline; library rules,
  twenty titles with copies and fifteen loans (some overdue); a transport fee head, two
  vehicles, two routes with three stops each and students of six sections on them; a boys'
  hostel with a fee head, six rooms, fifteen residents and a leave-out; inventory categories,
  three locations, twelve items with purchases, issues to teachers and three tagged
  projectors; a read-only API key named *Demo portal*.
- `api_keys::create_with_secret` (demo data only). `LoadReport.standard` summarises it; the
  demo load test asserts the counts.
- TESTING.md section 5: tasks 27–38 for the Standard modules, directory login and the
  read-only API.
- Spec §15 lists every Standard section; the permissions matrix covers every permission and
  the two new roles (checked by `matrix_matches_docs`); the user guide has a chapter per
  module (9–17).

### 2026-11-09 — v0.3.1: fixes from the v0.3.0-alpha Standard test report (branch `dev/v0.3.1-standard-fixes`)
- Done, B-1 timetable: demo staff full names are unique (the generator re-draws on a clash);
  the engine's teacher check suggests a free short name when initials collide ("AH" → "AHa")
  and only warns, instead of refusing, when two teachers share a full name but their short
  names differ; the year setup gives every teacher a unique short name (initials, then more
  letters of the surname, then a number) so the generated timetable is never blocked by
  initials; the demo seeder records a plain-language note (and a warning in the log) when
  the timetable cannot be generated or published, the setup API returns the notes, and the
  wizard's last step lists them under "Some demo data could not be loaded". The full and the
  small demo school load with a published timetable.
- Done, B-2 library fines: settling a fine through a fee head adds a line to the student's
  voucher for that month (`fees::add_to_month_voucher`, get-or-create), the way transport and
  hostel charges do, instead of opening a second voucher.
- Done, B-3: TESTING.md title and file names say v0.3.1-alpha. B-4: chalk and printer toner
  are seeded below their reorder level, so the reorder flag and the reorder report have data.
  B-5: the role validation message for staff logins and the users API lists all six codes
  (principal, admin, accountant, teacher, librarian, storekeeper).
- Done, B-6: conflict messages no longer end in ".." (the API added a full stop to rule
  messages that already had one); hostel list routes check the edition before parsing the
  query, so a bad query on Lite is "feature unavailable", not a validation error; moving an
  application to Test scheduled / Interviewed / Offered stamps the test, interview or offer
  date when it is empty (a date already entered is kept); a hostel's kind is enforced on
  allocation (boys' hostels take boys, girls' hostels girls, mixed anyone) and the demo
  residents are boys in the boys' hostel; student search accepts `search` as well as `q`.
  The demo wizard says "fees and attendance up to this month".
- Tested: core unit tests (teacher name check with suggestion and warning, suggested short
  names, hostel gender rule), db unit test for unique teacher short names, demo generator
  test for unique names, demo load test asserts a published timetable with no notes (and a
  second load with three teachers for 39 sections asserts the visible note), API
  tests: library (fine joins an existing voucher; store keeper login through /users; one full
  stop), hostel (girl refused in a boys' hostel and vice versa; bad queries on Lite are 403),
  admissions (stamped and kept dates), staff logins (role message), students (`search`),
  demo setup (`notes` is empty); wizard test for the notes panel.
- Left: nothing from the report. Version 0.3.1.

## Nexus alpha

### 2026-11-11 — Stage 0: Design (branch `dev/nexus-0-design`)
- Done: `docs/adr/0005-nexus-hub-and-sync.md` (hub server in `crates/nexus`, one
  PostgreSQL schema per school, joining and leaving, the sync engine with field-level
  timestamps, head office winning same-field conflicts, receipts and attendance always
  merged, the overwrite log, network-wide reports, backup key escrow, Docker, Azure and
  Google Cloud packaging) and `docs/TASKS-NEXUS.md` (nine build stages).
- The spec describes Nexus in one line (§2), so the design fills the gaps from the request
  and from what the code already guarantees: all 84 tables have an `id` key and an `hlc`
  stamp, nothing is hard-deleted, and every update writes a fresh stamp. That lets change
  capture be one generic database trigger, attached only when a school joins.
- Tested: nothing to test; documents only.
- Left: stages 1 to 9.

### 2026-11-11 — Stage 1: Hub foundation and container (branch `dev/nexus-1-hub`)
- Done: `crates/nexus` (`madrasa-nexus` binary) with configuration from environment
  variables only, its own migrations in a `nexus` schema (`hub_users`, `hub_sessions`,
  `hub_settings`, append-only `hub_audit_log`), first-run setup behind a one-time token,
  login with lockout, logout, hub users with three permissions (`nexus.admin`,
  `nexus.reports`, `nexus.escrow`), the audit log endpoint, and `madrasa-nexus healthcheck`.
  `deploy/nexus/`: Dockerfile (cargo-chef dependency layer, non-root user, self health
  check), Compose file (hub, PostgreSQL 16, optional Caddy for HTTPS, loopback-only by
  default), `.env.example`. `scripts/nexus-smoke.sh`. CI job "Nexus hub image". Spec §16,
  `docs/nexus/deploy.md`, hub section in `docs/permissions.md`.
- Tested: 7 unit tests (configuration, secrets hidden from debug output, permission codes)
  and 6 database-backed tests (setup once and only with the token, generated token, login
  and lockout, logout, hash-only session storage, permission checks, the last
  administrator rule, audit rows without secrets, append-only log, hub tables in their own
  schema). The image was built on the development machine with the real build steps and the
  Compose stack was started there: both containers healthy, smoke test passed, process
  running as uid 10001. One step could not run on that machine (`apt-get install
  ca-certificates`, blocked by its network proxy); CI builds the unmodified Dockerfile.
- Left: stages 2 to 9. The image is not yet published anywhere (stage 8).

### 2026-11-11 — Stage 2: Sync engine, capture and field-level merge (branch `dev/nexus-2-sync`)
- Done: `madrasa_core::sync` (the table policy for all 84 tables, the wire format, the
  merge decisions and the per-record merge plan as pure functions); migration
  `0026_sync.sql` (five bookkeeping tables, the generic capture trigger, a number-safe row
  encoder); `madrasa_db::sync` (switch capture on and off, queue a full snapshot parents
  first, collect a batch, apply a batch with the merge plan, acknowledge, the overwrite log
  passed to the other side, parked records with reasons). Restore keeps the change counter
  consistent. Spec §16.3.
- How "head office wins" is decided: by whether this side has an unsent change to the same
  field, not by clocks. The stamps are kept per field for replays and for the log. A school
  PC with a wrong clock cannot win or lose a conflict because of it.
- One refinement of ADR 0005's wording: a replay is recognised by comparing a change's stamp
  with the latest stamp already received from the other side for that field (not with the
  local stamp), which is exact where the ADR's sentence was approximate. Spec §16.3 has
  the precise rule.
- One deviation from the schema rules, on purpose: the five `sync_*` tables have a UUID v7
  key and `school_id` but not `updated_at`, `updated_by`, `deleted_at` or `hlc`. Those
  columns exist for syncing, and the bookkeeping is never synced. The rules test says so.
- Tested: 16 unit tests in core (field rule on both sides, attendance rule, replays, equal
  values, excluded columns, table order, version check) and 10 database tests with two real
  databases as school and hub copy: the policy covers every table; an unjoined school has no
  triggers and queues nothing; a first upload reproduces every synced table exactly;
  password hashes never leave; different fields of one record both survive; a same-field
  conflict resolves to head office with the same log entry on both sides, in both
  directions; a replayed batch changes nothing, even after a later local edit; a child that
  arrives before its parent and a duplicate student number are parked with a reason and go
  in later; a record of another school is refused; leaving removes every trigger and keeps
  the data. A whole demo school (20,687 records) uploads in 42 batches in under four
  seconds on the development machine and matches table for table.
- Left: receipts and attendance merging (stage 3). Nothing calls the engine over the
  network yet (stage 4).

### 2026-11-11 — Stage 3: Receipts and attendance always merge (branch `dev/nexus-3-merge`)
- Done: `madrasa_db::sync::special` and migration `0027_sync_receipts.sql`. Receipts: union
  of both sides; a voucher's paid amount is the sum of its receipts on each side and its
  payment status follows; two receipts with one number are both kept (the later one gets a
  suffix); a receipt reversed on both sides is reversed once. Attendance: sheets, entries
  and staff records made separately for the same section, student or staff member and day
  fold into one; the later mark stands whichever side made it; the earlier mark is logged;
  sheet totals are counted again. Spec §16.4.
- Receipts stay immutable for users. The database trigger allows exactly two things, only
  while a received batch is being merged: changing a receipt's number and nothing else,
  and removing a duplicate reversal. A test shows every other change is still refused,
  even during a merge.
- Found while testing: number sequences are not synced, so a hub copy that issued receipts
  would reuse the school's numbers. The merge handles it (both kept, one renumbered), and
  the alpha console issues no receipts. If head office is ever to take payments it needs
  its own number series. Written down in the spec.
- Tested: 3 more unit tests in core and 9 database tests using the real fee and attendance
  repository functions on both copies, each scenario run with either side syncing first:
  receipts taken on both sides add up on the voucher and both are on both sides; receipts
  that together overpay are both kept; a receipt reversed on both sides is reversed once
  with the removed reversal in the log; receipts stay immutable outside a merge; two sheets
  for one day fold into one with one live mark per student and correct totals; different
  days simply combine; a school correction made after a head office correction stands;
  staff attendance folds the same way; a merged day keeps syncing normally. The earlier 10
  engine tests and the whole-demo-school upload still pass. Repeated four times to check
  for timing dependence.
- Left: nothing calls the engine over the network yet (stage 4).

### 2026-11-11 — Stage 4: Joining and leaving (branch `dev/nexus-4-join`)
- Done, hub: the school register and invitations (migration `0002_schools.sql`), one
  PostgreSQL schema per school created on join, transactions that can see exactly one
  school, the join and sync endpoints, removing a school, purging an archive, sending
  everything again, and bringing school schemas up to date when the hub starts.
- Done, school: feature `nexus_link`, the link as an audited setting, the sync token sealed
  with the instance key, an HTTPS client, `madrasa_db::sync::round` (one round: push, pull,
  acknowledge), `GET/POST /nexus/link…`, a background task every five minutes, and the
  check at start that change capture matches the link. Settings → School network (Nexus)
  in the desktop app, English and Urdu.
- Found and fixed while testing end to end: writing "last synced" as a setting was audited,
  the audit row was itself a record to send, and every sync left one record pending for the
  next. The last sync time and error now live in `sync_state` (migration
  `0028_sync_status.sql`), which is local bookkeeping.
- Lite gains one feature code, `nexus_link` (the choice to join); the test that Lite has
  exactly its nine core modules now says nine plus that. Nothing else changes for a school
  that does not join: a test asserts it has no triggers and queues nothing.
- Tested: hub tests (5): invitations work once, expire and can be revoked, and the code is
  stored only as a hash; a school joins, uploads and syncs both ways through the hub's
  endpoints; two installs with identical ids, codes and data never see each other, the
  hub's tables are invisible from inside a school's transaction, and purging one leaves the
  other complete; leaving and removal; sending everything again. End-to-end tests (3) with
  a school's API and a hub talking over real HTTP: join, first upload, edits both ways,
  head office winning a conflict with the log visible at the school, permissions, leaving
  with every record intact and joining again; removal told to the school; the hub going
  away (a status, nothing lost); a restart. Component tests (6) for the settings card. The
  whole workspace suite and the frontend suite (193 tests) pass.
- Not verified here: a school reaching a hub over real HTTPS on the internet. The tests use
  `http://127.0.0.1`, which the client allows for this computer only. The first real
  deployment is a hand test (TESTING.md, stage 9).
- Left: stages 5 to 9.

### 2026-11-11 — Stage 5: Network-wide reports (branch `dev/nexus-5-reports`)
- Done: five hub reports (network summary; enrolment by school and class; fees billed,
  collected and outstanding by school and month; attendance by school and month; sync
  health), each as JSON, CSV or Excel, in English or Urdu, for all joined schools or one,
  over a period of up to two years. Permission `nexus.reports`; every run audited.
  Spec §16.6.
- The enrolment and collection figures come from the same queries the school's own reports
  use, run in each school's schema. Money is never added across currencies: the fees
  report totals per currency.
- Tested: 6 unit tests (catalogue in both languages, periods, money and percentage
  formatting, CSV and Excel output, the text table complete in both languages) and one
  database test with two demo schools (one in PKR, one in USD) and a third school that
  left: every reported figure is compared with the school's own database; the school that
  left is in no figure but is listed in the sync report; one-school filter, Urdu headings,
  CSV and Excel downloads, refused periods and formats, permissions, audit entries.
- Left: stages 6 to 9. The reports have no screen yet (stage 7).

### 2026-11-11 — Stage 6: Backup key escrow (branch `dev/nexus-6-escrow`)
- Done: a joined school can leave a sealed copy of its backup key with head office
  (Settings → Nexus, with the backup password), see whether head office holds it and
  whether it is still the current key, and take it back. Head office sets escrow up with
  an escrow passphrase (at first-run setup or later), sees which schools have left a key,
  and releases one with the `nexus.escrow` permission, the passphrase and a reason. The
  released key is a printable code shown once. Restore, and setting a new backup password,
  accept it as a third way in. Spec §16.7.
- A release cannot be hidden: after its next sync the school writes it into its own audit
  log and Settings → Nexus shows who released the key, when and why.
- Safety: the hub stores only sealed keys and its own private key wrapped by the
  passphrase, so a copy of the hub database opens nothing. A deposit is bound to its
  school. Five wrong passphrases stop that user's releases for fifteen minutes. Neither the
  passphrase nor a key is ever logged or audited.
- Decision made here, stricter than ADR 0005: when a school leaves, is removed or is
  purged, its sealed key is erased at once, and only a joined school's key can be released
  (a school that is no longer joined could not be told about a release).
- Tested: 6 unit tests of the cryptography (wrong passphrase, tampering, another school's
  deposit, another hub's key, passphrase change, the printable code and typing mistakes);
  3 hub database tests (setup, permissions, deposit, release, audit without secrets, the
  school shown and confirming, another school kept out, a copied deposit refused, lockout,
  the sealed key erased on withdrawal, leaving, removal and purge); one end-to-end test
  over real HTTP (a school's API leaves its key, makes a real backup, head office
  releases, the school records the release at sync, restores the backup with the released
  key, sets a new password with it, takes the key back); 5 new screen tests.
- Not done, said plainly: the hub has no screen yet, so setting escrow up and releasing a
  key are API calls until stage 7. The app cannot replace a backup data key, so a released
  key keeps opening that school's backups; it must be kept like the recovery key.
- Left: stages 7 to 9.

### 2026-11-11 — Stage 7: Hub console and head office corrections (branch `dev/nexus-7-console`)
- Done: the hub serves its own web console (English and Urdu, right to left): first-run
  setup, sign-in, the school register with invitations, sync state, the overwrite log and
  held records, removing a school and deleting an archive, network-wide reports with Excel
  and CSV downloads, corrections, backup key escrow (set up, release, list of releases,
  change the passphrase), head office users and the activity log. It is a second build of
  the desktop app's components (`pnpm build:console`); the container image now ships it.
  Spec §16.8.
- Done: head office corrections. An administrator can correct names and contact details of
  a joined school's students, guardians and staff, and the school's name and city. The
  school receives the change at its next sync; a school edit of the same field loses and
  is logged on both sides; other fields are untouched. Both audit logs say head office made
  the change. Money, marks, attendance and accounts cannot be corrected from head office.
- Tested: 2 hub database tests (a correction reaches the school; head office wins a
  collision with the log row on both sides; different fields both stand; a phone emptied;
  the school's name; refusals for other fields, other kinds, other schools' records,
  schools that left, and users without the permission) and 2 unit tests; 1 hub test that
  the console's files are served with the security headers, that console addresses return
  the page and that the API is not shadowed; 14 screen tests of the console (setup,
  sign-in and sign-out, pages by permission, invitations, remove and delete with their
  confirmations, reports and downloads, corrections sending only what changed, escrow
  setup and release shown once, users, money shown exactly). The image's smoke test now
  loads the console.
- Tried by hand in a real browser against a local hub and a local demo school of 1,200
  students: setup, sign-in, invitation, the school joining and uploading, reports and an
  Excel download, a correction arriving at the school after its sync, escrow setup, Urdu
  layout, sign-out. No browser errors. Not tried by hand: releasing a key from the console
  (the demo school has no backup password; the release itself is covered by the stage 6
  tests).
- Left: stages 8 and 9.

### 2026-11-11 — Stage 8: Deployment templates (branch `dev/nexus-8-deploy`)
- Done: an Azure Bicep template (`deploy/nexus/azure`: Container Apps, PostgreSQL Flexible
  Server, Log Analytics) and a Google Cloud Terraform module (`deploy/nexus/gcp`: Cloud Run,
  Cloud SQL, Secret Manager, an Artifact Registry cache of the image), each with an example
  parameters file and a step-by-step guide in `docs/nexus/deploy.md`. The release workflow
  now builds the hub image and publishes it to GitHub Container Registry on a tagged
  release. Operations notes: backing up the hub database, upgrading (hub before schools),
  a leaked sync key, a leaked password, what to watch.
- Checked: CI builds the Bicep template and its example parameters (`az bicep build`,
  `az bicep build-params`) and validates the Terraform module (`terraform validate`) on
  every change.
- **Not done, said plainly: neither template has been deployed.** No Azure subscription or
  Google Cloud project was available, and the tools could not be run on the build machine,
  so the only check is CI's. The first real deployment is a manual test for the owner.
- **No image is published yet.** The first appears with the next tagged release, and GitHub
  will create the package as private: an owner must make it public before Azure or Google
  Cloud can pull it.
- Not built: replacing a school's sync key in place. The guide says what to do instead
  (remove the school and invite it again).
- Left: stage 9.

### 2026-11-11 — Stage 9: Close-out (branch `dev/nexus-9-closeout`)
- Done: `madrasa-nexus demo-network --yes` fills an empty hub with three generated schools
  (320, 220 and 140 students; one billing in another currency), each in its own schema, so
  the console, reports and corrections can be tried before any real school joins. It
  refuses a hub that already has schools. TESTING.md section 7 (tasks 39 to 48, and the
  cloud deployment nobody has done yet). README status. Spec §16 checked against the code
  (tables, endpoints, commands); permissions document checked.
- Tested: 1 database test of the demo network (three separate schools with no shared ids,
  no password hashes, reports over all three with totals per currency, a correction, the
  refusal to load twice, removal and purge) and the command run by hand against a scratch
  database (with and without `--yes`, and a second time).
- Fixed on the way: a Standard timetable test failed once in CI on a branch that changed no
  Rust code. The generator works against the clock, and the test assumed the first session
  of whatever arrangement came out had a free slot to move to. It now takes the first
  session that has one. No product code changed.
- Left: nothing in docs/TASKS-NEXUS.md. See the summary below for what is not proven.

### 2026-11-12 — v0.4.1: the release image build (branch `dev/v0.4.1-nexus-image`)
- What happened: the v0.4.0-alpha Release run built the desktop installers but failed in
  the Nexus hub image job. The version bump to 0.4.0 changed `Cargo.toml` and the package
  files but not `Cargo.lock`, which still named 0.3.1; the image is built with
  `cargo build --locked`, which refuses that. CI on `main` failed in its own image job on
  the same commit, for the same reason, but the tag had been pushed a minute after the
  bump, which had not gone through a pull request. The desktop builds and the Rust jobs do
  not use `--locked`, so they passed.
- Fixed: `Cargo.lock` is in step again (only this workspace's six entries changed) and the
  version is 0.4.1.
- So that it cannot recur quietly:
  - `scripts/set-version.sh` now updates `Cargo.lock` itself and stops with an error if it
    cannot; before, a failure to do so was swallowed.
  - `scripts/check-lockfile.sh` compares the two files and then asks cargo. CI runs it in
    the Rust job (a named step, before the long builds) and, in its quick form, at the top
    of the image job. The Release workflow has a new *Preflight* job that checks the tag
    against the version and runs the same check; the installers and the image wait for it.
  - CI's image job already ran on every pull request and push; it now also passes a release
    name into the build, as the Release workflow does, and the smoke test checks that the
    hub reports it.
- `docker/build-push-action` v6 → v7, `docker/login-action` v3 → v4,
  `docker/setup-buildx-action` v3 → v4: the versions that run on Node 24 (checked in each
  action's `action.yml`).
- Docs: `docs/RELEASING.md` (bump through a pull request, commit `Cargo.lock`, tag only a
  green commit), TESTING.md and the deployment examples name 0.4.1.
- Tested: the failure was reproduced locally (`cargo metadata --locked` refuses the old lock
  file with the same message as the Release log) and passes after the fix; the check
  script fails on the old state and passes on the new; the version script was run for
  0.4.1 and refuses `0.4.1-alpha`. The image build itself and the new workflow steps are
  proven by CI on this pull request. **The Release workflow's own changes (Preflight, the
  new action versions in the publishing job) run only on a tag or a manual run, so they are
  not exercised by this pull request.**

## Parent app alpha

### 2026-11-12 — Stage 0: Design (branch `dev/parent-0-tasks`)
- Done: `docs/adr/0006-parent-app.md` and `docs/TASKS-PARENT-APP.md` (eight stages). The
  app talks only to the Nexus hub; parent accounts live at the hub and are opened by the
  school with one-time access codes; a parent sees only their own children; only approved
  results are shown; notices and messages are school records that sync; messages are
  add-only and not instant.
- Tested: documents only.

### 2026-11-12 — Stage 1: School records, notices and messages (branch `dev/parent-1-records`)
- Done: school migration 29 (`notices`, `messages`, `message_reads`), their sync rules,
  four permissions (`parents.manage`, `notices.manage`, `messages.reply`,
  `messages.view_all`) granted to the built-in roles, and the school API for notices and
  conversations. A conversation is with the class teacher of the child's section; the
  principal reads all and answers none; a teacher cannot open another teacher's class.
  Everything is refused, with a plain message, in a school that has not joined a hub.
  Spec §17.1.
- Changed from the task list: no new feature flag. The parent features hang off
  `nexus_link`, which every edition already has, and work only while the school is joined,
  so the feature lists of Lite and Standard are exactly as before.
- A trap avoided: the migration grants the new permissions to existing roles, and it runs
  both on a school and on the hub's copy of that school. Two different records for the
  same grant could never both be stored and would have waited at every sync for ever. The
  grants now get an id computed from the role and the permission and are not queued for
  sending, so both copies hold the same records. A test simulates the upgrade on both
  copies and syncs them.
- Tested: 5 unit tests (message and notice limits, when a notice is shown, previews); 3
  database tests (messages from both sides all kept and read marks not colliding; a notice
  with its classes reaching the hub and a withdrawal following; the migration's grants
  identical on both copies); 1 school API test (roles, the class-teacher rule, a class
  changing teacher, the not-joined refusal, validation, audit entries without message
  text, records queued for the hub).
- Left: stages 2 to 7. No screen yet (stage 4), no parent accounts yet (stage 2).

### 2026-11-12 — Stage 2: Parent accounts and access codes (branch `dev/parent-2-accounts`)
- Done: parent accounts at the hub (phone number and password), one-time access codes that
  a school asks for and hands to a guardian, and the links from an account to guardian
  records. A parent can check whose code it is, activate it, sign in and out and change the
  password. The school can see who has access and end it. Spec §17.2.
- One code, three outcomes, by the phone number the parent gives: a new account; a school
  added to an existing account (its password is needed); or a new password, when the
  guardian already has that account. The last is how a forgotten password is handled: the
  school issues a new code. There is no SMS or email.
- Safety: codes, passwords and tokens are stored as hashes only; five wrong passwords lock
  an account for fifteen minutes; a parent session opens nothing in the head office console
  or the school sync API and the reverse; the logs carry no code, password, token or phone
  number; the school sees only the last four digits of the phone number.
- Tested: 2 more unit tests (access code format and forgiving input, phone numbers); 2 hub
  database tests (everything above, including two schools on one account, a replaced
  phone, an expired code, a school leaving, and lockout with the school lifting it by a new
  code); 1 end-to-end test over real HTTP from the school's API through a hub.
- Left: stages 3 to 7. A signed-in parent cannot see anything yet (stage 3).

### 2026-11-12 — Stage 3: What a parent can see at the hub (branch `dev/parent-3-data`)
- Done, one calculation for school and hub: the result sheet and report card code moved
  from the school API (`crates/api/src/routes/exams.rs`, `exam_terms.rs`) into
  `madrasa_db::results`. The school API re-exports it; its answers are unchanged (the
  existing exam, report card, PDF, Excel, read-only API and allocation tests pass as they
  were). New `build_approved_report_card`: the same calculation over approved sheets only,
  with a position in class only when every sheet of the assessment is approved. New
  `DbError::Invalid` (a 400 in both servers) for the two formula errors that used to be
  raised in the API layer.
- Done, hub (`crates/nexus/src/routes/parent_data.rs`): `/parent/children` and, for one
  child, overview, attendance by month with month and year summaries, results (the
  approved-only report card), fees (vouchers with lines, receipts, balance), notices for
  the child's class, and the conversation with the class teacher (read, send, mark read).
  Every answer carries the school's last sync time. Spec §17.3.
- Done, scoping: one function (`open`) stands in front of every child route. It checks the
  account's live link to a guardian record of that school, then, in a transaction that sees
  only that school's schema, that the guardian record belongs to the student. Any failure
  is the same `404`.
- Done, messages from the app: written into the hub's copy of the school from the
  guardian's side (audit entry without the text), limited to 20 an hour for one child,
  refused when the class has no class teacher. They reach the school at its next sync.
- Done, cross-origin access for the app on `/parent/…` only: any origin, `Authorization`
  and `Content-Type` headers, no credentials.
- Decision: a voucher carried into a later one shows balance 0 in the app (the school's
  ledger screen shows the carried amount on both); the total owed is the same.
- Decision: the teacher's note on an attendance day, who received a payment, payment
  references and school login ids are not sent to the app.
- Tested: `crates/nexus/tests/parent_data.rs` (2 tests): a parent's own child answers on
  every route; another family's child, a child at another school, own child named under
  another school, unknown ids, a guardian removed from the student, access ended by the
  school and a school that left all give the identical `404` on all nine routes; a refused
  message is not stored; head office, school and made-up tokens get `401`; cross-origin
  headers are present on parent routes and absent on console and sync routes; notices
  respect draft, expiry and class; a message goes app → hub → school and the answer comes
  back; read marks; the hourly limit. `crates/api/tests/nexus_link.rs` (+1, now 6): a real
  school API and a live hub; attendance (month, year, days), fees (balance, every voucher
  figure, receipts) and the overview are compared with the school's own API answers; an
  unapproved Mathematics sheet is not shown and does not count, and after approval and a
  sync the parent's report card equals the school's field for field; a teacher's reply
  through the school API reaches the app. Hub unit test for month bounds.
- Left: stages 4 to 7 (school desktop screens, the app itself, the APK in CI, demo data
  and the hand-test script).

### 2026-11-12 — Stage 4: The school's screens (branch `dev/parent-4-desktop`)
- Done: a **Parent app** area in the desktop app with three tabs: Messages (class teacher
  answers own sections; the principal reads all, read-only), Notices (draft, publish for
  the whole school or chosen classes, last day, withdraw, delete) and Access (who has
  access, give access with a one-time code on a printable slip, new code, end access).
  Spec §17.4.
- Done: the sidebar entry appears only while the school is joined to a school network and
  only for people with one of the four parent app permissions, with the unread count
  beside it. New `GET /parent-app/status` answers that question in one call; a school that
  has not joined gets `joined: false`, never an error. Lite and Standard schools that have
  not joined see no change.
- Done: English and Urdu strings (`pa.*`, 81 keys each).
- Left as it was: the "Parents" sidebar placeholder of the Nexus edition (feature
  `parents_portal`, "Coming soon"). It predates this work and is a decision for the owner
  (see the summary at the end of the parent app work).
- Tested: 14 screen tests in `parentapp.test.tsx` (not joined; tabs follow permissions;
  class teacher reads, is marked read once and answers; principal read-only and never
  marks read; empty state; a notice for chosen classes with the missing-class check;
  withdraw; notice states; the access code slip, shown once and forgotten on close;
  ending access asks first; Urdu; the sidebar entry with its count, hidden when not
  joined, not asked about for people without the permissions), 1 more navigation test,
  and the status endpoint in `crates/api/tests/parent_app.rs`. ESLint, Prettier and tsc
  clean.
- Not done here: looking at the screens in a running app. That is part of the stage 5
  walkthrough against a local hub.
- Left: stages 5 to 7.

### 2026-11-12 — Stage 5: The parent app (branch `dev/parent-5-app`)
- Done: `apps/parent`, a new package in the pnpm workspace. Front end in `src/` (Vite,
  React, TypeScript) built from the desktop app's components, tokens and translations
  through the `@/…` alias; 135 `parent.*` strings in English and Urdu. Screens: welcome,
  access code (also adds a school to an account and sets a new password), sign in, my
  children, and for a child Home, Attendance, Results, Fees, Notices, Messages. Spec §17.5.
- Done: the Tauri 2 mobile shell `apps/parent/src-tauri` (crate `madrasa-parent`, in the
  Cargo workspace). It has no commands and stores nothing; the page may connect to
  `https:` hubs (and to this device for development) only. The Android project itself is
  generated by `tauri android init` and is not in the repository; building the APK is
  stage 6.
- Done, wiring: `Cargo.lock` and `pnpm-lock.yaml` include the new crate and package;
  `scripts/set-version.sh` sets the version in the parent app's `package.json` and
  `tauri.conf.json` too; the hub image's console stage copies the new `package.json` so
  the frozen lock file still matches; CI builds the parent front end (`pnpm build:parent`)
  and `pnpm -r` runs its lint, format, type and test steps.
- Tested: 37 tests in `apps/parent` (hub address rules; the client: token in the header,
  no cookies, the child named in the path, refusals, a lost session, offline; helpers;
  every screen, the empty and error states, sign-out, a second school, Urdu). ESLint,
  Prettier, tsc and the production build are clean. `cargo clippy --workspace` passes
  with the new crate.
- Tried by hand, in a phone-sized browser (390×844, 360×740 and 320×640) against a local
  hub: (1) the demo network, with a family of five children: every screen in English and
  Urdu, no console errors, no failed requests, no sideways scrolling; (2) a real school
  server with the small demo school, joined to that hub: an access code given on the
  desktop Parent app screen (slip checked on screen and in print layout), activated in the
  phone app; a notice published on the desktop appeared on the phone; the parent's message
  reached the class teacher after a sync (sidebar count 1), the teacher answered from the
  desktop, the principal could read but not answer, and the answer appeared on the phone.
  This also covers the look at the stage 4 screens that was left open there.
- Found and fixed while trying it: class names such as `4-A` were reordered inside Urdu
  text (now isolated left to right); Nastaliq lines were cramped at small sizes (taller
  lines in Urdu); "1 unpaid vouchers" (singular added).
- Not done: the app has not been run on an Android phone or emulator. This machine has no
  Android SDK; stage 6 builds the APK in CI. Until someone installs that APK, "works on
  Android" is untested (see the summary at the end of the parent app work).
- Left: stages 6 and 7.

### 2026-11-12 — Stage 6: The Android package (branch `dev/parent-6-android`)
- Done: `scripts/build-parent-apk.sh` (generates the Android project, applies Madrasa's
  launcher icon, builds one APK for 64-bit and 32-bit ARM) and `scripts/sign-apk.sh`
  (aligns, signs, verifies, writes `SIGNING.txt`). Spec §17.6.
- Done, CI: a new job *Parent app (Android APK)* on every pull request and push runs both
  scripts, signs with a throwaway test key and keeps the package as the artifact
  `madrasa-parent-apk`. A further step runs the signing script's release-key path with a
  keystore made on the spot.
- Done, Release workflow: the same two scripts; signs with the release key when the four
  `ANDROID_*` secrets are set, otherwise with a throwaway key (the file is then named
  `…-testkey.apk` and `SIGNING.txt` and a notice say so); attaches the package and its
  `SIGNING.txt` to the draft release after the desktop builds have created it.
- Done: `docs/RELEASING.md` has a section on the parent app: why the signing key matters,
  how to make it, the four secrets, how to check a release, and what is not done (Google
  Play, iOS). The version unit test covers the parent app's version files.
- Checked in CI (run 43278875924, on this branch before the pull request): the Android
  project was generated, the launcher icons were made from Madrasa's icon, the Rust code
  was cross-compiled for both ARM targets, Gradle built a 13.9 MB universal APK, it was
  signed with a test key and verified, and the release-key path produced a verified
  package with the stand-in key. The job passed at the first attempt; the hub image (whose
  build now has to cope with a new workspace member and package) and the front-end job
  passed too.
- **Not checked, and it cannot be from here:** nobody has installed the package on a phone
  or an emulator. The build machine has no Android SDK and no device; CI builds and signs
  but does not start the app. So these are still unknown: whether it installs and opens,
  the keyboard over the forms, the back button, Urdu rendering in the phone's WebView, and
  whether a phone reaches a hub over HTTPS (Android refuses plain `http://`, so a hub on a
  LAN address will not work with the package).
- **Not checked:** signing with a real release key (no secrets exist yet; the path was run
  with a stand-in key), and attaching to a release (no tag was pushed; the attach job runs
  only on a tag).
- Left: stage 7.

### 2026-11-12 — Stage 7: Close-out (branch `dev/parent-7-closeout`)
- Done, demo data: the demo school has three notices (whole school, one class, a draft)
  and one conversation of three messages between the guardian with the most children and
  the class teacher of one of them (`crates/demo-data/src/parent_app.rs`). The hub's demo
  network has them too, and `madrasa-nexus demo-network --yes` now ends by printing a
  parent app access code for one made-up guardian of each demo school;
  `madrasa-nexus demo-parent-codes` prints fresh ones. Spec §17.7.
- Done: TESTING.md section 8, "The parent app" (tasks 49 to 59, how to open the app in a
  browser against a local hub, what a phone needs, and what we have not tested); the user
  guide has a Parent app section in English and Urdu; README mentions the app and its
  folder.
- Checked: `docs/permissions.md` lists the four parent app permissions and matches the
  code (unit test `matrix_matches_docs`); spec §17.1 to §17.7 cover the tables, both APIs,
  the screens, the Android package and the demo data.
- Tested: the demo loader test checks the notices and the conversation (who wrote, to the
  class teacher of the guardian's own child); the demo school of 20,000 records still
  uploads completely and exactly; the hub's demo network test activates a printed code and
  reads every section for each child, finds the conversation and the notices but not the
  draft, and checks that a code works once and that new codes retire unused old ones.
  `cargo clippy --workspace` and the whole `cargo test --workspace` pass locally, as do
  both front ends' lint, format, type and test steps.
- Left: nothing in `docs/TASKS-PARENT-APP.md`. What needs a person is in the summary below.

## v0.4.3-alpha

### 2026-11-13 — Fixes after v0.4.2-alpha (branch `claude/kind-franklin-ac2vjh`)

1. **Restoring an older backup while joined to a hub (serious).** Cause: a backup holds the
   hub link and the record of what was already sent, so after a restore the school believed
   everything was sent while head office still held every record made after the backup, and
   the restored student-number counter issued those numbers again, so new records were
   parked at the hub for ever. Fix (ADR 0007, spec §16.9):
   - The restore asks for a confirmation while joined, with the warning *This school is
     joined to a network. Restoring an older backup will make your data differ from head
     office's copy. Madrasa will ask head office to reset its copy.* (`409
     restore_while_joined` without it). Not joined: no warning, nothing to confirm. A backup
     of another school is refused while joined.
   - The restore keeps the hub link and key as they were just before it, clears the sync
     queues, queues every record and marks the link restored, all in the restore's own
     transaction.
   - The next sync tells the hub first (`POST /sync/restored`) and sends nothing until it
     answers. The hub renames the school's copy to an archive (`a_<id>`, hub migration 5,
     `hub_school_archives`), starts a fresh copy from the school record and logs
     `school_restored`; the school logs the answer. The school then sends everything again.
     Purging the school drops its archives.
   - The Nexus card counts records parked at head office and explains them, and says *In
     step* only when nothing waits to be sent, nothing is parked on either side, the last
     sync worked and no restore waits to be told.
   - User guide (EN, UR): restoring while joined, and leave and join again as the fallback.
2. **Hub image docs.** The image is published with every release (v0.4.1-alpha and
   v0.4.2-alpha were): `docs/nexus/deploy.md`, TESTING.md, the Compose example and the cloud
   examples now pull `ghcr.io/quietbytehq/madrasa-nexus:<version>-alpha` or `:latest`.
3. **Backup key card** reloads after each sync (a release by head office is learned at a
   sync); the Nexus card re-reads its status every minute while joined, so automatic syncs
   show too. Deposit and withdraw already showed the hub's answer at once.
4. **Directory login certificates.** Test connection tells an untrusted, expired, not yet
   valid or wrongly named certificate apart from an unreachable server, with the TLS reason
   underneath. The screen and the user guide say the Madrasa server PC must trust the
   directory's CA (the LDAP client reads that PC's certificate store when Madrasa starts).
5. **Create login when a login exists.** The API names the existing login (`409
   login_exists` with its username and role) and changes nothing; the staff screen shows it
   and asks "Change this login's role?". Changing a role from the staff screen asks too.
6. **The .deb.** The released 0.4.2 package already declared `libwebkit2gtk-4.1-0` and
   `libgtk-3-0`; the failure seen is `dpkg -i`, which does not fetch dependencies (or a system
   too old to have WebKitGTK 4.1). The package now also declares libsoup 3, JavaScriptCore 4.1
   and `libc6 (>= 2.35)` (everything the binary links; the bundled PostgreSQL carries its own
   libraries and needs only libc). CI installs the built package with apt in clean Debian 12,
   Ubuntu 22.04 and Ubuntu 24.04 containers and fails if any library is missing. README and
   TESTING.md say `sudo apt install ./….deb` (and `sudo apt -f install` after `dpkg -i`).
7. **README**: status, download table, current editions, Nexus, the parent app.
8. **CI**: `upload-artifact@v6`, `download-artifact@v7`, `setup-terraform@v4` (Node 24);
   `ubuntu-latest` pinned to `ubuntu-24.04` (desktop builds stay on 22.04 for the glibc floor).
9. **Version 0.4.3** with `scripts/set-version.sh`; TESTING.md tasks 60 to 65.

- Tested: new end-to-end test (school API and a live hub): join, sync, back up, add two
  students, sync, restore the backup (refused without the confirmation), sync → every synced
  table holds the same number of the school's rows on both sides, nothing parked, the old
  copy is in the archive, both logs have it, and two new students with the same numbers as
  the lost ones reach the hub. A school that is not joined restores with no warning. Hub test
  for the re-baseline (only the school itself, with its own record; fresh copy; archive
  kept; other schools untouched; purge drops the archive). Unit tests for the certificate
  messages. API test for `login_exists`. Screen tests: restore warning only when joined;
  card says in step only when nothing is parked; restore notice; the key card reloads after a
  sync; certificate message and details; role change asks first. The existing escrow restore
  test now confirms the network.
- Not tested by me: the clean-container `.deb` install runs only in CI (no container engine
  here); a real directory with an untrusted or expired certificate (the messages are matched
  on the TLS library's texts, which are unit-tested, not on a live server); the restore on a
  real Windows or Linux install by a person (TESTING.md task 60).
- Decision taken: after a restore while joined, the school's data wins; what head office
  changed after the backup (corrections, parents' messages not yet received) leaves the live
  copy and stays in the archive. Rows that belong to no school (instance-wide settings in the
  audit log) are never sent to a hub, by design, and the test counts the school's rows.

## Brand identity

The brand sheet (`assets/brand/Madrasa_Brand_Identity_Showcase.png`) applied across the
product, without layout or feature changes (ADR 0008, spec §8.1).

- **Source files.** `assets/brand/tools/build_svg.py` draws the emblem and outlines the
  wordmark: `madrasa-icon.svg` (the app icon, source of every icon size), `madrasa-mark.svg`
  (the emblem), `madrasa-logo(-dark).svg` and `madrasa-logo-vertical(-dark).svg`.
- **Icons.** `assets/brand/generate-icons.sh` writes the desktop icon sets of both apps
  (Windows `.ico`, Linux PNGs, `.icns`, Store logos) and the parent app's Android launcher
  icons (legacy, round, adaptive; every density) from `madrasa-icon.svg`.
  `scripts/build-parent-apk.sh` copies the Android ones into the generated project.
- **Tokens.** Navy `#0B2557`, blue `#0757BA`, a new `gold` (`#F5B13B`), re-tinted selection,
  text and border; a dark palette in the brand's night blue. `font-display` (Merriweather
  Black, bundled) for the product name. One set for the desktop app, the parent app and the
  Nexus console.
- **Logo in the product.** The emblem replaces the "M" square at the same size in the sidebar
  header, desktop login and first-run screens, the console's header, login and setup, and the
  parent app. The startup screen shows the vertical logo (static HTML first, then the app's
  own while it starts). Vouchers, report cards and offer letters (server PDFs) and the voucher,
  receipt and report card print views carry the emblem. README has the logo as its header.

**Tested:** `cargo fmt`, clippy (`-D warnings`) on the API, the PDF unit tests (the emblem is
embedded; no other file can be read by the typesetter) and the API tests that produce PDFs
(fees, exams, admissions, alpha2, reports, international, polish); desktop 229 and parent 37
vitest tests (two new for the brand components), lint, format and type checks, the desktop,
console and parent builds. Every main screen of the three apps photographed before and after
with Playwright in light and dark against the demo school and demo network (in the PR).

**Not tested here:** the Windows installer and the APK on a phone with the new icons (CI builds
both); how launchers other than a circle mask crop the adaptive icon.

## Working offline

Goal: no school loses work because the network or the school server dropped (ADR 0009,
spec §18, user guide §19 in English and Urdu, TESTING.md tasks 66 to 72).

- **LAN client outbox** (`crates/api/src/outbox.rs`, in the client's loopback proxy): screens
  keep showing their last copy while the server is away; changes are queued (AES-256-GCM on
  disk) and sent in order on reconnect; "X waiting" in the top bar; changes that need the
  server there and then (sign-in, backup, imports, settings) are refused plainly.
- **Never twice**: an `Idempotency-Key` on every change from the first attempt, honoured by a
  new server middleware (`idempotency_keys`, migration 0030); repeats get the first answer.
- **Provisional receipts**: `P-<PC>-<n>` printed at once, kept on the real receipt
  (`fee_receipts.provisional_no`), real number shown afterwards; overpayments flagged.
- **Conflicts** by the sync rules: field-level merge (server's value wins a clash, yours
  listed), attendance later-mark-stands on the server (a teacher's queued sheet never
  overrides saved marks), receipts both stand. All shown in the badge's panel.
- **Connection badge** on every screen (desktop and parent app): Connected / Reconnecting /
  Offline, waiting count, things to read, head office sync when joined.
- **Encrypted drafts** for every form (Web Crypto AES-GCM, non-extractable key in
  IndexedDB), offered back after a crash or restart; never passwords.

**Tested:** `crates/api/tests/offline.rs` against the real API over TLS and the real proxy:
unplug the server mid-entry, queue a new student (Save pressed twice), an edit, two payments
(one pressed twice) and an attendance sheet while another PC changes the same records;
restart the client still offline (queue and cache intact and unreadable on disk); reconnect
and check exact counts (3 students, 3 receipts with 2 provisional, 1 sheet), no duplicates,
the field merge, attendance merge, overpaid notice, the replay of a key, a reused key
refused. Also: a teacher's queued sheet never overrides saved marks; a change queued under an
expired session waits for the same person, not another. Unit tests for the merge, sealing
and restart of the outbox, the idempotency hash, queued-time parsing, overwritten headers,
the outbox folder name. UI: 9 new desktop tests (badge states, panel lists, Urdu labels,
queued error and toast, provisional receipt print before and after, drafts encrypted,
restored after a restart, never passwords, cleared on save) and 2 parent app tests. Full Rust
workspace, lint, format, type checks and builds.

**Not tested here:** a real cable pulled on a real school network (tests stop and start the
server process on one machine); WebView2 and WebKitGTK storage of the drafts key (tests use
an in-memory key; where IndexedDB or Web Crypto is missing, drafts are not kept at all).

## Summary for the owner — Parent app alpha

The parent app alpha was built as one design pull request and seven stage pull requests,
each started from the latest `main` and merged only after CI was green on Linux and
Windows: #53 the design and task list, #54 notices and messages as school records, #55
parent accounts and access codes at the hub, #56 what a parent can see at the hub, #57
the school's screens, #58 the app, #59 the Android package, and this close-out.

**Lite and Standard keep working unchanged.** A school that has not joined a hub gets
three empty tables and four permissions from a migration, and nothing else: no sidebar
entry, no background task, no network call. Every existing Lite, Standard and Nexus test
still passes, and the school's report card answers are byte-for-byte what they were (the
calculation moved to a shared crate so the hub can run the same code).

### What was built

| Asked for | Built | Where |
| --- | --- | --- |
| A parent app for Android in `apps/parent`, Tauri 2 mobile | `apps/parent`: React front end and a Tauri 2 mobile shell; the Android project is generated at build time | stage 5, spec §17.5 |
| The same design system as the desktop app | It imports the desktop app's components, tokens and fonts directly (`@/…`); nothing was copied | stage 5 |
| English and Urdu | 135 `parent.*` strings in each; right-to-left layout; class names, codes and amounts kept left to right | stage 5 |
| Parents log in | Accounts at the hub: phone number and password, opened with a one-time access code the school prints on a slip. One account can hold children at several schools | stage 2, spec §17.2 |
| Pick a child | "My children" lists every child across the schools that gave access | stages 3 and 5 |
| Attendance | Month by month with counts and percentage, and the year so far | stages 3 and 5 |
| Results and report cards | The school's own report card calculation, from **approved** marks sheets only, on screen | stage 3, spec §17.3 |
| Fee status and vouchers | Amount due, each voucher with its lines, payments received. Read-only | stages 3 and 5 |
| School notices | Written at the school for the whole school or chosen classes; draft, publish, withdraw | stages 1, 4, 5 |
| One-to-one messaging with the class teacher | A conversation per child and guardian; only that section's class teacher answers | stages 1, 3, 4, 5 |
| Principal can view threads | `messages.view_all`: reads every conversation, answers none | stages 1 and 4 |
| Connects to the Nexus hub; hub endpoints in `crates/nexus` | `/parent/…` routes: accounts, children, and six sections per child | stages 2 and 3 |
| A parent only ever sees their own children | One check in front of every route, in the school's own schema; anything else is the same `404` | stage 3 |
| Android APK built in CI | Job *Parent app (Android APK)* on every pull request; artifact `madrasa-parent-apk` | stage 6, spec §17.6 |
| APK attached to releases | The Release workflow builds, signs and attaches it to the draft release | stage 6 |
| (not asked, needed to use it) | The school's "Parent app" screens (access codes with a printable slip, notices, messages); demo data and demo access codes; tester and user guides | stages 4 and 7 |

Automated tests added: 7 unit tests of the rules in `madrasa_core`; 3 database tests of
the sync of messages, notices and permission grants; 4 database tests of the hub (accounts
and codes; the seven kinds of "not yours" on all nine child routes; messages both ways); 1
test of the school's own routes and 2 end-to-end tests of a real school API against a live
hub (one compares every figure a parent sees with the school's own answers); additions to
the demo data and demo network tests; 15 tests of the school's screens and 37 tests of the
app. CI builds the Android package and signs it on every pull request.

### What to test by hand

Follow TESTING.md section 8 (tasks 49 to 59). What the automated tests cannot prove:

1. **The app on a real Android phone. This has never been done.** The package is built
   and signed by CI, and the same front end was walked through in a phone-sized browser,
   but nobody has installed the APK. Please install it and report from the first screen
   on: does it install and open, does the keyboard cover the fields, does the back button
   behave, does Urdu look right in the phone's own text rendering.
2. **A phone needs the hub on `https://`.** Android and the app both refuse plain
   `http://` except to the phone itself, so a hub on a PC's LAN address cannot be used
   from a phone. The hub's HTTPS profile (Caddy with a real domain) has still not been
   run by us (see the Nexus summary below). Until it has, the browser on the hub PC is the
   only way we have tried the app.
3. **The Urdu wording**, by someone who reads Urdu natively: the app's strings and the
   school's Parent app screens were written by the developer and checked only for layout.
4. **Printing the access slip** on a real printer (it was checked in print layout only).
5. **Real records.** Everything was tried with made-up schools. With a real class: do the
   figures in the app match the printed report card and the fee voucher exactly?

### Decisions left to you

1. **The signing key, before any parent gets the app.** No key exists yet, so every build
   is signed with a throwaway test key (the file name says `-testkey`). Such a build cannot
   be updated: the next one needs the old app removed first. `docs/RELEASING.md` ("The
   parent app (Android)") gives the one command to make a key and the four secrets to add.
   Keep the key safe: losing it means parents can never update.
2. **When to release.** Nothing was tagged and the version was not changed (0.4.1). The
   next tag will build and attach the APK. The attach step has never run, because it only
   runs on a tag; watch it the first time.
3. **Position in class.** A parent sees the child's position in class for an assessment
   once every marks sheet of it is approved, because the school's report card has it. Some
   schools do not tell parents positions. There is no setting for this yet: say if it
   should be hidden, or made a school setting.
4. **Forgotten passwords** are handled by the school giving a new access code; there is no
   SMS or email. Acceptable for the alpha, or is self-service needed?
5. **Messages are not instant** and there are no notifications: a message travels with the
   school's sync (within seconds while the school is online, otherwise at its next
   connection), and the parent sees an answer when they open the app. Push notifications
   would need Google's messaging service and are not built.
6. **Who may give access**: administrators and the principal (`parents.manage`). Class
   teachers cannot. Change the role grants if teachers should hand out slips.
7. **One guardian record, one code.** A code opens the children linked to that guardian
   record. Where the school entered the same parent as separate guardian records for each
   child, each record needs its own code (the parent can add them all to one account).
   Merging duplicate guardians is not part of this work.
8. **What the app leaves out on purpose**: teachers' notes on attendance, who received a
   payment and payment references, anything of another family. Fees show discounts and
   concessions as on the voucher. Say if any of this should change.
9. **Limits**: a message is at most 2,000 characters; a parent may send 20 an hour about
   one child; an access code lasts 14 days; five wrong passwords lock an account for 15
   minutes. All are constants, easy to change.
10. **The old "Parents" entry.** The Nexus edition's sidebar still has a "Parents — Coming
    soon" placeholder from the first milestone (feature `parents_portal`). The Parent app
    area replaces its purpose. It was left untouched because it is outside this work:
    remove it, or tell me what it was meant to become.
11. **Not built**: an iPhone version, a Google Play listing, payments in the app, report
    cards as PDF in the app, attachments in messages, changing the password inside the app.
12. `CLAUDE.md` still lists only `apps/desktop` under the folder layout; I did not edit
    your instructions file. `README.md` has the new folders.

## Summary for the owner — Nexus alpha

The Nexus alpha was built as one design pull request and nine stage pull requests, each
from a clean `main` and merged only after CI was green on Linux and Windows (#42 the design,
#43 hub and container, #44 sync engine, #45 receipts and attendance, #46 joining and
leaving, #47 reports, #48 key escrow, #49 console and corrections, #50 deployment
templates, then this close-out). A school that has not joined a hub is unchanged: no extra
triggers, no background task, no network call, and every Lite and Standard test still
passes.

### What was built

| Asked for | Built | Where |
| --- | --- | --- |
| Hub server in `crates/nexus` | `madrasa-nexus`: its own API (`/api/nexus/v1`), users, permissions, audit log | stage 1, spec §16.1–16.2 |
| Docker image and Compose file | `deploy/nexus/Dockerfile` (hub and console, non-root, health check), `docker-compose.yml` (hub, PostgreSQL 16, optional HTTPS) | stages 1 and 7 |
| Azure and Google Cloud templates | `deploy/nexus/azure` (Bicep), `deploy/nexus/gcp` (Terraform), guide | stage 8 |
| Schools joining and leaving, data kept separate | One PostgreSQL schema per school; one-time join codes; leave, remove, archive, purge | stage 4, spec §16.5 |
| Sync engine: field-level timestamps | Every field carries its own stamp; only changed fields are sent | stage 2, spec §16.3 |
| Head office wins on the same field | Yes, and the losing value is logged on both sides | stages 2 and 7 |
| Receipts and attendance always merged | Receipts from both sides are all kept; attendance marks for the same day are folded, later mark standing | stage 3, spec §16.4 |
| Overwritten values logged | The overwrite log, shown in the school (Settings → School network) and in the console | stages 2, 4, 7 |
| Network-wide reports | Five reports, JSON, Excel and CSV, English and Urdu, totals per currency | stage 5, spec §16.6 |
| Backup key escrow | Sealed deposit, release with passphrase and reason, the school always told | stage 6, spec §16.7 |
| (not asked, needed to use it) | Web console for head office; corrections to names and contact details; demo network | stages 7 and 9, spec §16.8 |

Automated tests added for Nexus: 20 unit tests of the sync rules and 6 of the escrow
cryptography in `madrasa_core`; 20 database tests of the sync engine (including a demo
school of 20,687 records uploaded and compared); 17 unit and 19 database tests of the hub;
4 end-to-end tests of a school's API against a real hub over HTTP; 23 screen tests (the
school's Nexus card and the console). CI also builds the image, starts the Compose stack
and runs a smoke test that loads the console, sets the hub up and signs in.

### What to test by hand

Follow TESTING.md section 7 (tasks 39 to 48). The points that automated tests cannot prove:

1. **A real cloud deployment.** Neither the Azure template nor the Google Cloud module has
   ever been deployed: no cloud account was available, and the tools could not be run on the
   build machine, so the only check is CI building and validating them. Expect to fix
   something on the first try. Before it can work at all: tag a release (the first hub image
   is published then) and make the image package public on GitHub.
2. **Schools on other PCs over real HTTPS.** Every automated test uses a local address. The
   Compose `https` profile (Caddy) has not been run with a real domain.
3. **The console in a browser, by a person.** It was driven once through a real browser
   against a local hub and a demo school of 1,200 students (setup, invitation, join, reports,
   a correction arriving at the school, Urdu). Releasing a key from the console screen was
   not tried by hand (the release itself is tested).
4. **Task 43, the conflict rule**, with real people at two desks: it is the heart of the
   design and worth seeing once.
5. **Task 45, a lost backup password**, end to end with a real backup file.
6. **Two real schools** (task 48): compare each report figure with the school's own reports.

### What is not built, or not proven

- **Head office cannot enter records.** The console corrects names and contact details
  only. It has no screens for receipts, attendance or marks, so "both sides entered a
  receipt" can only happen through the API today; the merge rules for it are tested there.
- **One hub instance.** The alpha hub must run as a single instance (the templates enforce
  it). It has not been load-tested with many schools.
- **No in-place replacement of a school's sync key or of a backup data key.** A leaked sync
  key means removing the school and inviting it again; a released backup key keeps opening
  that school's backups and must be guarded like the recovery key.
- **Number sequences are not synced.** If head office ever issues receipts or admits
  students it will need its own number series; today it issues none.
- **Sync is every five minutes or on demand**, not instant, and a school that restores an
  old backup needs *Send everything again* from the console to catch up.
- **No version bump or release** was made: the hub reports 0.3.1.

### Decisions left for the owner

- **Should leaving a backup key be required to join?** It is optional now, the school's
  choice.
- **Should releasing a key need two people?** Now one user with the release permission, the
  passphrase and a reason is enough; every release is logged and shown to the school.
- **Who holds the escrow passphrase, and where is it written down?** If it is lost, no key
  can ever be released.
- **When a school leaves, its sealed backup key is erased at once.** This is stricter than
  the design document (which kept it until the archive was deleted), so that a key can never
  be released without the school being told. Say if you want the other behaviour.
- **How long to keep the archive of a school that left.** Nothing deletes it automatically;
  an administrator deletes it by typing the school's code.
- **Which details head office may correct.** Now: the school's name and city; students'
  name, father's name and address; guardians' name, phone, email and address; staff name,
  father's name, phone and email. The list is one table in the code.
- **Who gets which hub permission** (administer, read reports, release keys). The first
  administrator has all three.
- **Where to run the hub** (one machine with Compose, Azure or Google Cloud), and who makes
  the first cloud deployment.
- **When to tag the first release that carries the hub**, which is also what publishes the
  image.

## Summary for the owner — Standard alpha

The Standard alpha was built in eleven pull requests, each from a clean `main` with CI green
on Linux and Windows (#29 the task list, #30 foundation, #31 timetable, #32 leave, #33
admissions, #34 library, #35 transport, #36 hostel, #37 inventory, #38 directory login, #39
read-only API, then this close-out). Every module sits behind the edition setting: a Lite
install keeps the same screens and endpoints, every Lite test stays green, and switching an
install to Standard in Settings → Edition adds the modules without touching existing data.

### What to test by hand

Follow TESTING.md section 5 (tasks 27–38) on the demo school after switching it to
Standard. The points that cannot be proven in CI:

1. **Timetable on paper (task 27).** Print a section grid and a teacher grid; check the
   period times and breaks match the shift, and that a double period prints as one block.
   Generate again after changing a lesson and read the check findings.
2. **Leave and attendance in step (tasks 28–29).** Approve a staff request, then open the
   staff attendance register for those days; try to mark the person present. Record a
   student leave and mark the section's attendance.
3. **Admissions without retyping (task 30).** Convert an admitted application and compare
   the student's profile, guardian and admission voucher with the application. Print the
   offer letter in Urdu and check the layout.
4. **Library desk with a barcode scanner (task 31)**, if the school has one: scan an
   accession number into the desk's field and confirm it looks the copy up at once.
5. **Transport and hostel fees (tasks 32–33).** Generate a month's vouchers for a section
   with riders and residents and read the voucher lines and totals; print one.
6. **Store (task 34)**: record a day's issues as the store keeper would, then run the
   movements report for the month.
7. **Directory login (task 36)** against the school's real Active Directory: this is the one
   feature whose wire protocol has no automated test. Test the bind account first, then a
   user lookup, then a real login, a wrong password and a disabled account.
8. **Read-only API (task 37)** from the system that will consume it (a portal, a BI tool):
   issue a key limited to its address, call `/students` and `/attendance/daily`, confirm
   the ETag round trip and that the call log fills.
9. **Roles**: log in as the librarian and the store keeper and check what they can and
   cannot open; the matrix in `docs/permissions.md` is the reference.

The demo school's read-only key is `mk_demo-readonly-key-0000000000000000000` (demo
installs only; the loader refuses to run next to a real school).

### Decisions left for the owner

- **Who approves leave.** The default is the principal and the administrator
  (`leave.approve`); Leave → Settings can name one approver instead.
- **Fines and charges on vouchers.** Library fines, transport and hostel charges only reach
  vouchers when a fee head is chosen in each module's settings; until then fines are settled
  at the desk and charges are informational. Decide the heads before the first monthly run.
- **Admission fee.** Conversion raises the admission voucher only when the application has
  an offer fee and the admissions fee head is set.
- **Hostel consent.** Each hostel decides whether leave-outs need recorded guardian consent.
- **Directory group mapping.** Which groups grant which roles, and whether people in no mapped
  group get a default role or are refused. Local accounts (the setup administrator and
  anything created in Users) always keep their password.
- **Read-only API scopes and addresses.** Issue one key per consuming system with only the
  scopes it needs; limit it to that system's address when it has a fixed one.
- **Instance key.** The directory bind password is sealed with a key kept in the database
  (ADR 0004); moving it to a key file on the server is a later hardening step.
- **Transport capacity** is a warning, not a block: a route can carry more students than
  seats, on purpose, for schools that run two trips.

## Summary for the owner — M2 (Lite complete)

M2 was built in nine pull requests, each from a clean `main` with CI green on Linux and
Windows (#18 stage 1, #19 stage 2, #20 stage 3, #21 stage 4, #22 stage 5, then stages 6, 7,
8 and the close-out). Nothing that worked in M1 behaves differently unless a stage says so:
every existing exam setup became one term holding its assessments, every student still takes
every class subject until the office allocates subjects, and no permission changed meaning.

### What to test by hand

Follow TESTING.md tasks 18–25 on the demo school, in this order:

1. **Reports (tasks 18–19).** Run a student list, a fee collection and an attendance report
   in English and in Urdu; download each as PDF and Excel. Log in as a teacher and check the
   Reports page only offers attendance and exam reports for that teacher's sections.
2. **Custom report (task 20).** Build a report from the catalogue, preview it, save it, share
   it, and run it as the accountant.
3. **Boards (task 21).** Register a class for a board exam, import a results file with a
   deliberate error, fix it, import, undo, and check the student's profile and the board
   reports agree.
4. **Terms (task 22).** Add a second term to the demo year, move an assessment into it, add a
   calculated "best of" component, and look at a report card with two terms.
5. **Subjects per student (task 23).** Change a grade 9 student's group, open Marks entry for
   Biology, and confirm only the students who take Biology are listed.
6. **Allocation screens (task 24).** Change a teacher in the grid, move two students between
   sections, and apply a balancing proposal.
7. **Consents (task 25).** Record consents for a section, withdraw one on the profile, and
   run the three consent reports.

### Decisions left for the owner

- **Teacher access to reports.** Teachers run attendance and exam reports for their own
  sections only, and never fee or student-list reports. If class teachers should print
  their section's student list, grant `reports.read` to the teacher role in Settings → Users.
- **Shared custom reports.** Only the principal and admin can share a saved report with
  everyone; the accountant can save private ones. Change the `reports.manage` grant if the
  accountant should share too.
- **Board grading.** Each board carries its own pass mark and grade bands; the demo uses BISE
  Karachi's. Set the real boards' bands before importing results.
- **Default exam structure.** A new academic year proposes two terms (two assessments each)
  on the Setup page; the API default stays one term so scripts and older clients are
  unaffected. Pick the structure the school actually follows before the first marks go in.
- **Electives and groups.** Marking subjects as electives changes nothing until a student is
  allocated. Decide whether the office allocates per section (Subject allocation page) or
  imports from the admission forms (Excel import).
- **Balancing sections.** The proposal moves the last-admitted students and ignores capacity;
  it is only a preview, so nothing moves until applied. Tick students who must stay first.
- **Consents.** Which consents are required for everyone versus a class range, and which
  renew every year, is a school policy: set the types before the office starts recording.
- **Roll numbers after moves.** Default is a fresh number in the new section; "keep" is there
  for schools that number across the class.

## Summary for the owner — Stages 5, 6 and 7 (night run of 2026-10-30/31)

All three stages are merged into `main` in order (pull requests #6, #7 and #8),
each from a clean `main` with CI green on Linux and Windows. What changed, in short:

- **Stage 5 — Backup and restore.** Nightly encrypted backups (`.mbk`, AES-256-GCM, per-file
  key wrapped for the school's key pair so no password is needed at night), restore with the
  backup password or the recovery key, preview before restore, tamper detection, password
  change that applies to new backups only, dashboard warning after two days, everything
  audited.
- **Stage 6 — Import and export.** Excel/CSV import wizard for students + guardians, staff and
  opening fee balances with templates, automatic column matching (English and Urdu
  headings), cleanup of messy data, row-level checks, duplicate handling, batches with
  one-click rollback; Excel/CSV export on every list.
- **Stage 7 — Pilot readiness.** Year rollover, permission matrix document, dashboard with
  real figures, performance check with the pilot profile, user guide in both languages,
  manual installer builds.

**Please test by hand** (things that cannot be proven in CI):

1. Install the Windows build from the Release workflow artifacts on a school PC; run the
   setup wizard; confirm the bundled database starts and the nightly backup lands in
   `Documents\Madrasa Backups`. Point the backup folder at a USB drive and a network share.
2. Restore a backup on a second PC with the recovery key (the pilot's disaster scenario).
3. Import the pilot school's real student register through the wizard (on a copy, not on the
   production install) and look at the preview errors; the column matching is built for
   varied headings but real files will show gaps.
4. Walk one month of the real routine: generate vouchers, collect fees, mark attendance,
   enter and approve marks, print a report card and a voucher, in Urdu as well as English.
5. On the last day of the year, run the rollover preview and check the destinations.
6. Leave the app running past midnight with an overdue voucher and confirm the late fee
   appears on its own (and that a waived one stays waived).
7. LAN mode on two or more real PCs over the school Wi-Fi: server on one, clients on the
   others, the Windows firewall prompt, the fingerprint check, and several teachers entering
   marks at once (TESTING.md §6 and task 17).

**Decisions for you:**

- Who should hold `exams.approve` and `attendance.correct` at the pilot school (the defaults
  are principal, and principal + admin). The matrix is in `docs/permissions.md`.
- Whether the dashboard's defaulter rule (any balance past its due date) matches how the
  school talks about defaulters, or whether a minimum of days overdue should be the default.
- Whether the scheduled backup time (02:00) suits a PC that is switched off at night; if so,
  set it to a time in the school day (the status card shows when it last ran).
- Code signing for the installers before the pilot (SmartScreen warnings otherwise).

## User manual (PDF, English and Urdu)

### 2026-11-14 — The manual, its screenshots and its build
- Done: `docs/manual`: the whole manual in `manual.yaml`, English and Urdu side by side, laid
  out by `manual.typ` in the brand's colours with the logo (cover, part pages, chapter
  headings, edition chips, tips and warnings; Urdu right to left in Noto Nastaliq Urdu).
  Seven parts: getting started (install, setup wizard, settings, structure, staff), daily
  work (students, import, attendance, fees, examinations, reports, backups), the Standard
  modules, the school server and its clients, Madrasa Nexus (hub setup, head office
  console, a school joining), the parent app (at the school and on the phone), and the
  year-end rollover with a troubleshooting page. 137 steps, each with its own screenshot;
  the three that happen outside Madrasa (running an installer, deploying the hub,
  installing the APK) say so, and the build stops on any other step without one.
- `crates/manual` typesets both books with the Typst crates the API already uses (fonts
  embedded, nothing to install); `scripts/build-manual.sh` writes
  `Madrasa-User-Manual-EN.pdf` and `-UR.pdf`. Screenshots are kept as WebP and handed to
  the PDF as JPEG, which halves the size (about 17 MB a book).
- `scripts/manual-screenshots.sh` makes every screenshot again, in both languages, light
  mode: fresh copies of the demo school (Standard edition), an empty school for the setup
  wizard, the demo network on a hub and a hub before its first setup, the desktop app and
  the parent app from Vite, driven by Playwright (`docs/manual/tools`). Screens are found
  by their translated labels, so the same scenes work in Urdu.
- CI builds both PDFs on every push and keeps them as the artifact `madrasa-user-manual`;
  the Release workflow attaches them to the draft release (docs/RELEASING.md).
- Tested: `cargo test -p madrasa-manual` (the WebP to JPEG step); both PDFs built from the
  committed screenshots; every page looked at in both languages.
- Left: the screenshots show the screens as they are on `main`; after the brand and
  offline pull requests are merged, run `scripts/manual-screenshots.sh` once and commit.
