# Testing Madrasa v0.4.1-alpha (Lite, Standard and the Nexus hub)

Thank you for trying Madrasa. This is an early build for a small group of testers. It is
not ready for a school's real records yet: use made-up data, expect rough edges, and tell us
about everything that confuses you. Nothing you do in the app leaves your computer.

**What you need:** a 64-bit Windows 10 or 11 PC, or a 64-bit Linux PC running Debian 12 or
13, Ubuntu 22.04 or newer, Linux Mint 21 or newer, or another distribution from 2022 or later
(the build needs glibc 2.35), about 1 GB of free disk space, and 20–40 minutes. No internet connection is needed after the
download.

## 1. Install

Download the file for your computer from the release page (the "Assets" list):

| Computer | File |
| --- | --- |
| Windows 10/11 (64-bit) | `Madrasa_0.4.1_x64-setup.exe` |
| Debian 12/13, Ubuntu 22.04+, Linux Mint 21+ | `Madrasa_0.4.1_amd64.deb` |
| Any other 64-bit Linux from 2022 or later | `Madrasa_0.4.1_amd64.AppImage` |

### Windows

1. Double-click the `-setup.exe` file.
2. Windows will show a blue **"Windows protected your PC"** box. That is expected: this test
   build is not yet signed with a certificate, so Windows does not recognise the publisher.
   Click **More info**, then **Run anyway**.
3. If your antivirus asks, allow the installer to run.
4. Follow the installer (Next, Next, Finish). Madrasa appears in the Start menu.
5. Start Madrasa. The first start takes a little longer (up to a minute) because it sets up
   its own database on your PC. Later starts take a few seconds.

### Linux (.deb: Ubuntu, Debian, Mint)

Open a terminal in the folder with the downloaded file and run:

```bash
sudo apt install ./Madrasa_0.4.1_amd64.deb
```

Then start **Madrasa** from your applications menu, or run `madrasa` in a terminal.

### Linux (AppImage: any distribution)

1. Right-click the `.AppImage` file → Properties → Permissions → tick **Allow executing
   file as program** (or run `chmod +x Madrasa_0.4.1_amd64.AppImage`).
2. Double-click the file. If nothing happens, open a terminal in that folder and run
   `./Madrasa_0.4.1_amd64.AppImage`; if it complains about FUSE, run
   `./Madrasa_0.4.1_amd64.AppImage --appimage-extract-and-run` instead.

### Where Madrasa keeps its files

Madrasa stores everything in one folder on your PC and nowhere else:

- Windows: `%APPDATA%\madrasa` (paste that into the File Explorer address bar)
- Linux: `~/.local/share/madrasa`

Backups go to **Documents → Madrasa Backups**. To start over from scratch, quit Madrasa and
delete the `madrasa` folder above.

## 2. First run: set up a test school

On a fresh install the first screen asks **how this PC will be used**: *Just this PC*,
*This PC is the school server*, or *Connect to a school server*. For a one-PC test choose
**Just this PC**. (Section 6 covers the other two choices.) A PC upgraded from an earlier
build keeps working as *Just this PC* without asking; the choice is under **Settings →
Network → Change**. The setup wizard follows. Make up a school; do not use a real school's
data.

1. **School**: name (for example "Test School"), a short code (for example `TST`), a city and
   the **country**. The country fills in the currency, time zone, weekend and ID-number
   format on the next step; all can be changed.
2. **Academic year**: the month the year starts (April for Pakistan, August for the US), the
   currency (PKR, USD, GBP…) and the time zone. Leave **Create classes KG–12 with a section A
   each** ticked: the wizard then creates the current academic year and the classes, so you can
   import students straight away.
3. **Administrator**: your name, a username, a password of at least 10 characters.
4. **Backup security**: a backup password (at least 10 characters). It protects the backup
   files Madrasa makes every night.
5. **Recovery key**: Madrasa shows a long key once. Copy it somewhere (a text file is fine for
   testing), tick the box, and go to the login page.

