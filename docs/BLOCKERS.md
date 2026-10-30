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
