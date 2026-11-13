# Madrasa Lite — user guide

Madrasa Lite is a desktop School Information System for one school. Everything runs on the
school's own computer; nothing leaves it except the backups you choose to copy elsewhere.
This guide covers the daily work. Press **Ctrl+K** anywhere to search.

## 1. First run and setup

1. Install Madrasa (Windows installer or Linux `.deb`/AppImage) and open it. The first
   screen asks how this PC will be used: **Just this PC** (everything on this computer),
   **This PC is the school server** (keeps the records and lets other PCs on the school
   network connect) or **Connect to a school server**. On the server, or on a single PC, the
   bundled database starts on its own. A client finds the server on the network (or you type
   its address), shows the server's **certificate fingerprint**, and connects once you confirm
   it matches the one under **Settings → Network** on the server. Windows asks once whether
   to allow Madrasa on private networks on the server PC: click Allow. A PC that already has
   Madrasa keeps working on its own; the principal or an administrator changes this under
   **Settings → Network → Change**, after which Madrasa restarts in the new mode.
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

- **Subjects per student** (grades with electives): under Settings → Academics, tick
  **Elective** next to a class subject and define **Subject groups** (for example Pre-Medical:
  Physics, Chemistry, Biology). Then Students → **Subject allocation**: pick the section, set
  each student's group or tick their electives, and **Save**. A student with no allocation
  takes every subject of the class. Marks entry, result sheets and report cards then show only
  a student's own subjects. Allocations can also be imported from an Excel sheet (download the
  section's sheet, fill the Group or Electives column, import, and undo if needed). The
  student's profile has a **Subjects** tab.

- **Section moves**: Students → **Section moves** → pick a class. Tick students and move them
  to another section (roll numbers reassigned or kept), or click **Balance sections** to see a
  proposal that evens out the sections, then apply it. Ticked students are never moved by the
  proposal. Attendance and marks already recorded stay with the student.

- **Consents**: under Settings → Academics → **Consent types**, list what the school asks
  for (photos, trips, data use), who gives it, who needs it (everyone or a class range) and
  whether it is renewed every year. Students → **Consents** shows a section at a time: set
  each student's state (given, refused, withdrawn) and the guardian who gave it, then save;
  the student's profile has a **Consents** tab with the history. Reports list the missing
  consents, the ones expiring with the year, and a form list to send home.

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

- **Examinations → Setup**: the year's **terms** (for example Term 1 and Term 2, with their
  weight in the year) and the **assessment components** of each term (default Bi-monthly
  Test 1 and 2, Mid-year, Final) with weights that total 100% within a term; the maximum
  marks per subject for each class; and the grading scheme (grade bands and pass mark). A
  component is either *entered* (marks are typed in or uploaded) or *calculated* from other
  components of the same term (sum, average, weighted average, best of N). A school that
  works without terms keeps one term for the whole year.
- **Marks entry**: a teacher opens a subject of a section, enters marks (or ticks absent),
  saves, and submits for approval. The principal approves; approved sheets are locked and can
  be reopened by the principal only.
- **Marks from Excel**: on the same screen, **Download Excel sheet** gives a file with the
  students and the maximum marks filled in. Fill the *Marks* column (or `Y` under *Absent*)
  and **Upload filled sheet**. The upload is checked like typed marks: if any row is wrong,
  nothing is saved and the rows are listed; otherwise the marks are saved as a draft to
  submit and approve as usual.
- **Results**: the section's result sheet with totals, percentages, grades, positions and a
  subject analysis. **Report cards** can be printed one by one or for the whole section;
  they show every assessment, the term results when the year has more than one term, the
  weighted year result and the attendance.

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

- **Teacher allocation**: Teachers → **Teacher allocation** shows a grid with sections down
  and subjects across; pick the teacher of each subject and the class teacher of each
  section. Each change is saved at once, and the teaching load of every teacher is listed
  below the grid.

## 7. End of the academic year