Log in with the administrator username and password you chose.

## 3. Load the demo school (sample data)

**The quick way:** on the wizard's first screen click **Load the demo school instead**,
choose an administrator password and a backup password, and wait a minute or two. You get
"DEMO School" with about 1,200 students, 120 staff and a year of fees, attendance and marks,
and every screen carries a DEMO banner. Do this on a fresh install only; it cannot be mixed
with a real school. The staff logins it creates are listed under Settings → Users.

**The import way** (to try the import wizard itself): the release includes two small
spreadsheets of made-up Pakistani names, `sample-students.xlsx` (72 students in KG–5,
sections A and B) and `sample-staff.xlsx` (8 staff). Download both from the release page,
then:

1. **Settings → Academic years**: the wizard created and activated the current year.
2. **Settings → Classes**: the wizard created KG–12 with section A each (if you unticked that,
   click **Add KG–12** and add sections). Add **section B** to KG, 1, 2, 3, 4 and 5 for the
   active year, since the sample file uses sections A and B.
3. **Fees & Finance → Setup**: under *Fee heads* click **Add common heads** (or add
   "Tuition" yourself), then under the fee structure give each class an amount (for example
   2,500 for KG rising to 4,000 for class 5). This is enough for vouchers later.
4. Sidebar → **Import** → **Students**: choose `sample-students.xlsx`. If a workbook has
   several sheets the wizard first asks which one to read (it preselects a sheet named
   "Students" or "Staff" over a "Read Me" sheet). The wizard should match every column on its
   own, including "Guardian 1 …", "Guardian 2 …" and "Family ID" columns if your file has
   them; the preview should say **72 rows to create, 0 errors**. Click **Import**.
5. **Import** → **Staff**: choose `sample-staff.xlsx`; 8 rows to create. Import.

If the preview shows errors, that is worth reporting (see section 5), with a screenshot of
the preview.

## 4. Things to try

Work through as many as you like, in any order. Each one is something a school office does.
Note anything that was unclear, slow, wrong or ugly.

1. **Find a student.** Press **Ctrl+K** (or use the search box at the top) and type part of a
   name. Open the student. Add a photo if you have any picture to hand.
2. **Add a student by hand**, with a father/guardian. Then add a sibling for the same guardian
   (search for the guardian by phone when adding the second child).
3. **Change the fee rules.** Fees & Finance → Setup → Policy: set a sibling discount of 10%
   and a late fee of 10% after the 15th. On a student's page, give them a 50% scholarship
   with a reason.
4. **Generate this month's vouchers** (Fees & Finance → Vouchers → Generate). Open a voucher for a
   student who has a sibling and check the sibling discount; check the scholarship on the
   other student. Print a voucher (use "Save as PDF" in the print dialog).
5. **Collect fees.** Record a full payment on one voucher and a partial payment on another.
   Print a receipt. Then reverse one receipt with a reason and see the ledger.
6. **Defaulters.** Fees & Finance → Defaulters: set the date to next month to see who still owes.
   Try the Excel export button.
7. **Mark attendance** for one section today (everyone present by default; mark two absent,
   one late). Try to mark attendance for a Sunday: it should be refused as a weekend.
8. **Add a holiday** (Attendance → Calendar), then look at the attendance register for the
   month.
9. **Enter marks.** Examinations → Setup: click **Add default assessments** (the exam types
   and their weights for the year). Then enter marks for one subject in one section, submit
   the sheet, and approve it (the administrator can).
10. **Print a report card** (Examinations → Report cards) for a student in that section.
11. **Make a backup** (Backup → Back up now) and look at the file in Documents → Madrasa
    Backups. Then change a student's name, **restore** that backup, and check the name is
    back. You will need the backup password you set in the wizard.
12. **Switch to Urdu** (the **Language** switch in the top bar) and repeat one of the tasks above
    in Urdu; check the layout reads right-to-left and nothing is cut off. Switch back.
