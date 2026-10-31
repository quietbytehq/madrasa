# Blockers

Things that could not be resolved without a decision or information from outside the repo.
Each entry: date, task, what is blocked, what was done instead.



## 2026-10-30 — All tasks: cannot push to GitHub (RESOLVED later the same day)
- Resolved: GitHub access was granted; the branch was pushed and CI run 1 queued
  (https://github.com/quietbytehq/madrasa/actions/runs/41022711742).
- Blocked (original): `git push` to `quietbytehq/madrasa` is refused (403). The Claude GitHub App is not
  installed or connected for the `quietbytehq` organization.
- Fix (outside the repo): an org admin installs the Claude GitHub App at
  https://github.com/apps/claude/installations/select_target or reconnects GitHub at
  https://claude.ai/connect-github, then re-runs the push.
- Done instead: work continues as local commits on `claude/kind-franklin-ac2vjh`; the push is
  retried after each task.

## 2026-10-30 — Task 10: CI has not run; Windows build and installer unverified (RESOLVED)
- Resolved: CI run https://github.com/quietbytehq/madrasa/actions/runs/41022747057 passed all
  five jobs (frontend; Rust on Linux with PostgreSQL and on Windows; desktop bundles on
  Linux and Windows, NSIS installer uploaded as an artifact).
- Blocked (original): the workflows in `.github/workflows/` cannot be executed from this session because
  the branch cannot be pushed (see above) and there is no Windows machine here. The Linux CI
  steps were mirrored locally (fmt, clippy, tests against PostgreSQL 16, `tauri build`), but
  the Windows job, the NSIS installer and the `tauri-action` release job are untested.
- Fix (outside the repo): once the push goes through, open the Actions tab and watch the first
  run; the "M0 done when" condition for Windows depends on it.
- Done instead: Linux verified end to end (fresh install → setup wizard → login → dashboard
  under Xvfb, `.deb` and AppImage bundles built locally); everything Windows-specific is guarded
  by `#[cfg(windows)]`/`#[cfg(unix)]` and the shell compiles without Unix-only crates.

## 2026-10-31 — Maple Ridge demo file not received

- Blocked: the task named an attached `Madrasa_Demo_MapleRidge_USA.xlsx` (1,123 students,
  40 staff), but no file reached the session's sandbox (`/mnt/user-data/uploads` was empty).
- Done instead: `crates/api/tests/international.rs` generates a US-style register with the
  same counts and the column shapes such an export usually has (Student ID, First/Last Name,
  Date of Birth in MM/DD/YYYY, Gender, Grade, Homeroom, Parent/Guardian Name, Relationship,
  Parent Phone, Parent Email, Street Address, Enrollment Date; Employee ID, Job Title, Hire
  Date for staff) and imports it through the real endpoints with zero errors.
- Fix: run the import wizard on the real file; any heading it does not map is one alias in
  `madrasa_core::imports` (`STUDENT_FIELDS` / `STAFF_FIELDS`).