Under **Settings → Academic years → Roll over**, Madrasa shows what will happen: every
active student moves up one class (the top class graduates), sections keep their class
teachers, the fee structure and assessment types are copied, and unpaid balances carry into
the new year's first voucher. Tick the students who repeat their class or leave, then confirm.
The old year stays readable.

## 7a. Examination boards

**Examinations → Boards** keeps the results of external boards (BISE, Federal Board, AKU-EB
and others) next to the school's own. Add the board once with its grade bands and pass
percentage, then an exam for the year (the classes that sit it and its papers with maximum
marks), register the candidates (a whole class in one click; roll numbers come from the
student record and can be edited), and when the board publishes its results import the
file: Madrasa matches the columns, checks every row (unknown roll numbers, marks over the
maximum) and imports only when the file is clean; an import can be undone. The result sheet
shows totals, grades, pass/fail and positions; the student's profile shows their board
result, and Reports has the board sheet, merit list, pass rates and a per-student result.

## 8. Reports

**Reports** in the sidebar lists the built-in reports by module. Pick one, set its options
(academic year, class, section, status, month or dates) and click **Run report** to see it on
screen, or **Download PDF** / **Download Excel** to get a file in the language the app is
showing. The student reports are: student list, class strength, admissions and
withdrawals, guardian contact list, birthdays and missing data. Fee reports: collection,
monthly collection, defaulters with ageing, a student's ledger, concessions, voucher
register, late fees, billed vs collected by fee head. Attendance reports: monthly register,
attendance by section, absentees, students below a threshold, staff attendance. Exam
reports: result sheet, merit list, subject analysis, grade distribution, students failing,
and the section's report cards as one PDF. Office roles see every report; teachers see the
attendance and exam reports for their own sections. The Dashboard links straight to today's
collection and today's absentees.

**Custom reports**: when the library does not have the list you need, click **New custom
report**, choose the data (students, guardians, fee vouchers, receipts, balances,
attendance, marks or staff), tick the columns, add filters, sort and group, preview and save.
Saved reports run like built-in ones. The principal and administrators can share a report
with everyone who runs reports; other users keep theirs private.


## 9. Timetable (Standard)

Available after Settings → Edition → Standard. Office roles see every tab; teachers see the
timetable and the substitutions.

1. **Setup**: add the morning shift (Mon–Thu 7 periods, Fri 5, a break after period 3) and
   adjust the days, period times and break. Add a second shift only if the school runs an
   afternoon shift. Add rooms if lessons need them. Set the school rules (max periods per
   day and week) and, per teacher, section or room, when they are not available (click a
   period: avoid, then never).
2. **Lessons**: "Seed from allocations" creates one lesson per section and subject from the
   teacher allocation grid (five sessions a week); then adjust sessions per week, make
   doubles, add room pools or joint classes. The Check card says what would stop the
   generator; the Load card shows periods used against the week.
3. **Generate**: give it a name and a time limit (start with 60 s) and generate. Watch the
   progress; the report lists anything left out with a reason and a fix. Publish the
   timetable the school will run on.
4. **Timetable**: show it by section, teacher or room; print it (PDF, English or Urdu).
   The office can click a session to move it to a free slot, swap it or lock it in place.
5. **Substitutions**: record a teacher's absence (whole days or periods). Every session
   they would teach gets suggestions: same subject first, fewest covers so far first.
   Choose the cover or "No cover needed", then print the cover sheet for the day or week.
   Teachers see their own week and cover duties on the Dashboard.

## 10. Leave (Standard)

1. **Leave types** (office): add annual, sick and casual leave with the days per year, how
   many unused days carry into the next year, and whether a request needs approval. Name an
   extra approver if someone besides the principal and admin decides leave.
2. **Staff leave**: a teacher requests their own leave (dates, a half day if needed, the
   reason and where the paperwork is); the office can record leave for anyone. Weekends and
   holidays are not counted. The principal, admin or the approver approves or rejects; an
   approved request marks those days as leave on the staff attendance, and withdrawing it
   unmarks them. Requests beyond the days left, or overlapping another, are refused.