13. **Dark mode and a small screen.** Use the **Theme** switch in the top bar to turn on dark
    mode, and if you can, try the app on a 1366×768 laptop screen or a window that size.
14. **Create a teacher login.** Teachers → pick a staff member → in the **Login account** box
    click **Create login**: choose a username, a password and the Teacher role. Assign them a
    section and subject in the same dialog. Log out and log in as them: they should be able
    to enter marks for that section but see no Fees or Settings. Back as the administrator,
    try **Reset password** and the **Can log in** switch.
15. **End of year (preview only).** Settings → Academic years → **Roll over year**: read the
    preview of who moves where, but you can cancel without applying it.
16. **Marks from an Excel sheet.** Examinations → Marks entry → open a subject of a section
    and click **Download Excel sheet**. The file has the students, their numbers and the
    maximum marks filled in. Type marks into the *Marks* column (put `Y` under *Absent* for an
    absent student), save, and click **Upload filled sheet**. Try once with a mark above the
    maximum or a letter in the marks column: nothing should be saved and the message should
    name the row. Fix it, upload again, then submit and approve the sheet as in task 9.
18. **Reports.** Open **Reports** in the sidebar. Run **Student list** for one class, then
    **Class strength**, **Guardian contact list**, **Birthdays** for this month and **Missing
    data**. Download one as **PDF** and one as **Excel**, then switch the app to Urdu and
    download the PDF again: the Urdu one should read right-to-left in Nastaliq with the same
    figures. Try **Print** on the on-screen table.
19. **Fee, attendance and exam reports.** In Reports, run **Collection (receipts)** for
    today, **Defaulters by class**, a **Student fee ledger** (type a student number from a
    voucher), the **Monthly register** for one section, **Attendance by section**, a
    **Result sheet** and **Report cards** for a section (download the PDF: one page per
    student). Then log in as a teacher: Reports should show only attendance and exam reports
    for their own sections. On the Dashboard, click **Today's collection report** and
    **Today's absentees report**.
20. **Build your own report.** Reports → **New custom report**: pick *Students*, tick a few
    columns, add a filter (for example Gender is Female), group by Section, click **Preview**,
    then name it and **Save report**. It should appear under *Custom reports* and download as
    PDF and Excel. As the principal, tick **Share with everyone who can run reports** and
    check the accountant can see and run it but not edit it.
21. **Board results.** Examinations → **Boards**: the demo school has BISE Karachi with two
    exams. Open *SSC Part I* → **Results** to see the sheet; try **Candidates** and change a
    roll number. Add your own board and exam (two or three papers), register a class, then
    import a results file: make a small Excel sheet with a *Roll No* column and one column
    per paper (type ABS for an absent student and one mark above the maximum), upload it,
    check the column matching, click **Check** (the bad mark should be flagged), fix the
    file, **Import**, and then **Undo this import**. Reports → *Board result sheet* and
    *Student board result* should show the same figures; the student's profile → Results
    shows the board result under the report card.
22. **Terms and calculated components.** Examinations → **Setup** → *Terms and assessment
    components*: the demo year shows one term *Year* holding the four assessments, and
    results are unchanged. Add a term *Term 2*, set the weights in the year (for example
    60 and 40) and **Save term weights**; move *Final* to *Term 2* with its **Move to**
    select and set each term's component weights to total 100. Click **Add calculated
    component**, name it *Best test*, choose *Best of N* with N = 1, tick *Bi-monthly Test 1*
    and *Bi-monthly Test 2*, and **Save**. The component appears as *Calculated*, does not
    appear under Marks entry or Maximum marks, and the Results tab shows its figures for a
    section. A report card should now show two term headings and a *Term result* row. On a
    new academic year, *Add default assessments* offers one or two terms.
