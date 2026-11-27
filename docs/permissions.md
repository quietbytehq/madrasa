# Permission matrix (Lite, v0.1)

Every endpoint checks a permission in the backend (`crates/api`), never only in the UI. The
built-in roles are created at setup with the grants below (`RoleCode::default_permissions` in
`crates/core/src/permissions.rs`); a school may change a role's grants later, and the grants of
existing roles are extended by migrations when a stage adds a permission.

Legend: ✓ granted by default · — not granted.

| Permission | What it allows | Principal | Admin | Accountant | Teacher | Librarian | Store keeper |
| --- | --- | :-: | :-: | :-: | :-: | :-: | :-: |
| `school.read` | See the school profile | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `school.manage` | Edit the school profile | ✓ | ✓ | — | — | — | — |
| `settings.manage` | School settings (student number prefix, …) | ✓ | ✓ | — | — | — | — |
| `users.read` | List user accounts | ✓ | ✓ | — | — | — | — |
| `users.manage` | Create, edit, disable accounts | ✓ | ✓ | — | — | — | — |
| `roles.manage` | Change role grants | ✓ | ✓ | — | — | — | — |
| `audit.read` | Read the audit log | ✓ | ✓ | — | — | — | — |
| `academics.read` | Years, classes, sections, subjects | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `academics.manage` | Edit academic structure, calendar, exam setup (terms, components), electives and subject groups, boards, consent types, year rollover | ✓ | ✓ | — | — | — | — |
| `students.read` | Student directory and profiles, subject allocations and consents (teachers: own sections only) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `students.manage` | Add, edit, withdraw students; import students and opening balances; allocate subjects (and import allocations); move students between sections; record consents | ✓ | ✓ | — | — | — | — |
| `attendance.read` | Attendance sheets, register, absentees (teachers: own sections) | ✓ | ✓ | — | ✓ | — | — |
| `attendance.manage` | Mark attendance (teachers: own sections) | ✓ | ✓ | — | ✓ | — | — |
| `attendance.correct` | Change a submitted attendance sheet | ✓ | ✓ | — | — | — | — |
| `exams.read` | Marks sheets, results, report cards (teachers: own sections) | ✓ | ✓ | — | ✓ | — | — |
| `exams.manage` | Enter and submit marks (teachers: own subjects and sections) | ✓ | ✓ | — | ✓ | — | — |
| `exams.approve` | Approve or reopen submitted marks sheets | ✓ | — | — | — | — | — |
| `fees.read` | Vouchers, ledgers, defaulters, collections | ✓ | ✓ | ✓ | — | — | — |
| `fees.manage` | Generate vouchers, record payments | ✓ | ✓ | ✓ | — | — | — |
| `fees.approve` | Concessions, late-fee waivers, reversals, fee setup | ✓ | ✓ | — | — | — | — |
| `teachers.read` | Staff list, staff attendance and the teacher allocation grid | ✓ | ✓ | — | ✓ | ✓ | ✓ |
| `teachers.manage` | Edit staff, assignments (including the allocation grid), staff attendance; import staff | ✓ | ✓ | — | — | — | — |
| `reports.read` | Reports (built-in and custom; teachers run attendance and exam reports for their own sections with the module's read permission) | ✓ | ✓ | ✓ | — | — | — |
| `reports.manage` | Save shared custom reports (others may run them) | ✓ | ✓ | — | — | — | — |
| `backup.manage` | Backup settings, run, restore, password | ✓ | ✓ | — | — | — | — |
| `timetable.read` | See timetables (by section, teacher, room), the cover sheet, own timetable (Standard) | ✓ | ✓ | ✓ | ✓ | — | — |
| `leave.read` | Leave requests and balances (teachers: their own, and student leave for their sections), staff register (Standard) | ✓ | ✓ | ✓ | ✓ | — | — |
| `leave.manage` | Leave types, balances and carry-over, requests for others, student leave requests and decisions (Standard) | ✓ | ✓ | — | — | — | — |
| `leave.approve` | Approve or reject staff leave requests (Standard) | ✓ | ✓ | — | — | — | — |
| `admissions.read` | Admissions pipeline: applications, board, reports (Standard) | ✓ | ✓ | ✓ | — | — | — |
| `admissions.manage` | Record and move applications, stages, convert to students, import enquiries (Standard) | ✓ | ✓ | — | — | — | — |
| `library.read` | Library catalogue, loans, members, fines and reports (Standard) | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| `library.manage` | Catalogue and copies, issue, return, renew, fines and waivers, loan rules (Standard) | ✓ | ✓ | — | — | ✓ | — |
| `transport.read` | Transport routes, vehicles, assignments, rosters and reports (Standard) | ✓ | ✓ | ✓ | — | — | — |
| `transport.manage` | Vehicles, routes and stops, assign students, the transport fee head (Standard) | ✓ | ✓ | — | — | — | — |
| `hostel.read` | Hostels, rooms, occupancy, residents, the register and reports (Standard) | ✓ | ✓ | ✓ | — | — | — |
| `hostel.manage` | Hostels and rooms, allocate and move residents, record leave-outs and visitors (Standard) | ✓ | ✓ | — | — | — | — |
| `inventory.read` | Store items, stock on hand, movements, assets and reports (Standard) | ✓ | ✓ | ✓ | — | — | ✓ |
| `inventory.manage` | Items, locations, record movements and transfers, asset tags (Standard) | ✓ | ✓ | — | — | — | ✓ |
| `timetable.manage` | Shifts, rooms, lessons, rules, generate, edit and publish timetables, record absences and covers (Standard) | ✓ | ✓ | — | — | — | — |
| `parents.manage` | Parent app: give a guardian access, see who has access, end access (schools joined to a hub) | ✓ | ✓ | — | — | — | — |
| `notices.manage` | Parent app: write, publish and withdraw school notices | ✓ | ✓ | — | — | — | — |
| `messages.reply` | Parent app: answer parents' messages, only as class teacher of the child's section | ✓ | — | — | ✓ | — | — |
| `messages.view_all` | Parent app: read every conversation between parents and class teachers (read-only) | ✓ | — | — | — | — | — |
| `nexus.admin` | Network hub (Nexus edition only) | — | — | — | — | — | — |

Notes

- Teachers are scoped to the sections they are class teacher of or teach a subject in. The
  scope is applied in the API for students, attendance and exams.
- The **Parent** role exists only in the Nexus edition and reads the school profile only.
- The **Librarian** role exists on Standard and Nexus (created on the edition switch) and
  holds the library grants plus read access to students and staff.
- The **Store keeper** role exists on Standard and Nexus likewise and holds the inventory
  grants plus read access to students and staff.
- The table is checked by a unit test (`permissions::tests::matrix_matches_docs`), so it cannot
  drift from the code without failing CI.

## Nexus hub

The hub server (`madrasa-nexus`) has its own accounts and three permissions of its own,
granted per hub user (docs/spec.md §16.1). They are separate from the school roles above.

- `nexus.admin`: schools and invitations, corrections to a school's names and contact
  details (docs/spec.md §16.8), hub users, the hub audit log.
- `nexus.reports`: network-wide reports.
- `nexus.escrow`: release a school's backup key (with the escrow passphrase and a reason),
  change the escrow passphrase, see which schools have left a key and every release.
  Setting escrow up needs `nexus.admin`, which also sees that list.

Every hub user, whatever their permissions (also none), changes their **own** password
(`POST /auth/password`, with the current one; the school logins' rules and lockout). Setting
another user's password is `nexus.admin`; an administrator's own password goes the same way
as everyone's, with the current one.

At the school, leaving the backup key with head office or taking it back needs both
`settings.manage` and `backup.manage`; seeing what head office holds and every release of
the key needs `settings.manage` (docs/spec.md §16.7).

The first administrator, created at first-run setup, holds all three.

## Routes

Who may call each route of the school API (`/api/v1`). This is the same table the server
enforces before a handler runs (`crates/api/src/authz.rs`); `crates/api/tests/authz_matrix.rs`
calls every route as every built-in role and fails if the code, this table and the behaviour
disagree, or if a route is missing here.

- **public**: no login (logging in, first-run setup, the health check, what the login screen
  needs). Setup refuses once a school exists.
- **signed in**: any signed-in user.
- A permission, or several joined by "or": a signed-in user holding at least one of them.
  The handler may ask for more (a teacher's own sections, the owner of a saved report, the
  kind of file being imported); it never asks for less.

The read-only API (`/api/ro/v1`, Standard) is not in this table: it is called with an API
key and its scopes, not a user's login (docs/spec.md §15).

| Method | Route | Needs |
| --- | --- | --- |
| GET | `/academic-years` | `academics.read` |
| POST | `/academic-years` | `academics.manage` |
| POST | `/academic-years/{id}/activate` | `academics.manage` |
| GET | `/classes` | `academics.read` |
| POST | `/classes` | `academics.manage` |
| PATCH | `/classes/{id}` | `academics.manage` |
| GET | `/classes/{id}/subjects` | `academics.read` |
| PUT | `/classes/{id}/subjects` | `academics.manage` |
| GET | `/sections` | `academics.read` |
| POST | `/sections` | `academics.manage` |
| PATCH | `/sections/{id}` | `academics.manage` |
| GET | `/subjects` | `academics.read` |
| POST | `/subjects` | `academics.manage` |
| PATCH | `/subjects/{id}` | `academics.manage` |
| GET | `/admissions/applications` | `admissions.read` |
| POST | `/admissions/applications` | `admissions.manage` |
| POST | `/admissions/applications/move` | `admissions.manage` |
| GET | `/admissions/applications/{id}` | `admissions.read` |
| PUT | `/admissions/applications/{id}` | `admissions.manage` |
| DELETE | `/admissions/applications/{id}` | `admissions.manage` |
| POST | `/admissions/applications/{id}/convert` | `students.manage` |
| POST | `/admissions/applications/{id}/move` | `admissions.manage` |
| GET | `/admissions/applications/{id}/offer-letter` | `admissions.read` |
| GET | `/admissions/board` | `admissions.read` |
| GET | `/admissions/settings` | `admissions.read` |
| PUT | `/admissions/settings` | `admissions.manage` |
| GET | `/admissions/stages` | `admissions.read` |
| POST | `/admissions/stages` | `admissions.manage` |
| PUT | `/admissions/stages/{id}` | `admissions.manage` |
| DELETE | `/admissions/stages/{id}` | `admissions.manage` |
| POST | `/allocation/balance` | `students.manage` |
| POST | `/allocation/moves` | `students.manage` |
| GET | `/allocation/sections` | `students.manage` |
| GET | `/allocation/teachers` | `teachers.read` |
| PUT | `/allocation/teachers/cell` | `teachers.manage` |
| GET | `/settings/api-keys` | `settings.manage` |
| POST | `/settings/api-keys` | `settings.manage` |
| DELETE | `/settings/api-keys/{id}` | `settings.manage` |
| GET | `/settings/api-keys/{id}/calls` | `settings.manage` |
| GET | `/attendance/absentees` | `attendance.read` |
| GET | `/attendance/calendar` | `attendance.read` |
| POST | `/attendance/calendar/holidays` | `academics.manage` |
| DELETE | `/attendance/calendar/holidays/{id}` | `academics.manage` |
| PUT | `/attendance/calendar/weekend` | `academics.manage` |
| GET | `/attendance/register` | `attendance.read` |
| GET | `/attendance/sheet` | `attendance.read` |
| PUT | `/attendance/sheet` | `attendance.manage` |
| GET | `/attendance/sheet/history` | `attendance.read` |
| GET | `/attendance/staff` | `teachers.read` |
| PUT | `/attendance/staff` | `teachers.manage` |
| GET | `/attendance/staff/{id}` | `teachers.read` |
| GET | `/attendance/status` | `attendance.read` |
| GET | `/attendance/students/{id}` | `attendance.read` |
| GET | `/dashboard` | `students.read` |
| GET | `/audit` | `audit.read` |
| POST | `/auth/login` | public |
| POST | `/auth/logout` | signed in |
| GET | `/auth/me` | signed in |
| GET | `/auth/options` | public |
| POST | `/auth/password` | signed in |
| GET | `/backup/files` | `backup.manage` |
| POST | `/backup/password` | `backup.manage` |
| POST | `/backup/preview` | `backup.manage` |
| POST | `/backup/restore` | `backup.manage` |
| POST | `/backup/run` | `backup.manage` |
| GET | `/backup/runs` | `backup.manage` |
| GET | `/backup/settings` | `backup.manage` |
| PUT | `/backup/settings` | `backup.manage` |
| GET | `/backup/status` | `backup.manage` |
| POST | `/backup/unlock` | `backup.manage` |
| GET | `/boards` | `exams.read` |
| POST | `/boards` | `academics.manage` |
| PUT | `/boards/candidates/{id}` | `academics.manage` |
| DELETE | `/boards/candidates/{id}` | `academics.manage` |
| GET | `/boards/exams` | `exams.read` |
| POST | `/boards/exams` | `academics.manage` |
| GET | `/boards/exams/{id}` | `exams.read` |
| PUT | `/boards/exams/{id}` | `academics.manage` |
| DELETE | `/boards/exams/{id}` | `academics.manage` |
| GET | `/boards/exams/{id}/candidates` | `exams.read` |
| POST | `/boards/exams/{id}/candidates` | `academics.manage` |
| GET | `/boards/exams/{id}/results` | `exams.read` |
| POST | `/boards/exams/{id}/results/apply` | `academics.manage` |
| POST | `/boards/exams/{id}/results/parse` | `academics.manage` |
| POST | `/boards/exams/{id}/results/preview` | `academics.manage` |
| PUT | `/boards/{id}` | `academics.manage` |
| DELETE | `/boards/{id}` | `academics.manage` |
| GET | `/students/{id}/board-results` | `exams.read` |
| GET | `/consent-types` | `students.read` |
| POST | `/consent-types` | `academics.manage` |
| PUT | `/consent-types/{id}` | `academics.manage` |
| DELETE | `/consent-types/{id}` | `academics.manage` |
| GET | `/sections/{id}/consents` | `students.read` |
| POST | `/sections/{id}/consents` | `students.manage` |
| GET | `/students/{id}/consents` | `students.read` |
| POST | `/students/{id}/consents` | `students.manage` |
| GET | `/settings/directory` | `settings.manage` |
| PUT | `/settings/directory` | `settings.manage` |
| POST | `/settings/directory/test` | `settings.manage` |
| GET | `/settings/edition` | `settings.manage` |
| PUT | `/settings/edition` | `settings.manage` |
| GET | `/exams/structure` | `exams.read` |
| GET | `/exams/terms` | `exams.read` |
| POST | `/exams/terms` | `academics.manage` |
| PUT | `/exams/terms/weights` | `academics.manage` |
| PUT | `/exams/terms/{id}` | `academics.manage` |
| DELETE | `/exams/terms/{id}` | `academics.manage` |
| GET | `/exams/assessments` | `exams.read` |
| POST | `/exams/assessments` | `academics.manage` |
| POST | `/exams/assessments/defaults` | `academics.manage` |
| PUT | `/exams/assessments/weights` | `academics.manage` |
| PATCH | `/exams/assessments/{id}` | `academics.manage` |
| DELETE | `/exams/assessments/{id}` | `academics.manage` |
| GET | `/exams/grading` | `exams.read` |
| PUT | `/exams/grading` | `academics.manage` |
| GET | `/exams/max-marks` | `exams.read` |
| PUT | `/exams/max-marks` | `academics.manage` |
| GET | `/exams/report-cards/pdf` | `exams.read` |
| GET | `/exams/results/section` | `exams.read` |
| GET | `/exams/results/section/report-cards` | `exams.read` |
| GET | `/exams/results/student/{id}` | `exams.read` |
| GET | `/exams/sheet` | `exams.read` |
| PUT | `/exams/sheet` | `exams.manage` |
| GET | `/exams/sheet/template` | `exams.read` |
| POST | `/exams/sheet/upload` | `exams.manage` |
| GET | `/exams/sheets` | `exams.read` |
| POST | `/exams/sheets/{id}/approve` | `exams.approve` |
| POST | `/exams/sheets/{id}/reopen` | `exams.approve` |
| POST | `/exams/sheets/{id}/submit` | `exams.manage` |
| GET | `/data-health` | `students.manage` or `settings.manage` |
| GET | `/export/attendance/absentees` | `attendance.read` |
| GET | `/export/attendance/register` | `attendance.read` |
| GET | `/export/everything` | `backup.manage` |
| GET | `/export/exams/results` | `exams.read` |
| GET | `/export/fees/defaulters` | `fees.read` |
| GET | `/export/fees/vouchers` | `fees.read` |
| GET | `/export/staff` | `teachers.read` |
| GET | `/export/students` | `students.read` |
| GET | `/fees/collections` | `fees.read` |
| DELETE | `/fees/concessions/{id}` | `fees.approve` |
| GET | `/fees/defaulters` | `fees.read` |
| GET | `/fees/heads` | `fees.read` |
| POST | `/fees/heads` | `fees.approve` |
| PATCH | `/fees/heads/{id}` | `fees.approve` |
| POST | `/fees/late-fees/run` | `fees.manage` |
| GET | `/fees/policy` | `fees.read` |
| PUT | `/fees/policy` | `fees.approve` |
| GET | `/fees/receipts/{id}` | `fees.read` |
| POST | `/fees/receipts/{id}/reverse` | `fees.approve` |
| GET | `/fees/structure` | `fees.read` |
| PUT | `/fees/structure` | `fees.approve` |
| GET | `/fees/students/{id}/concessions` | `fees.read` |
| POST | `/fees/students/{id}/concessions` | `fees.approve` |
| GET | `/fees/students/{id}/ledger` | `fees.read` |
| GET | `/fees/vouchers` | `fees.read` |
| POST | `/fees/vouchers/generate` | `fees.manage` |
| GET | `/fees/vouchers/pdf` | `fees.read` |
| GET | `/fees/vouchers/{id}` | `fees.read` |
| POST | `/fees/vouchers/{id}/cancel` | `fees.approve` |
| POST | `/fees/vouchers/{id}/payments` | `fees.manage` |
| GET | `/fees/vouchers/{id}/pdf` | `fees.read` |
| POST | `/fees/vouchers/{id}/waive-late-fee` | `fees.approve` |
| GET | `/health` | public |
| GET | `/hostel/allocations` | `hostel.read` |
| POST | `/hostel/allocations` | `hostel.manage` |
| PUT | `/hostel/allocations/{id}` | `hostel.manage` |
| DELETE | `/hostel/allocations/{id}` | `hostel.manage` |
| GET | `/hostel/board` | `hostel.read` |
| GET | `/hostel/hostels` | `hostel.read` |
| POST | `/hostel/hostels` | `hostel.manage` |
| GET | `/hostel/hostels/{id}` | `hostel.read` |
| PUT | `/hostel/hostels/{id}` | `hostel.manage` |
| DELETE | `/hostel/hostels/{id}` | `hostel.manage` |
| POST | `/hostel/hostels/{id}/rooms` | `hostel.manage` |
| GET | `/hostel/register` | `hostel.read` |
| POST | `/hostel/register` | `hostel.manage` |
| POST | `/hostel/register/{id}/close` | `hostel.manage` |
| GET | `/hostel/residents` | `hostel.read` |
| PUT | `/hostel/rooms/{id}` | `hostel.manage` |
| DELETE | `/hostel/rooms/{id}` | `hostel.manage` |
| POST | `/imports/apply` | `students.manage` or `teachers.manage` or `admissions.manage` or `fees.manage` |
| GET | `/imports/batches` | `students.manage` or `teachers.manage` or `admissions.manage` or `fees.manage` |
| POST | `/imports/batches/{id}/rollback` | `students.manage` or `teachers.manage` or `admissions.manage` or `academics.manage` or `fees.manage` |
| GET | `/imports/fields/{kind}` | `students.manage` or `teachers.manage` or `admissions.manage` or `fees.manage` |
| GET | `/imports/mappings/{kind}` | `students.manage` or `teachers.manage` or `admissions.manage` or `fees.manage` |
| PUT | `/imports/mappings/{kind}` | `students.manage` or `teachers.manage` or `admissions.manage` or `fees.manage` |
| POST | `/imports/parse` | `students.manage` or `teachers.manage` or `admissions.manage` or `fees.manage` |
| POST | `/imports/preview` | `students.manage` or `teachers.manage` or `admissions.manage` or `fees.manage` |
| POST | `/imports/report` | `students.manage` or `teachers.manage` or `admissions.manage` or `fees.manage` |
| GET | `/imports/templates/{kind}` | `students.manage` or `teachers.manage` or `admissions.manage` or `fees.manage` |
| GET | `/inventory/assets` | `inventory.read` |
| POST | `/inventory/assets` | `inventory.manage` |
| PUT | `/inventory/assets/{id}` | `inventory.manage` |
| DELETE | `/inventory/assets/{id}` | `inventory.manage` |
| GET | `/inventory/categories` | `inventory.read` |
| POST | `/inventory/categories` | `inventory.manage` |
| PUT | `/inventory/categories/{id}` | `inventory.manage` |
| DELETE | `/inventory/categories/{id}` | `inventory.manage` |
| GET | `/inventory/items` | `inventory.read` |
| POST | `/inventory/items` | `inventory.manage` |
| GET | `/inventory/items/{id}` | `inventory.read` |
| PUT | `/inventory/items/{id}` | `inventory.manage` |
| DELETE | `/inventory/items/{id}` | `inventory.manage` |
| GET | `/inventory/locations` | `inventory.read` |
| POST | `/inventory/locations` | `inventory.manage` |
| PUT | `/inventory/locations/{id}` | `inventory.manage` |
| DELETE | `/inventory/locations/{id}` | `inventory.manage` |
| GET | `/inventory/movements` | `inventory.read` |
| POST | `/inventory/movements` | `inventory.manage` |
| DELETE | `/inventory/movements/{id}` | `inventory.manage` |
| GET | `/inventory/recipients` | `inventory.read` |
| GET | `/inventory/stock` | `inventory.read` |
| POST | `/inventory/transfers` | `inventory.manage` |
| GET | `/leave/balances` | `leave.read` |
| PUT | `/leave/balances` | `leave.manage` |
| POST | `/leave/balances/carry-over` | `leave.manage` |
| GET | `/leave/requests` | `leave.read` |
| POST | `/leave/requests` | `leave.read` or `leave.manage` |
| POST | `/leave/requests/{id}/decide` | `leave.approve` or `leave.read` |
| POST | `/leave/requests/{id}/withdraw` | `leave.read` |
| GET | `/leave/settings` | `leave.read` |
| PUT | `/leave/settings` | `leave.manage` |
| GET | `/leave/staff-register` | `leave.read` |
| GET | `/leave/students` | `leave.read` |
| POST | `/leave/students` | `leave.manage` |
| POST | `/leave/students/{id}/decide` | `leave.read` |
| POST | `/leave/students/{id}/withdraw` | `leave.manage` |
| GET | `/leave/types` | `leave.read` |
| POST | `/leave/types` | `leave.manage` |
| PUT | `/leave/types/{id}` | `leave.manage` |
| DELETE | `/leave/types/{id}` | `leave.manage` |
| GET | `/library/books` | `library.read` |
| POST | `/library/books` | `library.manage` |
| GET | `/library/books/{id}` | `library.read` |
| PUT | `/library/books/{id}` | `library.manage` |
| DELETE | `/library/books/{id}` | `library.manage` |
| POST | `/library/books/{id}/copies` | `library.manage` |
| GET | `/library/copies` | `library.read` |
| GET | `/library/copies/lookup` | `library.read` |
| PUT | `/library/copies/{id}` | `library.manage` |
| GET | `/library/loans` | `library.read` |
| POST | `/library/loans` | `library.manage` |
| POST | `/library/loans/{id}/renew` | `library.manage` |
| POST | `/library/loans/{id}/return` | `library.manage` |
| POST | `/library/loans/{id}/settle` | `library.manage` |
| POST | `/library/loans/{id}/waive` | `library.manage` |
| GET | `/library/members` | `library.read` |
| GET | `/library/members/{kind}/{id}` | `library.read` |
| GET | `/library/settings` | `library.read` |
| PUT | `/library/settings` | `library.manage` |
| GET | `/library/stock` | `library.read` |
| GET | `/nexus/status` | signed in |
| GET | `/nexus/link` | `settings.manage` |
| GET | `/nexus/link/escrow` | `settings.manage` |
| POST | `/nexus/link/escrow` | `backup.manage` |
| DELETE | `/nexus/link/escrow` | `backup.manage` |
| POST | `/nexus/link/join` | `settings.manage` |
| POST | `/nexus/link/leave` | `settings.manage` |
| GET | `/nexus/link/overwrites` | `settings.manage` |
| POST | `/nexus/link/sync` | `settings.manage` |
| GET | `/parent-app/access` | `parents.manage` |
| DELETE | `/parent-app/access/{guardian_id}` | `parents.manage` |
| POST | `/parent-app/access/{guardian_id}/code` | `parents.manage` |
| GET | `/parent-app/conversations` | `messages.reply` or `messages.view_all` |
| GET | `/parent-app/conversations/{student_id}/{guardian_id}` | `messages.reply` or `messages.view_all` |
| POST | `/parent-app/conversations/{student_id}/{guardian_id}/messages` | `messages.reply` |
| POST | `/parent-app/conversations/{student_id}/{guardian_id}/read` | `messages.reply` |
| GET | `/parent-app/notices` | `notices.manage` |
| POST | `/parent-app/notices` | `notices.manage` |
| PATCH | `/parent-app/notices/{id}` | `notices.manage` |
| DELETE | `/parent-app/notices/{id}` | `notices.manage` |
| POST | `/parent-app/notices/{id}/publish` | `notices.manage` |
| POST | `/parent-app/notices/{id}/withdraw` | `notices.manage` |
| GET | `/parent-app/status` | `parents.manage` or `notices.manage` or `messages.reply` or `messages.view_all` |
| GET | `/reports` | `reports.read` or `attendance.read` or `exams.read` or `timetable.read` or `leave.read` or `admissions.read` or `library.read` or `transport.read` or `hostel.read` or `inventory.read` |
| GET | `/reports/custom` | `reports.read` |
| POST | `/reports/custom` | `reports.read` |
| POST | `/reports/custom/run` | `reports.read` |
| GET | `/reports/custom/sources` | `reports.read` |
| GET | `/reports/custom/{id}` | `reports.read` |
| PUT | `/reports/custom/{id}` | `reports.read` |
| DELETE | `/reports/custom/{id}` | `reports.read` |
| GET | `/reports/{code}` | `reports.read` or `attendance.read` or `exams.read` or `timetable.read` or `leave.read` or `admissions.read` or `library.read` or `transport.read` or `hostel.read` or `inventory.read` |
| POST | `/academic-years/rollover` | `academics.manage` |
| GET | `/academic-years/rollover/preview` | `academics.manage` |
| GET | `/settings/school` | `settings.manage` |
| PUT | `/settings/school` | `settings.manage` |
| GET | `/schools/current` | `school.read` |
| PATCH | `/schools/current` | `school.manage` or `settings.manage` |
| POST | `/setup` | public |
| POST | `/setup/demo` | public |
| GET | `/setup/status` | public |
| GET | `/staff` | `teachers.read` |
| POST | `/staff` | `teachers.manage` |
| GET | `/staff/{id}` | `teachers.read` |
| PATCH | `/staff/{id}` | `teachers.manage` |
| GET | `/staff/{id}/assignments` | `teachers.read` |
| PUT | `/staff/{id}/assignments` | `teachers.manage` |
| POST | `/staff/{id}/login` | `users.manage` |
| PATCH | `/staff/{id}/login` | `users.manage` |
| POST | `/staff/{id}/login/password` | `users.manage` |
| GET | `/guardians` | `students.read` |
| POST | `/guardians` | `students.manage` |
| GET | `/guardians/{id}` | `students.read` |
| PATCH | `/guardians/{id}` | `students.manage` |
| GET | `/students` | `students.read` |
| POST | `/students` | `students.manage` |
| POST | `/students/check-duplicates` | `students.manage` |
| GET | `/students/{id}` | `students.read` |
| PATCH | `/students/{id}` | `students.manage` |
| POST | `/students/{id}/enrol` | `students.manage` |
| POST | `/students/{id}/guardians` | `students.manage` |
| DELETE | `/students/{id}/guardians/{guardian_id}` | `students.manage` |
| GET | `/students/{id}/photo` | `students.read` |
| PUT | `/students/{id}/photo` | `students.manage` |
| DELETE | `/students/{id}/photo` | `students.manage` |
| POST | `/students/{id}/withdraw` | `students.manage` |
| GET | `/classes/{id}/subject-groups` | `academics.read` |
| POST | `/classes/{id}/subject-groups` | `academics.manage` |
| GET | `/sections/{id}/subject-allocations` | `students.read` |
| PUT | `/sections/{id}/subject-allocations` | `students.manage` |
| GET | `/students/{id}/subjects` | `students.read` |
| PUT | `/students/{id}/subjects` | `students.manage` |
| POST | `/subject-allocations/import/apply` | `students.manage` |
| POST | `/subject-allocations/import/parse` | `students.manage` |
| POST | `/subject-allocations/import/preview` | `students.manage` |
| GET | `/subject-allocations/import/template` | `students.read` |
| PUT | `/subject-groups/{id}` | `academics.manage` |
| DELETE | `/subject-groups/{id}` | `academics.manage` |
| GET | `/system/features` | public |
| GET | `/system/locale` | public |
| POST | `/timetable/absences` | `timetable.manage` |
| DELETE | `/timetable/absences/{id}` | `timetable.manage` |
| GET | `/timetable/cover-sheet` | `timetable.read` |
| GET | `/timetable/cover-sheet/pdf` | `timetable.read` |
| PUT | `/timetable/covers` | `timetable.manage` |
| DELETE | `/timetable/covers` | `timetable.manage` |
| GET | `/timetable/current` | `timetable.read` |
| POST | `/timetable/generate` | `timetable.manage` |
| GET | `/timetable/generate/{id}` | `timetable.read` |
| POST | `/timetable/generate/{id}/cancel` | `timetable.manage` |
| POST | `/timetable/lessons` | `timetable.manage` |
| POST | `/timetable/lessons/seed` | `timetable.manage` |
| PUT | `/timetable/lessons/{id}` | `timetable.manage` |
| DELETE | `/timetable/lessons/{id}` | `timetable.manage` |
| GET | `/timetable/mine` | `timetable.read` |
| GET | `/timetable/print` | `timetable.read` |
| GET | `/timetable/rooms` | `timetable.read` |
| POST | `/timetable/rooms` | `timetable.manage` |
| PUT | `/timetable/rooms/{id}` | `timetable.manage` |
| DELETE | `/timetable/rooms/{id}` | `timetable.manage` |
| PUT | `/timetable/rules` | `timetable.manage` |
| GET | `/timetable/setup` | `timetable.read` |
| POST | `/timetable/shifts` | `timetable.manage` |
| POST | `/timetable/shifts/default` | `timetable.manage` |
| PUT | `/timetable/shifts/{id}` | `timetable.manage` |
| DELETE | `/timetable/shifts/{id}` | `timetable.manage` |
| GET | `/timetable/substitutions` | `timetable.read` |
| POST | `/timetable/timetables` | `timetable.manage` |
| GET | `/timetable/timetables/{id}` | `timetable.read` |
| PATCH | `/timetable/timetables/{id}` | `timetable.manage` |
| DELETE | `/timetable/timetables/{id}` | `timetable.manage` |
| POST | `/timetable/timetables/{id}/lock` | `timetable.manage` |
| POST | `/timetable/timetables/{id}/move` | `timetable.manage` |
| GET | `/timetable/timetables/{id}/options` | `timetable.read` |
| POST | `/timetable/timetables/{id}/publish` | `timetable.manage` |
| POST | `/timetable/timetables/{id}/swap` | `timetable.manage` |
| GET | `/timetable/timetables/{id}/swap-options` | `timetable.read` |
| GET | `/transport/assignments` | `transport.read` |
| POST | `/transport/assignments` | `transport.manage` |
| POST | `/transport/assignments/bulk` | `transport.manage` |
| PUT | `/transport/assignments/{id}` | `transport.manage` |
| DELETE | `/transport/assignments/{id}` | `transport.manage` |
| GET | `/transport/routes` | `transport.read` |
| POST | `/transport/routes` | `transport.manage` |
| GET | `/transport/routes/{id}` | `transport.read` |
| PUT | `/transport/routes/{id}` | `transport.manage` |
| DELETE | `/transport/routes/{id}` | `transport.manage` |
| GET | `/transport/routes/{id}/roster` | `transport.read` |
| GET | `/transport/settings` | `transport.read` |
| PUT | `/transport/settings` | `transport.manage` |
| GET | `/transport/students` | `transport.read` |
| GET | `/transport/vehicles` | `transport.read` |
| POST | `/transport/vehicles` | `transport.manage` |
| PUT | `/transport/vehicles/{id}` | `transport.manage` |
| DELETE | `/transport/vehicles/{id}` | `transport.manage` |
| GET | `/users` | `users.read` or `users.manage` |
| POST | `/users` | `users.manage` |
| PATCH | `/users/{id}` | `users.manage` |
| POST | `/users/{id}/password` | `users.manage` |
