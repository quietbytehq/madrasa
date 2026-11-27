# Testing Madrasa v0.6.1-alpha (Lite, Standard, the Nexus hub and the parent app)

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
| Windows 10/11 (64-bit) | `Madrasa_0.6.1_x64-setup.exe` |
| Debian 12/13, Ubuntu 22.04+, Linux Mint 21+ | `Madrasa_0.6.1_amd64.deb` |
| Any other 64-bit Linux from 2022 or later | `Madrasa_0.6.1_amd64.AppImage` |

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
sudo apt install ./Madrasa_0.6.1_amd64.deb
```

apt also installs the libraries Madrasa needs (WebKitGTK 4.1 and GTK 3) if they are missing,
so it needs an internet connection the first time. Keep the `./`. If you installed with
`sudo dpkg -i` instead and it stopped with "dependency problems", run `sudo apt -f install`.

Then start **Madrasa** from your applications menu, or run `madrasa` in a terminal.

### Linux (AppImage: any distribution)

1. Right-click the `.AppImage` file → Properties → Permissions → tick **Allow executing
   file as program** (or run `chmod +x Madrasa_0.6.1_amd64.AppImage`).
2. Double-click the file. If nothing happens, open a terminal in that folder and run
   `./Madrasa_0.6.1_amd64.AppImage`; if it complains about FUSE, run
   `./Madrasa_0.6.1_amd64.AppImage --appimage-extract-and-run` instead.

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
    Wrong password five times locks it for 15 minutes like a local account. The screen says
    the server PC must trust the directory's certificate authority; if your directory uses
    the organisation's own CA, try Test connection before installing it on the server PC (a
    plain "not trusted" message), then after installing it and restarting Madrasa.
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

The hub image is published with every release (`ghcr.io/quietbytehq/madrasa-nexus`, tagged
with the release, for example `:0.6.1-alpha`, and `:latest`). Compose pulls it:

```bash
cd deploy/nexus
```

```bash
cp .env.example .env
```

Edit `.env` and set `NEXUS_DB_PASSWORD` (letters and digits) and `NEXUS_SETUP_TOKEN` (any
long text you choose), and `NEXUS_IMAGE=ghcr.io/quietbytehq/madrasa-nexus:0.6.1-alpha` so
the hub is the same version as your Madrasa. Then:

```bash
docker compose pull nexus
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

*Since v0.4.2-alpha. The APK on the release page is a test build (see "On a phone").*

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

## 9. New in v0.5.0-alpha

60. **Restoring a backup while joined to a hub** (section 7 set up, the school joined and
    synced). Back up now. Add two students and press *Sync now*: head office has them. Restore
    the backup you just made. The restore window warns that the school is joined and that
    head office will be asked to reset its copy, and **Restore now** stays greyed out until
    you tick the box. Log in again: Settings → School network says *Not in step yet* and that
    head office will be asked. Press *Sync now*: it then says head office set its old copy
    aside, and after another *Sync now*, *In step*. In the console the school has exactly the
    restored students. Add a new student (it gets the same number as one of the two you lost)
    and sync: it reaches head office, nothing waits at head office.
61. **Restoring when not joined.** On an install that never joined, restore a backup: there is
    no network warning and nothing extra to tick.
62. **Records waiting at head office.** If the School network card ever shows "record(s) from
    this school are waiting at head office", it must not say *In step* at the same time. Tell
    us what the reason says.
63. **Backup key card.** With the key left with head office (task 45), release it in the
    console, then press *Sync now* in the school without leaving Settings: the release appears
    on the card at once.
64. **Create login twice.** Teachers → open a staff member who has no login, but do not
    create it yet. In Settings → Users create a login for the same person. Back in the first
    window press *Create login*: Madrasa shows the login that already exists and asks
    "Change this login's role?"; *Keep it as it is* changes nothing. Changing the role in the
    Login account box also asks first.
65. **Installing the .deb on a fresh system** (a spare Ubuntu or Debian PC, or a virtual
    machine, where Madrasa was never installed): `sudo apt install ./Madrasa_0.6.1_amd64.deb`
    installs WebKitGTK and the rest by itself.

## 10. Working offline

You need the school server and one client PC (section 6). Unplug the server's network cable
(or switch off its Wi-Fi) to "lose" it; plug it back in to reconnect.

66. **The badge.** On the client, the top bar shows **Connected**. Unplug the server: within
    about 5 seconds it shows **Reconnecting…**, and about 15 seconds after the cable came out
    **Offline**. Plug it back: **Connected** within a few seconds.