23. **Subjects per student.** Settings → Academics → **Classes** → open *9*: Physics,
    Chemistry, Biology and Computer are ticked **Elective** in the demo school, and the
    **Subject groups** card lists *Pre-Medical* and *Computer Science*. Students → **Subject
    allocation** → pick *9 A*: most students have a group; change one student's group, untick
    an elective for another, clear a third (back to *All subjects*) and **Save allocations**.
    Open that student's profile → **Subjects**. Then Examinations → Marks entry for *9 A* →
    *Biology*: only the students who take Biology are listed, and the Results tab shows "—"
    for the others. Finally **Download Excel sheet** on the allocation page, change a group
    name in the file, **Import from Excel**, **Check**, **Import**, and **Undo this import**.
24. **Allocation screens.** Teachers → **Teacher allocation**: filter by a class, change the
    teacher of a subject in one section and the class teacher of another; the teaching load
    list below should update. Then Students → **Section moves** → pick *1*: tick two students
    in *A*, choose *B* under **Move to**, keep or reassign roll numbers, and **Move**. Click
    **Balance sections**: the dialog lists the proposed moves and the sizes after; tick a
    student first to keep them where they are, then **Apply moves**. Attendance and marks
    already recorded for a moved student still show on their profile.
25. **Consents.** Settings → Academics → **Consent types**: the demo school has *Photos*,
    *Trips* (every year) and *Data use*; add one for a class range (for example *Lab safety*,
    levels 9–12). Students → **Consents** → pick *9 A*: the badge shows how many consents are
    still to collect; set a few cells to *Given* or *Refused* (pick the guardian) and **Save
    consents**. Open a student's profile → **Consents** to see the states and the history,
    and record a *Withdrawn*. Reports → *Missing consents*, *Consents expiring with the year*
    and *Consent form list* should match what you see on screen.
26. **v0.2.1 fixes.** On a demo school every screen shows the yellow DEMO banner, and the
    demo year is the current one with fees and attendance up to today. Open a voucher and
    click **Print / Save as PDF**: the PDF shows the currency on every amount and the
    payment block from Settings → School → *Payment reference on vouchers* (edit the heading
    and print again). Print a report card the same way. In Examinations → Setup, add a
    calculated *Average* component: the **Absences** choice defaults to *Skip the absent
    paper*. As a teacher, Backup must not open. Settings → Academic years → roll over: every
    class keeps its sections, including Kindergarten.
17. **Several PCs at once** (needs two or more PCs on the same network; see section 6).
    Log in on each PC with a different account at the same time and enter marks or attendance
    for different sections. Each person should see only their own sections and modules, and
    nobody should be logged out or see someone else's work.

## 5. Standard edition: the modules a bigger school needs

The Standard modules are switched on in **Settings → Edition** (principal or administrator).
The demo school already holds data for every one of them, so the quickest way to look is:
load the demo school (section 3), log in as the principal, open Settings → Edition and choose
**Standard**. The sidebar grows at once; nothing you did before changes.

27. **Timetable.** Timetable → the demo year has a morning shift, six rooms and lessons
    seeded from the teacher allocations; a timetable is generated and published. Open a
    section's grid and a teacher's grid, print one (Print), then **Setup** → change a
    lesson (give it a second teacher) and **Generate** again; read the check findings before
    the run. Record an absence under **Covers** and see the free teachers proposed.
28. **Leave.** Leave → **Requests**: approve the pending request as the principal, then
    reject one with a note. Open Teachers → attendance for the approved days: they show as
    *Leave* and cannot be changed by hand. **Balances** must have gone down.
29. **Student leave.** Students → a grade 9 student → **Leave** tab: record two days of
    leave; mark the section's attendance for those days — the student is on leave already.
30. **Admissions.** Admissions → the board shows twelve applications across the stages.
    Drag one from *Applied* to *Test scheduled* (or use Move), record the test marks, move
    to *Offered* with a fee, print the offer letter in Urdu, then move to *Admitted* and
    **Convert**: pick section 6-A. The new student appears in Students with a 5,000
    admission voucher. Import → *Admission enquiries* with a small Excel sheet.
