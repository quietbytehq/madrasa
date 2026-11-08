# Madrasa Standard — alpha

Goal: the Standard edition (docs/spec.md §2, §3, §4) as a working alpha: the modules a
school needs beyond Lite, served by `madrasa-server` to many desktop clients. LAN mode and
the server binary already exist; this milestone adds the modules and the integrations.

**Lite stays exactly as it is.** Every Standard module sits behind a feature flag that the
Lite edition does not enable (`madrasa_core::edition`); the API refuses the endpoints and the
UI hides the modules unless the instance edition is `standard` or `nexus`. Tables may be
created by migrations on every edition (they stay empty on Lite); behaviour of existing Lite
screens must not change, and every Lite test keeps passing.

Stages are ordered by what schools need most, as told by the pilot: the daily timetable and
leave first, then admissions for the new year, then the office registers (library, transport,
hostel, inventory), then IT integration (directory login, read-only API).

## How to work

- Other sessions work on this repository at the same time. **Start every stage from the
  latest `main`** (`git fetch origin && git checkout -b dev/std-stage-N-name origin/main`),
  use the **next free migration number after pulling `main`**, and **rebase on `main`
  before opening the PR**.
- One stage at a time, in order. One branch and one pull request per stage. When the PR is
  open and CI is green, merge it into `main` and continue with the next stage.
- Follow every rule in CLAUDE.md (UUID v7, `school_id`, sync columns, soft delete, audit
  log, money as exact decimals, backend permissions, en/ur i18n with RTL, tests before
  ticking). Use the demo data generator for all testing; never real data.
- Every new permission goes into `madrasa_core::permissions`, docs/permissions.md and the
  role defaults (teachers never get office permissions by default). Every new feature code
  goes into `madrasa_core::edition` and `apps/desktop/src/api/types.ts`.
- Tick `[x]` only when built and tested. Keep docs/PROGRESS.md updated after each stage;
  log blockers in docs/BLOCKERS.md and move on rather than guess. Record significant
  decisions as ADRs in docs/adr/.
- Printouts and reports: English and Urdu, light theme, through the report PDF engine.

## Stage 0 — Standard foundation

- [x] Feature codes for the Standard modules: `timetable`, `leave`, `admissions`,
      `library`, `transport`, `hostel`, `inventory`, `hr` (replaces `hr_payroll` as the
      code for HR basics; payroll stays for later), `directory_login`, `readonly_api`;
      Standard and Nexus enable them, Lite does not; `GET /system/features` lists them
- [x] Edition switch: Settings → Edition (`settings.manage`) shows the edition and lets
      the principal switch Lite ↔ Standard (`PUT /settings/edition`); the API reloads its
      flags; roles gain the new module permissions on the switch; switching back to Lite
      hides the modules but keeps the data; audited
- [x] `require_feature(Feature)` guard used by every Standard router: 404 "not in this
      edition" on Lite, so a Lite install never exposes a Standard endpoint
- [x] Sidebar and routes for the new modules (hidden on Lite; "Coming soon" until a stage
      ships them); permissions matrix rows; i18n
- [x] Tests: features per edition, the switch, a Standard router refused on Lite, every
      existing suite unchanged

## Stage 1 — Timetable

Periods, breaks, working days, rooms, lessons, a generator and the daily views, reusing the
solver of quietbytehq/timewizer (Apache-2.0, same owner): its compile → difficulty order →
ejection-chain placement → relaxation → verify pipeline is ported into
`madrasa_core::timetable` with Madrasa's entities (sections, subjects, staff, rooms).

- [x] `timetable_shifts` (name, working days, period times, breaks that lessons cannot
      cross), one per shift (morning/afternoon); `rooms` (name, short name, building,
      capacity); `timetable_lessons` (section or group of sections, subject, teacher(s),
      sessions per week, length 1–3, room pool); time-off grids per teacher, section and
      room; teacher limits (max per day, per week, consecutive)
- [x] Generator: `POST /timetable/generate` runs the ported solver in the background with a
      time limit, reports placed/unplaced with plain-language reasons and fixes; manual
      moves with live collision checks and locks; `verify` as an independent checker
- [x] Views: by section, by teacher, by room, for a day or the week; printable (PDF, en/ur);
      the teacher's own timetable on the Dashboard; substitutions: a teacher away for a
      day or a period gets cover suggestions (same subject first, fewest covers first) and
      a cover sheet
- [ ] Attendance per period (optional, behind a setting) — deferred: it changes the Lite
      attendance tables; to be done in a later stage. A period-marked sheet reuses the
      daily attendance engine and the timetable's lesson
- [x] Permissions `timetable.read` (everyone), `timetable.manage` (principal, admin);
      reports: section timetable, teacher load, free rooms, cover sheet
- [x] Tests: core solver (ported tests), generation on the demo school, collisions, cover
      suggestions, vitest for the grid

## Stage 2 — Leave: student leave requests and HR basics

- [x] Staff leave types (annual, sick, casual, …) with yearly entitlement, carry-over and
      whether approval is needed; balances per staff member and year; requests with dates
      (half days allowed), reason and attachment note; approval by the principal or a
      configured approver; the approved days mark `staff_attendance` as `leave`
      automatically and unmarked on withdrawal; audited
- [x] Student leave requests: a guardian's note recorded by the office (dates, reason),
      approved by the class teacher or office; approved days mark the attendance entry as
      `leave` on each sheet of the range (and are restored if the sheet is corrected);
      the register and the absentee report say "leave (approved)"; the student profile
      lists requests
- [x] Staff attendance polish: monthly staff register with leave shown, late marking with
      time, summary per staff member and month