3. **Balances**: what each staff member has per type this year: granted, used, pending and
   available. Adjust a balance with a note; at the end of the year run "Carry over" into
   the new year.
4. **Student leave**: the office records a guardian's note; the class teacher or the office
   approves it. The days become "leave" on every attendance sheet of the range, including
   sheets marked later, and the absentee list says "Leave (approved)". The student profile
   lists the requests.
5. **Staff register**: the month at a glance, one cell per day, with late arrivals and their
   time. Mark "Late" with the arrival time on the daily staff attendance.

## 11. Admissions (Standard)

1. **Stages** (office): the pipeline starts as Enquiry → Applied → Test scheduled →
   Interviewed → Offered → Admitted, with Rejected and Withdrawn. Rename, reorder, add or
   remove stages (empty ones only). Pick the fee head for the admission fee voucher.
2. **Record an enquiry** with the applicant, the class applied for and the guardian's
   contact, or import a list from Excel (Import → Admission enquiries).
3. **Move applications** along the pipeline from the board or the list (tick several to
   move them together). Record the test marks, the interview notes and the offer (fee and
   validity); print the offer letter in English or Urdu.
4. **Convert** an admitted application: choose the section and roll number; the student,
   the guardian and the enrolment are created in one step and the admission fee voucher
   is raised when an offer fee and a fee head are set. The application keeps a link to
   the student.
5. **Reports**: the admissions funnel, applications by class and pending tests.

## 12. Library (Standard)

1. **Settings** (librarian or office): loan limits, loan days, fine per day and renewals for
   students and for staff; the fee head to use when a student's fine is settled (leave it
   empty to settle fines at the desk only).
2. **Catalogue**: add a book with its copies, one accession number per line (the number
   printed on the copy or its barcode), and the shelf. Edit a copy's shelf and condition
   later; withdraw copies that leave the library.
3. **Desk**: find the borrower (student or staff), scan or type the accession number and
   press Issue. The same lookup shows a copy that is out, with Return, Renew and Mark lost.
   The fine appears on return; waive it with a reason or settle it.
4. **Loans**: open, overdue, returned and fines due, with return, waive and settle on
   each row.
5. **Stock**: counts by category and every copy; after a stock take set missing copies to
   Lost and discarded ones to Withdrawn.
6. **Reports**: overdue books, fines, most borrowed titles and stock by category.

## 13. Transport (Standard)

1. **Vehicles** (office): registration, seats, driver and helper with phones. The seat
   count drives the warning when a route carries more students than it can seat.
2. **Routes**: a route has a vehicle, a default monthly fee and its stops in pickup order,
   each with a time and an optional fee of its own. Reorder stops with the arrows; a stop
   with students on it cannot be removed until they are moved.
3. **Assign**: pick the year and a section, tick the students, choose the route and the
   stop and the date they start, then press Assign. A student who changes stops keeps the
   history: the previous assignment ends the day before. End a student's transport from
   the row when they stop using it.
4. **Settings**: choose the fee head for the monthly charge. From then on, every voucher
   generated for a student on a stop carries the transport line (the student's own amount
   if set, else the stop's, else the route's).
5. **Roster**: one route by stop, with student numbers, classes and guardian phones; print
   it for the driver. Reports: route manifests, students without transport, transport fee
   by route for a month.

## 14. Hostel (Standard)

1. **Hostels** (office): name, kind, warden, the default monthly fee and the fee head the
   charge is posted under. Tick "Leave-outs need the guardian's consent" to make the
   register ask for it. Add rooms with their floor, bed count and an optional fee of their
   own.
2. **Board**: every room with its beds. Click a free bed, find the student, set the date
   (and an amount when the student pays something else) and save. Move a resident to
   another bed from their row; End closes their stay today. A student who moves keeps the
   history: the previous allocation ends the day before.