31. **Library.** Library → **Desk**: find a student, scan an accession number (the demo
    copies are `DEMO-001-1` and so on), issue, then look up a copy that is out and return it;
    some demo loans are overdue, so a fine appears — waive part of it with a reason, settle
    the rest. **Settings** → pick a fee head and settle another student's fine: a voucher
    line appears on the student's account. **Stock** → change a copy to Lost. Reports →
    Overdue books in Urdu.
32. **Transport.** Transport → **Routes**: open *North route*, reorder its stops, add one.
    **Assign** → section 6-A → tick three students, put them on the new stop from today.
    **Roster** → print it. Fees → generate next month's vouchers for 6-A: each rider's
    voucher carries the transport line (1,500, or 1,200 on the last stop).
33. **Hostel.** Hostel → **Board**: put a student in a free bed, move another to a
    different room, end one stay. **Register**: record a leave-out without consent (refused),
    then with consent, then the return. Generate vouchers for 9-A: residents carry the hostel
    line (8,000, or 9,000 on the first floor).
34. **Inventory.** Inventory → **Items**: *Chalk (box)* is flagged for reorder. Record a
    purchase, an issue to a staff member (search their name), a transfer from the main store
    to the science lab, and try to issue more than is in stock (refused). **Assets**: move
    projector PJ-002 to *Under repair*. Reports → Below reorder level.
35. **Store keeper and librarian logins.** Settings → Users: give a staff member the
    *Store keeper* role and another the *Librarian* role (both exist only on Standard), log
    in as each and check they see only their module plus the directories.
36. **Directory login** (needs an Active Directory or LDAP server reachable over TLS).
    Settings → Directory login: enter the server, bind account, base DN and a group-to-role
    row, **Test connection** with a username, save with the switch on, then log in with that
    person's directory password. The account appears in Users with a *Directory* badge.
    Wrong password five times locks it for 15 minutes like a local account.
37. **Read-only API.** Settings → API: the demo school has a key named *Demo portal*
    whose secret is documented in docs/PROGRESS.md (demo installs only). From another PC on
    the network run the `curl` examples on the card against the server address; create a
    key limited to that PC's address and an expiry date, revoke it, and check the call log.
38. **Back to Lite.** Settings → Edition → *Lite*: the sidebar shrinks to the Lite modules,
    `/api/ro` stops answering, directory logins stop, and every Lite screen is unchanged.

## 6. Several PCs: one school server, many clients

A school with many teachers keeps the records on one PC (the **server**) and lets the other
PCs (the **clients**) work on it over the school's own network (Wi-Fi or cable). Nothing goes
over the internet; the PCs only need to be on the same local network.

### The server PC

1. Install Madrasa on the PC that will keep the records (ideally one that stays on during
   the school day and is backed up). On the first screen choose **This PC is the school
   server**. Leave the port at `8471` unless another program uses it, and click **Start as
   the school server**. Then run the setup wizard as in section 2 (or load the demo school).
   On a PC that already has Madrasa and a school set up, log in as the administrator, open
   **Settings → Network**, click **Change**, pick **This PC is the school server** and click
   **Start as the school server**: Madrasa restarts in the new mode and the records stay as
   they are. The same card switches back to *Just this PC* or to a client later.
2. **Windows Firewall:** when Windows asks whether to allow Madrasa on private networks,
   click **Allow access**. If that prompt never appeared or a client cannot connect, open a
   Command Prompt **as administrator** on the server PC and run:

   ```
   netsh advfirewall firewall add rule name="Madrasa" dir=in action=allow program="C:\Program Files\Madrasa\Madrasa.exe" enable=yes profile=private
   ```

   (The school's network must be marked *Private*, not *Public*, in Windows network settings;
   on a Public network Windows blocks everything.) On Linux, allow TCP port 8471 and UDP port
   47121 in `ufw` or `firewalld` if one is enabled.
3. Log in as the administrator and open **Settings → Network**. It shows the server's
   address (for example `192.168.1.10`), the port and a long **certificate fingerprint**
   (`AB:12:…`). Leave this screen open or write the fingerprint down: clients compare it on
   their first connection so that nobody can slip a different PC in between.