67. **Keep working.** With the server unplugged and the badge at Offline, open screens you had
    open before (a student, the voucher list): they show at once, without a wait. Then make
    exactly three changes: admit a new student (Students → Add student, fill the form, Save),
    edit another student's address (open the student, Edit, Save), and mark attendance for a
    class (Save). Each save says "Saved on this PC" and the badge counts 1, 2, then
    **3 waiting**; the badge's panel lists a new student, a student record and an attendance
    sheet, and nothing else. Opening a form, typing in it, or the form saying "Duplicates
    cannot be checked right now" adds nothing to the count. Press Save twice on one of them:
    still 3.
68. **Restart.** Still unplugged, close Madrasa on the client and open it again (or restart
    the PC). The badge still shows 3 waiting. Plug the server back: within seconds the badge
    shows Connected with nothing waiting. On the server, check: the new student is there
    once, the edit is there, the attendance is there.
69. **A provisional receipt.** Unplug the server and record a fee payment on the client. A
    receipt numbered `P-…` opens at once, marked "Provisional receipt — not yet confirmed".
    Print it. Plug the server back. The badge's panel says which real receipt number replaced
    it; the real receipt (Fees → the voucher) says "Replaces provisional receipt P-…". Open
    the provisional receipt page again (from the panel): it now says "Confirmed — receipt R…"
    and no longer "not yet confirmed".
70. **Two PCs, one record.** Unplug the client PC this time (not the server). On the client,
    change a student's address; on the server PC, change the same student's address to
    something else and their father's name. Plug the client back. The server's address
    stands, the father's name too, and the client's badge panel lists your address under
    "Overwritten on arrival".
71. **Drafts.** Start a new admission on any PC, type a few fields, then close Madrasa
    without saving (or end it in Task Manager). Open Madrasa and the same form: it offers
    **Restore**, and restores what you typed.
72. **Parent app.** With the phone's network off, the badge at the top says Offline, and the
    last loaded screens still show.

**Not tested by us:** a real cable pulled out of a real school network. Our tests stop the
server program, and cut the virtual network between a server and a client that each run in
their own container.

## 11. New in v0.5.1-alpha

Fixes for what the v0.5.0-alpha test found. Tasks 66, 67 and 69 above changed too.

73. **A payment taken offline shows at once.** On a client, open a student with an unpaid
    voucher (Students → the student → Fees, or Fees → Collect) while the server is still
    there. Unplug the server, wait for Offline, and record part of the voucher, say
    Rs 5,000. Close the provisional receipt and go back to the student's Fees tab: beside the
    voucher's status it says **Paid (provisional, waiting to send): Rs 5,000.00**, and under
    the totals a line says how much is waiting and what the balance will be once it arrives.
    Fees → Vouchers shows the same beside that voucher, and so does the voucher's print page.
74. **No collecting twice.** Still offline, press *Record payment* on the same voucher again.
    A warning says a payment of Rs 5,000.00 is already waiting, with its `P-…` number; the
    amount offered is what is left, not the full balance; *Record payment* stays greyed out
    until you tick "This is a further payment". Cancel. Stay on the student's Fees tab and
    plug the server back: within a few seconds the note goes and the same screen shows the
    server's own figures (Partial, the amount paid, the smaller balance) without being
    reopened.
75. **Checking for duplicates offline.** Offline, Students → Add student, type a name, the
    father's name and a date of birth: the form says "Duplicates cannot be checked right
    now", you can still save, and the badge counts one change for the save, none for the
    typing.
