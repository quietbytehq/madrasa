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

At the school, leaving the backup key with head office or taking it back needs both
`settings.manage` and `backup.manage`; seeing what head office holds and every release of
the key needs `settings.manage` (docs/spec.md §16.7).

The first administrator, created at first-run setup, holds all three.