4. Create a login for each teacher (task 14) so that every person uses their own account.

### Each client PC

1. Install Madrasa and choose **Connect to a school server** on the first screen.
2. The app searches the network and lists the servers it finds. Click the school server, or
   type its address (`192.168.1.10` or `office-pc:8471`) and click **Check**.
3. Compare the fingerprint shown with the one on the server's **Settings → Network** screen.
   If it matches, tick **The fingerprint matches the server's screen** and click **Connect**.
   If it does not match, stop and tell us.
4. Log in with that person's own account. From here on the app works exactly as on the
   server; the data stays on the server PC. **Settings → Network** on a client shows which
   server it uses. (A PC that already had Madrasa can become a client through **Settings →
   Network → Change** too; its own records are kept on disk but not used.)

Things to try: teachers logging in on several PCs at once (task 17); switching the server PC
off and seeing what a client shows (it should say the server cannot be reached, not crash);
restarting the server and continuing. If the server is ever reinstalled from scratch its
fingerprint changes and clients will refuse to connect until they are set up again: that is
intended.

## 7. Madrasa Nexus: several schools and a head office

Madrasa Nexus joins schools into one network. Each school keeps working on its own install
and keeps all its data; a **hub** server at head office holds a synced copy of each school,
kept apart from every other school's, and shows network-wide reports. A school that never
joins a hub is unchanged: everything in sections 1 to 6 works as before.

This section is for a tester who is comfortable with a terminal. **You need** a PC with
Docker and the Compose plugin for the hub, this repository checked out on it, and one or two
Madrasa installs to act as schools (the demo school of section 3 is fine).

### Start a hub

No hub image has been published yet, so build it from the checkout (about ten minutes the
first time):

```bash
cd deploy/nexus
```

```bash
cp .env.example .env
```

Edit `.env` and set `NEXUS_DB_PASSWORD` (letters and digits) and `NEXUS_SETUP_TOKEN` (any
long text you choose). Then:

```bash
docker compose build nexus
```

To look around with made-up schools first, fill the empty hub with the demo network (three
schools; one bills in another currency):

```bash
docker compose run --rm nexus demo-network --yes
```

```bash
docker compose up -d
```

Open <http://127.0.0.1:4780> on the hub PC.

### Things to try

39. **First run.** The first visit shows the setup page. Paste your setup token, name the
    network, create the administrator, and sign in. Reload the page: setup is gone for good.
    Switch the console to Urdu and back.
40. **The demo network.** Schools lists three demo schools. Open Reports: run *Network
    summary*, then the fees report. The fees totals are one line per currency and are never
    added together. Download a report as Excel and open it.
41. **A school joins.** In the console: Schools → *Invite a school*. Copy the join code. In a
    Madrasa install **on the same PC as the hub**, log in as the administrator, open Settings
    → School network (Nexus), enter `http://localhost:4780` and the code, and join. The first
    upload starts at once; press *Sync now* until nothing is waiting. The school appears in
    the console with its last sync time. *(A school on another PC needs the hub on an
    `https://` address: the app refuses to send records over plain `http://` except to the
    same PC. See `docs/nexus/deploy.md` for the HTTPS profile.)*
42. **A correction from head office.** Console → Corrections: choose the school, find a
    student, correct the spelling of the name. In the school, press *Sync now* and open the
    student: the name has changed.
43. **Both sides change the same thing.** Without syncing in between: change a student's name
    in the school, and correct the same student's name in the console to something else. Sync.
    The school now shows head office's name, and Settings → School network lists the school's
    own value under *Overwritten values*. The console shows the same entry when you open the
    school. Then change two different details of one student on the two sides: both survive.
44. **The hub is away.** `docker compose stop nexus`. Keep working in the school (add a
    student, take a payment): nothing is blocked; the Nexus card says the hub could not be
    reached. `docker compose start nexus`, press *Sync now*: everything arrives.
