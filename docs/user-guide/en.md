# Madrasa Lite — user guide

Madrasa Lite is a desktop School Information System for one school. Everything runs on the
school's own computer; nothing leaves it except the backups you choose to copy elsewhere.
This guide covers the daily work. Press **Ctrl+K** anywhere to search.

## 1. First run and setup

1. Install Madrasa (Windows installer or Linux `.deb`/AppImage) and open it. The bundled
   database starts on its own.
2. The setup wizard asks for the school name, code and country, the academic year's start
   month, the currency and time zone (filled in from the country), the administrator account,
   and a **backup password**. The country also sets the weekend and how ID numbers and phone
   numbers are checked; all of this can be changed later under **Settings**.
3. Write down the **recovery key** shown once at the end and keep it somewhere safe. It opens
   your backups if the backup password is ever lost. Madrasa never shows it again.
4. Log in as the administrator. Under **Settings** add the academic year, classes, sections
   (with a class teacher each) and subjects. Under **Teachers** add staff and assign them to
   sections and subjects; teachers who log in see only their own sections.
5. **Logins for staff**: open a staff member under **Teachers** and use **Create login** in
   the *Login account* box: choose a username, a password (at least 10 characters) and a role
   (principal, administrator, accountant or teacher). The same box changes the role, switches
   the login off and on, and resets a forgotten password. Only the principal and
   administrators can do this, and every change is in the audit log.

Roles: **principal** (everything, approves marks), **admin** (everything except approving
marks), **accountant** (fees), **teacher** (attendance and marks for own sections). The full
list is in `docs/permissions.md`.

## 2. Students

- **Students → Add student**: name, father's name, date of birth, gender, B-Form/CNIC,
  address, admission date, class and section, roll number, and one or more guardians.
  Guardians with the same phone or CNIC are shared, so siblings are linked automatically.
- Madrasa warns about a possible duplicate (same B-Form, or same name + date of birth +
  father's name) before saving.
- The profile page has tabs for details, guardians, fees, attendance and results. A student
  is withdrawn or marked graduated from the profile; records are never deleted.
- **Import from Excel** (sidebar): download the template, fill it (or export from your old
  register), upload it, match the columns, check the rows, fix any errors in the grid or in
  Excel, then import. Every import can be rolled back from the history on the same page.
- **Export to Excel/CSV** is available on every list.

## 3. Fees

- **Fees → Setup**: fee heads (tuition, exam, transport, admission…), the fee structure per
  class for the year, the sibling discount (percent and who gets it), how concessions combine
  with it, the late fee percent and the due day (default 10% after the 15th).
- **Vouchers → Generate vouchers** once a month for the whole school or one class. Unpaid
  balances and late fees carry forward into the next voucher, also across academic years.
- **Collect**: find the voucher and record the payment (cash, bank, online, 1Bill, Kuickpay
  with the reference number). Partial payments are fine. A receipt is issued and can be
  reprinted; corrections are reversals, never edits.
- **Late fees are added on their own.** Every day (and whenever Madrasa starts) the late fee
  is added to each voucher that is still unpaid after its due day, so the dashboard, the
  defaulters list and every ledger show what is really owed. A waived late fee stays waived.
  Each added fee is in the audit log. Set the late fee percent to 0 to switch this off.
- Concessions, late-fee waivers, cancellations and reversals need the admin or principal and
  a reason; everything is in the audit log.
- **Defaulters** lists who owes what and for how long; **Collections** shows a day's takings.
- Print vouchers (school and parent copies) and receipts in English or Urdu; "Save as PDF" in
  the print dialog makes a PDF.

## 4. Attendance

- **Attendance → Mark**: choose the section and date. Everyone is present; mark only the
  absent, late or on-leave students (keyboard: ↑ ↓ to move, P/A/L/V to set, Enter for next).
  Submit once. After that only the admin can correct the sheet, and every change is kept in
  the history.
- No attendance is taken on weekends or holidays; set them under **Calendar**.
- The **Monthly register**, the **Absentees** list (with guardian phone numbers) and the
  student's profile show the history. The dashboard shows today's percentage and which
  sections are still unmarked.
- **Staff** attendance is a simple present / absent / leave per day.

## 5. Exams and report cards

- **Examinations → Setup**: the assessment types of the year (default Bi-monthly Test 1 and
  2, Mid-year, Final) with weights that must total 100%, the maximum marks per subject for
  each class, and the grading scheme (grade bands and pass mark).
- **Marks entry**: a teacher opens a subject of a section, enters marks (or ticks absent),
  saves, and submits for approval. The principal approves; approved sheets are locked and can
  be reopened by the principal only.
- **Results**: the section's result sheet with totals, percentages, grades, positions and a
  subject analysis. **Report cards** can be printed one by one or for the whole section;
  they show every assessment, the weighted year result and the attendance.

## 6. Backup and restore

- Madrasa backs up every night (default 02:00) to **Documents\Madrasa Backups**, or to the
  USB drive or network folder you choose under **Backup**. Copy the newest `.mbk` file off the
  computer regularly; the dashboard warns when the last backup is older than two days.
- **Back up now** makes a backup immediately.
- **Restore**: pick a file, check its date and record counts, enter the backup password (or
  the recovery key) and confirm. Everything is replaced by the backup and everyone logs in
  again. Changed or damaged files are refused.
- Changing the backup password applies to new backups only; old files open with the password
  that was in use when they were made, or with the recovery key.

## 7. End of the academic year

Under **Settings → Academic years → Roll over**, Madrasa shows what will happen: every
active student moves up one class (the top class graduates), sections keep their class
teachers, the fee structure and assessment types are copied, and unpaid balances carry into
the new year's first voucher. Tick the students who repeat their class or leave, then confirm.
The old year stays readable.
