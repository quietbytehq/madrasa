# Testing Madrasa Lite v0.1.0-alpha

Thank you for trying Madrasa. This is an early build for a small group of testers. It is
not ready for a school's real records yet: use made-up data, expect rough edges, and tell us
about everything that confuses you. Nothing you do in the app leaves your computer.

**What you need:** a Windows 10/11 PC or a Linux PC (Ubuntu 22.04 or newer, or similar),
about 1 GB of free disk space, and 20–40 minutes. No internet connection is needed after the
download.

## 1. Install

Download the file for your computer from the release page (the "Assets" list):

| Computer | File |
| --- | --- |
| Windows 10/11 (64-bit) | `Madrasa_0.1.0_x64-setup.exe` |
| Ubuntu, Debian, Linux Mint | `Madrasa_0.1.0_amd64.deb` |
| Any other Linux | `Madrasa_0.1.0_amd64.AppImage` |

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

The first screen is the setup wizard. Make up a school; do not use a real school's data.

1. **School**: name (for example "Test School"), a short code (for example `TST`), a city.
2. **Academic year**: starts in April or August. Currency is PKR in this build.
3. **Administrator**: your name, a username, a password of at least 10 characters.
4. **Backup security**: a backup password (at least 10 characters). It protects the backup
   files Madrasa makes every night.
5. **Recovery key**: Madrasa shows a long key once. Copy it somewhere (a text file is fine for
   testing), tick the box, and go to the login page.

Log in with the administrator username and password you chose.

## 3. Load the demo school (sample data)

This build has no one-click "demo school" inside the app; the full demo school (1,200
students) is loaded with a developer tool that is not part of the installer. Instead, the
release includes two small spreadsheets of made-up Pakistani names for the import wizard:
`sample-students.xlsx` (72 students in KG–5, sections A and B) and `sample-staff.xlsx`
(8 staff). Download both from the release page, then:

1. **Settings → Academic years**: make sure a year is active (the wizard created one).
2. **Settings → Classes**: click **Add KG–12**. Then add **sections A and B** to KG, 1, 2, 3, 4
   and 5 for the active year (the import needs them to exist).
3. **Settings → Fee heads / Fee structure**: add a head "Tuition" and give each class an
   amount (for example 2,500 for KG rising to 4,000 for class 5). This is enough for
   vouchers later.
4. Sidebar → **Import** → **Students**: choose `sample-students.xlsx`. The wizard should match
   every column on its own; the preview should say **72 rows to create, 0 errors**. Click
   **Import**.
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
3. **Change the fee rules.** Fees → Setup: set a sibling discount of 10% and a late fee of 10%
   after the 15th. Give one student a 50% scholarship with a reason.
4. **Generate this month's vouchers** (Fees → Vouchers → Generate). Open a voucher for a
   student who has a sibling and check the sibling discount; check the scholarship on the
   other student. Print a voucher (use "Save as PDF" in the print dialog).
5. **Collect fees.** Record a full payment on one voucher and a partial payment on another.
   Print a receipt. Then reverse one receipt with a reason and see the ledger.
6. **Defaulters.** Fees → Defaulters: set the date to next month to see who still owes.
   Try the Excel export button.
7. **Mark attendance** for one section today (everyone present by default; mark two absent,
   one late). Try to mark attendance for a Sunday: it should be refused as a weekend.
8. **Add a holiday** (Attendance → Calendar), then look at the attendance register for the
   month.
9. **Enter marks.** Settings → Assessments shows the exam types for the year. Enter marks for
   one subject in one section, submit the sheet, then (as the principal/admin) approve it.
10. **Print a report card** for a student in that section.
11. **Make a backup** (Backup → Back up now) and look at the file in Documents → Madrasa
    Backups. Then change a student's name, **restore** that backup, and check the name is
    back. You will need the backup password you set in the wizard.
12. **Switch to Urdu** (the language switch in the top bar) and repeat one of the tasks above
    in Urdu; check the layout reads right-to-left and nothing is cut off. Switch back.
13. **Dark mode and a small screen.** Turn on dark mode (Settings), and if you can, try the
    app on a 1366×768 laptop screen or a window that size.
14. **Create a teacher login.** Teachers → pick a staff member → create a login with the
    Teacher role, assign them as class teacher of one section, log out and log in as them:
    they should see only their own section's attendance and marks.
15. **End of year (preview only).** Settings → Academic years → **Roll over year**: read the
    preview of who moves where, but you can cancel without applying it.

## 5. Report a problem

Please report anything, including small things, at
<https://github.com/quietbytehq/madrasa/issues/new>. If you do not have a GitHub account,
send the same information to the person who gave you this build.

For each problem, tell us:

- **Which build**: v0.1.0-alpha, and Windows or Linux.
- **What you did**: the steps, in order, from the screen you started on.
- **What you expected**, and **what happened instead**.
- **A screenshot** of the screen, including any red message (Windows: press
  Win+Shift+S; Linux: Print Screen).
- If the app would not start or closed on its own, say so and attach a zip of the
  `madrasa` folder named above **without** the `pg` subfolder (that subfolder holds the
  database).

Things we already know about in this build: installers are unsigned (hence the Windows
warning); the demo school cannot be loaded from inside the app; the app does not show its
version number yet; everything is in English and Urdu only.

Thank you. The next build will carry your fixes.