45. **Backup key with head office.** Console → Backup keys: set up escrow with a passphrase
    (write it down). In the school (it must have been set up with a backup password, section
    2): Settings → School network → *Leave the key with head office*, with the backup
    password. Make a backup (Backup → Back up now). In the console, release the school's key
    with a reason: a code is shown once. Sync the school: the Nexus card now says head office
    opened the key, who and why. In the school: Backup → Restore → *Use a key released by
    head office instead*, paste the code, restore. Try the code with one character changed:
    it is refused before anything happens.
46. **Head office accounts.** Console → Users: add a user who may only read reports. Sign in
    as that user: only Schools and Reports are there. Try
    `http://127.0.0.1:4780/users` in the address bar: the page is not offered.
47. **Leaving.** In the school: Settings → School network → *Leave the network*. Every record
    is still in the school and it works as before. In the console the school is *Left* and is
    in no report. Invite it again and join: it uploads into a fresh copy. Then, in the
    console, remove a school (it is told why at its next sync and unlinks itself, keeping its
    data), and delete the archive of a school that has gone by typing its code.
48. **Two real schools.** With two installs joined, run the reports again and check that
    each figure matches what each school's own reports say, and that nothing of one school
    shows up in the other.

### Not tested by us: a real cloud deployment

The Azure template and the Google Cloud module in `deploy/nexus` are checked by automated
tests for mistakes, but **nobody has deployed them**: no cloud account was available. If you
have one, follow `docs/nexus/deploy.md` (Azure or Google Cloud), then repeat tasks 39 to 43
against the cloud address with a school on another PC, and tell us exactly what happened,
including anything the cloud provider refused. Two things will need doing first: publishing
a release (so the image exists) and making the image package public on GitHub.

Also not tried by hand: schools on other PCs over real HTTPS (the automated tests use a
local address), and receipts or attendance entered for the same day on both sides (covered
by automated tests only, because the console has no screens for entering them).

## 8. The parent app

*New since v0.4.1-alpha: it needs a build from `main`, or the next release.*

Parents use the **Madrasa Parent** app on a phone to see their own children's attendance,
results, fees and the school's notices, and to write to the class teacher. The app talks to
a Nexus hub, so this section builds on section 7: you need a running hub. A school that has
not joined a hub has no Parent app area and is unchanged.

There are two ways to open the app:

- **In a browser on the hub PC, at phone size.** This is what we tried, and it works with
  the hub of section 7 as it is. You need Node 22 and pnpm, and this repository checked out:

  ```bash
  pnpm install
  ```

  ```bash
  pnpm dev:parent
  ```

  Open <http://localhost:1440>, then your browser's device toolbar (F12, then the phone
  icon) and choose a phone.
- **On an Android phone**, with the package from the release page (`madrasa-parent-….apk`)
  or from a CI run (artifact `madrasa-parent-apk`). Read "On a phone" below first: the hub
  must be on an `https://` address, and **we have not been able to try this ourselves**.

### Things to try

49. **A demo parent.** With the demo network of section 7 loaded, ask the hub for parent
    access codes (the demo-network command also prints them when it is first run):

    ```bash
    docker compose run --rm nexus demo-parent-codes
    ```

    Each line is a code, a demo school and a made-up guardian with their children. In the
    app choose *I have an access code*, enter `http://localhost:4780` and one code. The app
    shows whose code it is before you choose a phone number (any number) and a password (at
    least 10 characters). You arrive at *My children*.
