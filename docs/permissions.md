# Permission matrix (Lite, v0.1)

Every endpoint checks a permission in the backend (`crates/api`), never only in the UI. The
built-in roles are created at setup with the grants below (`RoleCode::default_permissions` in
`crates/core/src/permissions.rs`); a school may change a role's grants later, and the grants of
existing roles are extended by migrations when a stage adds a permission.

Legend: ✓ granted by default · — not granted.

| Permission | What it allows | Principal | Admin | Accountant | Teacher |
| --- | --- | :-: | :-: | :-: | :-: |
| `school.read` | See the school profile | ✓ | ✓ | ✓ | ✓ |
| `school.manage` | Edit the school profile | ✓ | ✓ | — | — |
| `settings.manage` | School settings (student number prefix, …) | ✓ | ✓ | — | — |
| `users.read` | List user accounts | ✓ | ✓ | — | — |
| `users.manage` | Create, edit, disable accounts | ✓ | ✓ | — | — |
| `roles.manage` | Change role grants | ✓ | ✓ | — | — |
| `audit.read` | Read the audit log | ✓ | ✓ | — | — |
| `academics.read` | Years, classes, sections, subjects | ✓ | ✓ | ✓ | ✓ |
| `academics.manage` | Edit academic structure, calendar, exam setup, year rollover | ✓ | ✓ | — | — |
| `students.read` | Student directory and profiles (teachers: own sections only) | ✓ | ✓ | ✓ | ✓ |
| `students.manage` | Add, edit, withdraw students; import students and opening balances | ✓ | ✓ | — | — |
| `attendance.read` | Attendance sheets, register, absentees (teachers: own sections) | ✓ | ✓ | — | ✓ |
| `attendance.manage` | Mark attendance (teachers: own sections) | ✓ | ✓ | — | ✓ |
| `attendance.correct` | Change a submitted attendance sheet | ✓ | ✓ | — | — |
| `exams.read` | Marks sheets, results, report cards (teachers: own sections) | ✓ | ✓ | — | ✓ |
| `exams.manage` | Enter and submit marks (teachers: own subjects and sections) | ✓ | ✓ | — | ✓ |
| `exams.approve` | Approve or reopen submitted marks sheets | ✓ | — | — | — |
| `fees.read` | Vouchers, ledgers, defaulters, collections | ✓ | ✓ | ✓ | — |
| `fees.manage` | Generate vouchers, record payments | ✓ | ✓ | ✓ | — |
| `fees.approve` | Concessions, late-fee waivers, reversals, fee setup | ✓ | ✓ | — | — |
| `teachers.read` | Staff list and staff attendance | ✓ | ✓ | — | ✓ |
| `teachers.manage` | Edit staff, assignments, staff attendance; import staff | ✓ | ✓ | — | — |
| `reports.read` | Reports | ✓ | ✓ | ✓ | — |
| `backup.manage` | Backup settings, run, restore, password | ✓ | ✓ | — | — |
| `nexus.admin` | Network hub (Nexus edition only) | — | — | — | — |

Notes

- Teachers are scoped to the sections they are class teacher of or teach a subject in. The
  scope is applied in the API for students, attendance and exams.
- The **Parent** role exists only in the Nexus edition and reads the school profile only.
- The table is checked by a unit test (`permissions::tests::matrix_matches_docs`), so it cannot
  drift from the code without failing CI.
