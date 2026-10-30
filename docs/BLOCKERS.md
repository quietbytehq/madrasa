# Blockers

Things that could not be resolved without a decision or information from outside the repo.
Each entry: date, task, what is blocked, what was done instead.



## 2026-10-30 — All tasks: cannot push to GitHub
- Blocked: `git push` to `quietbytehq/madrasa` is refused (403). The Claude GitHub App is not
  installed or connected for the `quietbytehq` organization.
- Fix (outside the repo): an org admin installs the Claude GitHub App at
  https://github.com/apps/claude/installations/select_target or reconnects GitHub at
  https://claude.ai/connect-github, then re-runs the push.
- Done instead: work continues as local commits on `claude/kind-franklin-ac2vjh`; the push is
  retried after each task.

## 2026-10-30 — Task 10: CI has not run; Windows build and installer unverified
- Blocked: the workflows in `.github/workflows/` cannot be executed from this session because
  the branch cannot be pushed (see above) and there is no Windows machine here. The Linux CI
  steps were mirrored locally (fmt, clippy, tests against PostgreSQL 16, `tauri build`), but
  the Windows job, the NSIS installer and the `tauri-action` release job are untested.
- Fix (outside the repo): once the push goes through, open the Actions tab and watch the first
  run; the "M0 done when" condition for Windows depends on it.
- Done instead: Linux verified end to end (fresh install → setup wizard → login → dashboard
  under Xvfb, `.deb` and AppImage bundles built locally); everything Windows-specific is guarded
  by `#[cfg(windows)]`/`#[cfg(unix)]` and the shell compiles without Unix-only crates.