- [x] Permissions `leave.read`, `leave.manage` (office), `leave.approve` (principal,
      admin); teachers may request their own leave and approve student leave for their
      sections; reports: leave balances, leave register, student leave register
- [x] Tests: balances and carry-over, approval marks attendance, overlapping requests
      refused, teacher scope, vitest

## Stage 3 — Admissions pipeline

- [x] `admission_applications`: enquiry → applied → test scheduled → interviewed →
      offered → admitted | rejected | withdrawn, with the applicant's details (same shape
      as a student plus guardian), the class applied for, the session, test marks and
      interview notes, offer letter date and fee, and every status change dated and
      audited; configurable stages per school
- [x] Converting an admitted application creates the student, guardians and enrolment in
      one step (no retyping) and links back; admission fee voucher generated on admission
      when a fee head is configured
- [x] Screens: pipeline board by stage with counts, application form, list with filters,
      bulk status moves; printable offer letter (PDF, en/ur); import of enquiries from
      Excel through the import engine
- [x] Permissions `admissions.read`, `admissions.manage` (office); reports: admissions
      funnel per session, applications by class, pending tests
- [x] Tests: the stage machine, conversion creates everything once, import, vitest

## Stage 4 — Library

- [x] Catalogue: `library_books` (title, author, publisher, ISBN, category, language,
      subject) with `library_copies` (accession number, shelf, condition, status);
      members are students and staff; loan rules per member type (max books, loan days,
      fine per day, renewal count)
- [x] Issue and return by accession number or barcode text; renewals; overdue list; fines
      computed at return (`madrasa_core::library`), waivable with a reason, posted as a
      fee voucher line for students (configurable) or recorded against staff; lost books
- [x] Screens: catalogue with search, member page with loans and fines, issue/return desk,
      stock take; reports: overdue books, fines collected, most borrowed, stock by category
- [x] Permissions `library.read`, `library.manage` (librarian role added to the defaults)
- [x] Tests: loan rules, fines and waivers, renewals limit, stock, vitest

## Stage 5 — Transport

- [x] Vehicles (registration, capacity, driver, helper), routes with ordered stops and
      pickup times, `transport_assignments` per student and year (route, stop, from/to
      dates) with a transport fee head charged per month through the fee structure
- [x] Screens: routes and stops, vehicle list, assign students by section, route roster
      (printable); reports: route manifest, students without a stop, fee per route
- [x] Permissions `transport.read`, `transport.manage`
- [x] Tests: capacity warnings, fee integration, roster, vitest

## Stage 6 — Hostel

- [x] Hostels, floors, rooms and beds; `hostel_allocations` per student and year with
      from/to dates; hostel fee head per hostel charged monthly; visitor and leave-out
      register (out/in with guardian consent)
- [x] Screens: occupancy board, allocate and move, register; reports: occupancy,
      residents by class, outstanding hostel fees
- [x] Permissions `hostel.read`, `hostel.manage`
- [x] Tests: no double allocation of a bed, fee integration, vitest

## Stage 7 — Inventory

- [x] Item categories and items (unit, reorder level), stock locations (store, lab,
      room), stock movements (purchase in, issue to staff/room/student, return, write-off)
      with quantities and optional cost; asset tags for durable items
- [x] Screens: items with stock on hand, movement entry, issue register; reports: stock on
      hand, below reorder level, movements in a period
- [x] Permissions `inventory.read`, `inventory.manage` (store keeper role)
- [x] Tests: stock never negative, reorder report, vitest

## Stage 8 — Directory (Active Directory / LDAP) login

- [x] Settings → Directory login (`settings.manage`): LDAP server URL (ldaps://), bind
      account, user base DN and filter, attribute for username, group→role mapping, and
      "test connection"; stored in settings with the bind password encrypted with the
      instance key (never logged)
- [x] Login: when enabled, a username that matches the directory is authenticated by an
      LDAP bind (`ldap3` crate, TLS required); the local account is created or updated on
      first login (full name, email, roles from groups); local passwords still work for
      the built-in admin and for accounts marked local; lockout and audit as today
- [x] Tests against an in-process LDAP stub (bind success, wrong password, group mapping,
      disabled account), settings validation, vitest for the settings card and the login
      hint

## Stage 9 — Read-only API

- [x] API keys (`settings.manage`): name, scopes (students, attendance, fees, exams,
      timetable, library), optional IP allow-list, expiry; shown once; hashed at rest;
      audited; revocable
- [x] `GET /api/ro/v1/...` endpoints returning JSON pages (students, sections, attendance
      summary per day, fee balances, results, timetable) with `If-None-Match` support and
      a documented rate limit; every call logged with the key; no writes
- [x] Documentation page under Settings → API with the endpoints and example `curl`
      calls; OpenAPI JSON served at `/api/ro/v1/openapi.json`
- [x] Tests: scopes, expiry, revocation, rate limit, a Lite install has no `/api/ro`

## Stage 10 — Close-out

- [x] Demo data for every Standard module (timetable generated, leave, applications,
      library stock and loans, routes, hostel beds, inventory, an API key)
- [x] User guide (en/ur) chapters, TESTING.md tasks for every stage, spec sections checked
      against the code, permissions matrix complete
- [x] docs/PROGRESS.md: what to test by hand and the decisions left for the owner

## Done when

- A Standard server with several clients runs every module above with the office's daily
  work covered: timetable on screen and paper, leave and attendance in step, admissions
  without retyping, registers for library, transport, hostel and store, directory logins,
  and a read-only feed for the school's other systems.
- A Lite install behaves exactly as before: same screens, same endpoints, every Lite test
  green; switching to Standard adds modules without touching existing data.