50. **One child.** Open a child. Home shows the class, the class teacher, attendance, the
    amount due, notices and messages. Go through the six sections at the bottom. In
    Attendance move back to April and May (the demo school's records are there). In Results
    look at the marks, totals, grade and position. In Fees open *Details* on a voucher. Read
    the notices. In Messages there is a short conversation with the class teacher: write an
    answer.
51. **Urdu.** Go back to *My children* and switch to اردو. Go through the same screens:
    everything reads right to left, and class names such as 4-A, amounts and dates stay
    readable. Tell us about any wording that sounds wrong.
52. **Sign out and in.** Sign out; sign in again with the phone number and password. Try a
    wrong password. Use the same code again in *I have an access code*: it is refused (a
    code works once).
53. **A real school gives access.** Join a Madrasa install to the hub (task 41). A new
    **Parent app** entry appears in the school's sidebar. Open it, then *Access* → *Give
    access*: find a guardian by name, get the code. It is shown once, on a slip: print it
    (or print to PDF) and check that only the slip prints. Use the code in the app.
54. **Notices.** In the school: Parent app → Notices → *New notice*, for the whole school;
    publish. Press *Sync now* (Settings → School network) or wait five minutes: the notice is
    in the app. Write one for a single class and check that a parent of another class does
    not get it. Withdraw a notice: after the next sync it is gone from the app.
55. **Messages, both ways.** In the app, write to the class teacher. Log in at the school as
    that child's class teacher (in the demo school: the class teacher's username from Staff,
    password `demo-password-1`): after a sync the sidebar shows a count beside Parent app.
    Open the conversation and answer. In the app press *Refresh*: the answer is there. Log
    in as the principal: every conversation can be read, none can be answered.
56. **Only approved results.** In the school, enter marks for a new assessment and leave the
    sheet unapproved; sync. The app does not show those marks. Approve the sheet; sync. Now
    it does, and the figures match the school's own report card for that student.
57. **Only your own children.** Give a second guardian (another family) access and use that
    code in a private browser window. Each parent sees only their own children. Fees and
    results of one family must never appear for the other.
58. **Access ends.** In the school: Parent app → Access → *End access* for a guardian. In
    the app that parent's child is no longer available and *My children* is empty. Give a
    *New code* and use it with the same phone number and a new password: access is back
    (this is also how a forgotten password is handled).
59. **The school goes quiet.** Stop syncing for a while (close the school's app). The app
    keeps showing the last records with "Last updated by the school" and, after a day, a
    warning that they may be behind.

### On a phone

The package installs on Android 7.0 or newer (allow "install unknown apps" for your browser
or file manager). Two things to know:

- **The hub must be reachable at an `https://` address.** The app, and Android itself,
  refuse plain `http://` to anything but the phone itself, so a hub on your PC's
  `http://192.168…` address will not work. Put the hub behind HTTPS first
  (`docs/nexus/deploy.md`, the HTTPS profile), then enter that address in the app.
- **A package with `-testkey` in its name is a test build.** It installs and runs, but a
  later build cannot be installed over it: remove the app first. See
  `docs/RELEASING.md` ("The parent app (Android)").

**Not tested by us:** the package has been built and signed by the automated build, but
nobody has installed it on a phone or an emulator yet. If you can, do tasks 49 to 52 on a
phone and tell us what happened from the first screen on: whether it installed, whether it
opened, whether the keyboard covers the fields, whether the back button behaves, how Urdu
looks on your phone.

Also not in this build: notifications when a message or notice arrives (open the app to
see), paying fees in the app, report cards as PDF, attachments, and an iPhone version.

## 9. Report a problem

Please report anything, including small things, at
<https://github.com/quietbytehq/madrasa/issues/new>. If you do not have a GitHub account,
send the same information to the person who gave you this build.

For each problem, tell us:

- **Which build**: the version shown under Help → About, and Windows or Linux.
- **What you did**: the steps, in order, from the screen you started on.
- **What you expected**, and **what happened instead**.
- **A screenshot** of the screen, including any red message (Windows: press
  Win+Shift+S; Linux: Print Screen).
- If the app would not start or closed on its own, say so and attach a zip of the
  `madrasa` folder named above **without** the `pg` subfolder (that subfolder holds the
  database).

Things we already know about in this build: installers are unsigned (hence the Windows
warning); everything is in English and Urdu only. The version number is under the **Help**
button (top bar) → About; please quote it in reports. Teachers see only the modules
their role can use (no Fees, Settings, Backup or Import) and no colleagues' phone numbers.

Thank you. The next build will carry your fixes.