3. **Residents**: everyone in the hostel this year, with room, bed and monthly charge.
   Every voucher generated for a resident carries the hostel line under the hostel's fee
   head (the resident's own amount if set, else the room's, else the hostel's).
4. **Register**: record a leave-out (expected back, the guardian's consent and how it was
   given) or a visitor (name, relationship, phone), and the return or departure. Open
   entries stay on top; a resident not back by the expected time shows Overdue.
5. **Reports**: hostel occupancy by room, residents by class, residents with fees
   outstanding.

## 15. Inventory (Standard)

1. **Setup** (store keeper or office): categories to group items, and locations where
   stock is kept (the main store, labs, rooms, offices).
2. **Items**: name, code, unit and a reorder level; tick "Durable item" for things that
   get an asset tag (projectors, computers). The list shows stock on hand and flags items
   at or below their reorder level.
3. **Record movement**: a purchase in (with the unit cost and bill number), an issue to a
   staff member, student or room, a return, a write-off, an adjustment, or a transfer
   between two locations. Stock at a location can never go below zero; an issue always
   names who received the goods.
4. **Stock**: what is on hand per item and location. **Movements**: the register with
   filters by kind, location, dates and text; a wrong entry can be removed while the stock
   is still there.
5. **Assets**: one row per tagged item with its serial number, status, location and who
   has it.
6. **Reports**: stock on hand, below reorder level, stock movements in a period.

## 16. Directory login (Standard)

1. **Settings → Directory login** (settings administrator): switch it on, enter the server
   (`ldaps://…`, TLS is required), the bind account that looks users up and its password
   (stored sealed and never shown again), the base DN where staff accounts live and, if
   needed, the filter and attribute names (the defaults suit Active Directory).
2. **Groups to roles**: add a row per directory group and the role it grants; the first
   matching group wins. Choose a role for people in no mapped group, or leave it empty to
   refuse them.
3. **Test connection**, optionally with a username, before saving: the message says whether
   the bind account worked, who was found and the role they would get.
4. **Logging in**: staff type their directory username and password on the normal login
   screen (the hint you set appears under the form). Their account appears in Settings →
   Users with a "Directory" badge the first time they log in; the role follows the groups on
   every login. The setup administrator and any account created in Users keep their local
   password.

## 17. Read-only API (Standard)

1. **Settings → API** (settings administrator): create a key for each system that needs
   to read school data: give it a name, tick the scopes it may read, optionally the
   addresses it may call from and an expiry date. The key is shown once; copy it into the
   other system. Revoke a key the moment it is no longer needed; every call made with it is
   listed under Calls.
2. **Calling**: the other system sends the key as `Authorization: Bearer mk_…` (or
   `X-Api-Key`) to `https://<server>:<port>/api/ro/v1/…`. The endpoint reference on the
   card lists every path with its scope and parameters and gives `curl` examples; the
   OpenAPI document is at `/api/ro/v1/openapi.json`. Answers carry an ETag; sending it
   back as `If-None-Match` returns 304 when nothing changed. Each key may make 120
   requests a minute.
3. Nothing can be written through this API, and it does not exist on Lite installs.

## 18. Parent app (schools joined to a school network)

Parents can see their own children's attendance, approved results, fees and the school's
notices on their phones, and write to the class teacher, with the **Madrasa Parent** app.
The app works through the school network (Madrasa Nexus): once the school has joined one
(Settings → School network), **Parent app** appears in the sidebar for the people who have
something to do there.

- **Access** (administrators and the principal). *Give access* → find the guardian → a
  one-time code is shown on a slip. Print it and hand it to the guardian in person: anyone
  with the code can open access to that guardian's children. The code works once, for 14
  days, and is not shown again. The list shows who has access, the last digits of the phone
  number they sign in with and when they last signed in. *New code* is for a forgotten
  password or a new phone number. *End access* signs the guardian out and closes their
  access to this school.
- **Notices** (administrators and the principal). Write a notice for the whole school or
  for chosen classes, with a last day if it should disappear by itself. Save it as a draft
  or publish it; a published notice can be withdrawn.
- **Messages.** A parent writes to the class teacher of their child. The class teacher sees
  a count beside *Parent app*, opens the conversation and answers. Only the class teacher
  of that section answers; the principal can read every conversation. Messages are not
  instant: they travel with the sync, every few minutes while the school is connected.
- **What parents see.** Only their own children. Only results from marks sheets that have
  been approved. Fees as on the school's vouchers; nothing is paid in the app. What they
  see is as fresh as the school's last sync, and the app says when that was.

## 19. Working offline

Madrasa is built so that no school loses work when the network or the school server drops.

### The connection badge

At the top of every screen, next to the bell:

- **Connected** (green): the school server answers; changes are saved there at once.
- **Reconnecting…** (amber): it stopped answering a moment ago; Madrasa keeps trying.
- **Offline** (red): it has not answered for 15 seconds or more.
- **3 waiting**: changes made on this PC that the server has not received yet.
- A red number: something to read (a change the server refused, a value overwritten, a
  payment to check).
- **Head office: in step / 4 to send / needs attention / not reached**: when the school is
  joined to a school network and you may see the network settings.

Click the badge for the details: what is waiting, what happened to it, your drafts, and a
**Try now** button.

### On a PC connected to the school server

When the server or the network goes down:

- **Keep working.** Screens show what they last showed (a copy kept on this PC).
- **Saving keeps the change on this PC**, encrypted, and says "Saved on this PC". The badge
  counts it. Changes are sent on their own, in the order you made them, as soon as the server
  answers, even if Madrasa was closed and opened again or the PC restarted in between.
- **Nothing is applied twice.** Pressing Save again, or a connection that drops half way,
  never makes a second student, receipt or mark.
- **Some things need the server there and then**: signing in, backups and restores, imports,
  setup and network settings. Madrasa says so instead of keeping them.
- **If your sign-in ends while the server is away**, the badge says **Sign in again**. The
  waiting changes go when you sign in again as the same person; they are never sent under
  anyone else's name.

Do not uninstall Madrasa or delete its data folder while the badge shows changes waiting.
Before replacing a PC, connect it to the server until the badge shows nothing waiting.

### Fee receipts taken offline

Record the payment as usual. Madrasa prints a **provisional receipt** at once, numbered like
`P-K7QM-0001` and clearly marked **Provisional receipt — not yet confirmed by the school
server**. Give it to the parent.

When the server is back, the payment gets its real receipt number (for example
`R2026-000123`). The badge's panel shows which provisional number became which, the
provisional receipt page shows the real number, and the real receipt says "Replaces
provisional receipt P-K7QM-0001", so either number finds it.

If someone paid the same voucher on another PC meanwhile, both receipts stand (the money was
received twice) and the panel asks you to refund or carry forward the overpayment.

### When two PCs changed the same thing

The same rules as head office sync:

- **Different fields of the same record**: both changes are kept.
- **The same field on both PCs**: the value saved on the server stands. Yours is listed under
  **Overwritten on arrival** with both values, so nothing disappears silently.
- **Attendance**: marks are merged student by student; where both PCs marked the same
  student, the later mark stands. A sheet a teacher marked offline never changes marks
  someone else has already saved; the differences are listed.
- **A change the server refuses** (for example, the student was withdrawn meanwhile) is
  listed under **Not accepted by the server** with the reason. Check it and enter it again if
  it is still needed.

### Drafts

Every form saves what you type as a draft, encrypted, on this PC (on every PC, the server
included). If Madrasa or the PC stops suddenly, or a window is closed by mistake, open the
same form again: Madrasa offers **Restore**. Drafts belong to the person who typed them,
passwords are never kept, and a draft disappears once its change is saved. The badge's panel
lists your drafts.

### On the server PC, or a single PC

There is no network to lose. The badge shows whether Madrasa's own service answers; if it
does not, close and open Madrasa, and your drafts come back.

### Parent app

The badge at the top of every screen shows whether the school network can be reached.
Offline, the app shows what it last loaded, and a message you were writing is kept as a draft
on the phone.