76. **Badge and card agree.** With the school joined to a hub (section 7), open Settings →
    School network and press *Sync now*. Whatever the card says ("In step", "Not in step
    yet", records waiting at head office, a number to send), the top bar's **Head office**
    line says the same in the same moment, not a minute later.
77. **Dashboard.** Mark attendance for every section (or load the demo school on a weekday
    and mark the sections it lists): the attendance panel is headed **All sections marked**,
    not "Not yet marked today". A new school with no students yet shows no such panel.
78. **Student profile.** Open any demo student: the profile's extra fields show only what
    the school entered. No "DEMO: true", and after an import no "import_batch" or row
    numbers. Edit and save the student, then look again: nothing appeared.
79. **A new install and backups.** Install on a PC that never had Madrasa and set up a school.
    The dashboard and the Backup page say **No backup yet — run one now**; nothing about a
    backup "older than 2 days". Press *Back up now*: "Backups are up to date".
80. **Private files (Linux).** After starting Madrasa once, run
    `find ~/.local/share/madrasa -not -path '*/pg/data/*' -not -path '*/pg/bin/*' -perm /077`
    in a terminal: it prints nothing (every file and folder is for your user only), and
    `ls ~/.local/share/madrasa/pg/.pgpass` says there is no such file. **Windows:** right-click
    `%APPDATA%\madrasa` → Properties → Security: only your own account, SYSTEM and
    Administrators are listed.
81. **Closing the server.** On the server PC (or a single PC) close Madrasa with the window's
    close button. Within a few seconds no `postgres` process of Madrasa is left (Windows:
    Task Manager → Details; Linux: `pgrep -u $USER postgres` prints nothing), and Madrasa
    opens again without delay.

## 12. New in v0.6.0-alpha

Tasks 82 to 107: what was added between v0.5.1-alpha and v0.6.0-alpha, the work done to
make Madrasa ready for a first school (security, bringing a school's records in, backups
that are checked, larger schools, wording, keyboard).

### Signing in and passwords

82. **Change your own password.** Click your name (top right) → *Change password*. Give the
    current password and a new one twice: "Password changed.", and you stay signed in. Sign
    out and in again: the old password is refused, the new one works.
83. **A reset signs the person out.** As administrator, Settings → Users → reset another
    user's password while that user is signed in on another PC (or another browser profile).
    On their next click they are returned to the sign-in screen, not left on a screen full of
    errors.
84. **Wrong name or wrong password look the same.** At the sign-in screen try a username
    that does not exist, then a real one with a wrong password: the message is the same for
    both.
85. **The log file.** After a start there is a file `logs/madrasa.log` in Madrasa's folder
    (section 1 says where). It holds no password. If Madrasa shows its error screen, this is
    the file it means.

### Bringing a school's records in (Import)

86. **Templates with instructions.** Import → choose *Students*, then *Guardians*, *Staff*,
    *Fee structure* and *Opening fee balances*; for each press *Excel template*. Open the file in
    Excel or LibreOffice: a sheet for the records, a sheet **Read me** and a sheet **ہدایات**
    (Urdu, right to left). Tell us if the Urdu sheet does not display properly or its wording
    is unclear.
87. **The dry run.** Fill a few rows of the Guardians template: two children of the same
    father (use real student numbers from Students), and one row with a student number that
    does not exist. Choose the file → *Check the file*. It says 2 will be created and 1
    cannot be imported, **why** (no student with this number) and the row number. *Download
    dry-run report* gives the same as a workbook. Nothing has been saved yet: check a
    student's Guardians tab.
88. **Confirm, then undo.** Press *Import 2 rows*: a question repeats the numbers. Confirm.
    Both children now show the same guardian (one record, not two). Under *Import history*
    press *Roll back*: the guardian is gone from both.
89. **Fee structure.** Fill the Fee structure template with a fee head that does not exist
    yet (say "Computer fee") for two classes, and import. Fees → Setup shows the new head
    with its amounts.
90. **Opening balances.** Import an opening balance of Rs 3,000 for one student, then
    generate this month's vouchers (Fees → Vouchers → *Generate vouchers*). That student's voucher
    shows **Arrears 3,000** above this month's fee, and the student also got this month's
    voucher (in v0.5.1 the balance took the place of the month's voucher). The opening
    balance never earns a late fee.
91. **Likely duplicates and missing classes.** Import a students sheet with one child who is
    already on file (same name and father's name) and one row for class "13": the dry run
    lists the first under *Likely duplicates* and "13" under *Classes and sections that are
    not set up*.
92. **An accountant** signed in can open Import, and of the kinds only *Fee structure* can
    be chosen.

### Data health

93. **Find what is wrong.** Import → *Data health*. Each line is a check with a count.
    Add a student without choosing a class and section, and a guardian without a phone
    number, then *Check again*: both are listed, each with why it matters and a link that
    opens the record.

### Leaving with your data

94. **Export everything.** Backup → *Export everything*. One workbook downloads, named with
    the school and the date. It has a first sheet *About this file*, then Students,
    Guardians, Staff, Fee vouchers, Fee receipts, Attendance, Marks and the rest. Search it
    for a password or for the word "argon": nothing. A teacher has no such button.

### Backups that are checked

95. **The check.** Backup → *Back up now*. The file is written at once ("Backup written
    to …"); below the date a line says *Checking the backup…* and within a minute *Checked
    after writing: restored into a temporary database and every record counted*. The run
    history shows **Checked** beside the backup.
96. **A backup that fails.** Set the backup folder to a USB drive, make one backup, pull the
    drive out and press *Back up now*. A **red notice** at the top of the Backup page, and of
    the dashboard for every user, says the backup failed, why, how old the last good backup
    is and what to do. Plug the drive back and back up again: the notice goes.
97. **Free space.** The check needs free disk space about the size of the database. Tell us
    what the notice says if the disk is nearly full.

### Larger schools

98. **Speed.** If your school has more than 1,000 students, or you loaded several years of
    records: open Students, Fees → Defaulters, Fees → Daily collection, Data health and the
    dashboard, and tell us any screen that takes more than a second or two to show.
99. **Vouchers for the whole school.** Fees → Vouchers → *Generate vouchers* for a month,
    all classes:
    tell us how long it took and for how many students. (Our own test: 5,040 students in 12
    seconds on a 4-core machine.)

### Clearer wording

100. **Receipts.** Print any receipt: the amount is written "Rs 500.00" like everywhere
     else (in v0.5.1 it said "PKR 500.00").
101. **Back from a receipt.** Fees → Collect → find a student → open one of their receipts →
     *Back*: you are on that student's ledger again, not on an empty search.
102. **Report cards.** Exams → Report cards: the percentage in the list is the same figure
     as on the printed card (one decimal place).
103. **Empty lists** say what is missing and what to do, for example a student with no
     payments: "No payment has been received from this student yet."

### Keyboard and screen readers

104. **Without the mouse.** Press Tab on any screen: the first stop is *Skip to content*.
     Admit a student using only the keyboard (Tab, Shift+Tab, Enter, Space): every field is
     reached in order and you can always see where you are.
105. **Tabs, menus and dialogs.** On Fees, with a tab focused, the Left and Right arrows
     move between Vouchers, Collect, Defaulters… (in Urdu the arrows are mirrored). Open
     your name's menu with Enter, move with Up and Down, close with Esc. Open *Record
     payment* with Enter, close it with Esc: the keyboard is back on the *Record payment*
     button.
106. **A screen reader**, if you use one (NVDA on Windows, Orca on Linux, TalkBack with the
     parent app): we have not been able to test with a real one. Tell us what is announced
     wrongly, or not at all.

### Urdu

107. **Read the Urdu.** No Urdu in Madrasa has been read by a native speaker yet.
     `docs/urdu-review.md` says where it all is and which words we were unsure of. Three
     were settled in v0.6.1 (task 115). Any correction is welcome, in any form.

## 13. New in v0.6.1-alpha

Tasks 108 to 116. A small release before the first school: passwords in the head office
console, an import that is not lost when Madrasa is closed, three Urdu words settled, and
one change nobody should notice (see "Under the hood" at the end of this section).

Upgrading from v0.6.0-alpha: make a backup, then install v0.6.1 over it. Nothing in the
database changes between the two, so the first start does no conversion and a backup made
by v0.6.0 is accepted by v0.6.1. (Our automated upgrade test starts from v0.4.1; this
particular step, with the installers, has not been tried by us.)

### Head office console (the Nexus hub)

108. **Change your own password in the console.** Sign in to the head office console as any
     user, also one who may only read reports. Click the **lock** button at the top right
     (next to *Log out*) → *Change password*. Give the current password and a new one
     twice: "Password changed.", and you stay signed in. If you were signed in to the
     console in a second browser as well, that one is signed out at its next click. Sign
     out and in again: the old password is refused, the new one works.
109. **Wrong current passwords lock the account.** In the same dialog type a wrong current
     password five times: each time "The current password is not right."; after the fifth,
     the account is locked for fifteen minutes, also for signing in, exactly as after five
     wrong sign-ins. (Ask an administrator to reset the password to get in sooner.)
110. **An administrator resets someone else's password.** As an administrator: Users → *Edit*
     another user → type a *New password* → *Save*. That user is signed out everywhere, a
     lock on the account is cleared, and they sign in with the new password (and should then
     change it themselves, task 108). Now *Edit* your own row: there is no password box,
     only a line sending you to the lock button, because your own password always asks for
     the current one.

### Import

111. **An import survives closing Madrasa.** Import → *Students* (or *Guardians*) → choose a
     file with at least one row that has a mistake → match the columns → *Check the file* →
     correct the mistake in the grid. Under the steps a line says the import is kept on
     this PC. **Do not confirm.** Close Madrasa with the window's X, start it again, sign in
     as the same person and open Import: instead of the list of things to import it says an
     import was not finished, names the file and the number of rows, and offers **Continue
     the import you started on** (the date) and **Discard it**. Continue: the columns are
     matched as you left them, your correction is in the row, and a note says the rows were
     checked again. Confirm it now, or go on to the next task.
112. **The hard way.** Do the same, but instead of closing Madrasa end it from the Task
     Manager (Windows) or with `pkill madrasa` (Linux), or switch the PC off at the wall
     while the Check step is on screen. After the restart the same offer must be there.
     **We have not tried this in the installed program**, only in a browser on Linux; tell
     us what you see, also if nothing is offered.
113. **Discard, and nothing left behind.** Leave an import unfinished again, reopen Import
     and press **Discard it**: the ordinary list of things to import is back, and closing
     and reopening Madrasa offers nothing. Then finish an import normally (confirm it):
     afterwards nothing is offered either. And sign in as a *different* person on the same
     PC while someone's import is unfinished: that person is offered nothing.

### Small things

114. **No "Parents" in the sidebar.** A Madrasa running as the Nexus edition showed a
     "Parents" entry that only said "Coming soon". It is gone; what a school does for
     parents is under **Parent app**, which appears once the school has joined a school
     network (sections 7 and 8).
115. **The three Urdu words.** Switch to Urdu. The fee pages, a printed fee voucher, a
     receipt, the parent app's fee page and the fee reports say **چالان** (not "واؤچر");
     signing in says **لاگ ان** everywhere, also in the head office console (not
     "سائن ان"); head office is **ہیڈ آفس**. Tell us any place that still shows an old
     word, and any sentence where "چالان" reads wrongly.
116. **The manual.** The Urdu and English manuals (the two PDFs that come with a release)
     show the same words as the screens, and their pictures were made again.

### Under the hood

Nothing to click here, only something to watch for. v0.6.1 uses a newer version of the
library Madrasa talks to its database with (sqlx 0.9.1). That version repairs the fault
behind the worst problem found before v0.6.0, where a change shown as saved could be lost
on a school with client PCs, and the extra safeguard Madrasa carried against it since then
has been taken out again. Our automated tests cover this (one of them fails on the old
library and passes on the new one), but a school with several PCs is the real test: after a
day's work on client PCs, **anything that was saved and is not there the next morning** is
the first thing to tell us, with the time it was entered and on which PC.

## 14. For the person who installs (not everyday use)

Three scripts in `scripts/` test what a tester cannot do by hand; each says at its top what
it needs. They are run by us before a release; a school does not need them.

- `scripts/load-test.sh`: a made-up school of 5,000 students with three years of records;
  every main screen must open within a second, vouchers for the school within a minute, a
  backup within two.
- `scripts/soak-test.sh 480`: eight hours of use with client PCs switched off and on, the
  server restarted and its power cut; no request may fail, memory must not grow, no
  database process may be left behind.
  Our own eight-hour run of 26 November was cut off after 5 h 53 min by the machine it ran
  on. No request had failed, but the server's memory was still rising, so this one is
  **not passed yet** and has to be run again on a PC that stays on (docs/PROGRESS.md,
  "Summary for the owner", decision 12).
- `scripts/upgrade-test.sh`: a school on 0.4.1 is upgraded in place; every record must
  still be there.

**Upgrading and old backups.** A backup file can only be restored by a Madrasa whose
database is laid out the same way as the one that made it: Madrasa compares the layout, not
the version number. A version that changes the layout refuses older backups, so after
installing such a version make a new backup the same day (the nightly one does this by
itself) and keep the old installer next to the old backup files. **v0.6.1 does not change
the layout**: backups made by v0.6.0 stay usable. (By the rule above; we have not restored
a real v0.6.0 backup file on v0.6.1.)

**Not tested by us:** all of the above on Windows by hand; a real USB drive pulled out; a
real screen reader; the import templates opened in Excel by a person; a school's own PC
under the load test; the kept import (tasks 111 to 113) in the installed program, where it
was only tried in a browser on Linux.

## 15. Report a problem

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
warning); everything is in English and Urdu only, and the Urdu has not yet been read by a
native speaker (task 107). The version number is under the **Help**
button (top bar) → About; please quote it in reports. Teachers see only the modules
their role can use (no Fees, Settings, Backup or Import) and no colleagues' phone numbers.

Thank you. The next build will carry your fixes.
