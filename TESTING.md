# Testing Madrasa Lite v0.1.0-alpha

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
| Windows 10/11 (64-bit) | `Madrasa_0.1.0_x64-setup.exe` |
| Debian 12/13, Ubuntu 22.04+, Linux Mint 21+ | `Madrasa_0.1.0_amd64.deb` |
| Any other 64-bit Linux from 2022 or later | `Madrasa_0.1.0_amd64.AppImage` |

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
sudo apt install ./Madrasa_0.1.0_amd64.deb
```

Then start **Madrasa** from your applications menu, or run `madrasa` in a terminal.

### Linux (AppImage: any distribution)

1. Right-click the `.AppImage` file → Properties → Permissions → tick **Allow executing
   file as program** (or run `chmod +x Madrasa_0.1.0_amd64.AppImage`).
2. Double-click the file. If nothing happens, open a terminal in that folder and run
   `./Madrasa_0.1.0_amd64.AppImage`; if it complains about FUSE, run
   `./Madrasa_0.1.0_amd64.AppImage --appimage-extract-and-run` instead.

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
17. **Several PCs at once** (needs two or more PCs on the same network; see section 6).
    Log in on each PC with a different account at the same time and enter marks or attendance
    for different sections. Each person should see only their own sections and modules, and
    nobody should be logged out or see someone else's work.

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

## 7. Report a problem

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
