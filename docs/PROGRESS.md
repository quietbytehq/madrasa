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

**Decisions for you:**

- Who should hold `exams.approve` and `attendance.correct` at the pilot school (the defaults
  are principal, and principal + admin). The matrix is in `docs/permissions.md`.
- Whether the dashboard's defaulter rule (any balance past its due date) matches how the
  school talks about defaulters, or whether a minimum of days overdue should be the default.
- Whether the scheduled backup time (02:00) suits a PC that is switched off at night; if so,
  set it to a time in the school day (the status card shows when it last ran).
- Code signing for the installers before the pilot (SmartScreen warnings otherwise).
