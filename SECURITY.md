# Security

Madrasa keeps children's records and a school's money. If you find a way to read or change
data you should not be able to, to get past a login, or to stop a school from working, please
tell us privately first.

## Reporting a vulnerability

- Open a private report: <https://github.com/quietbytehq/madrasa/security/advisories/new>
  (GitHub → the repository → **Security** → **Report a vulnerability**). Only the
  maintainers see it.
- Please do **not** open a public issue for a vulnerability, and do not test against a
  school's real installation or a hub you do not run yourself. The demo school and the demo
  network (TESTING.md) are there for that.
- Say which version (Help → About), which part (the desktop app, the school server, the
  Nexus hub, the parent app), what you did, and what you could see or do as a result. A
  short script or a screenshot helps.

We aim to answer within five working days, to agree a fix and a date with you, and to credit
you in the release notes if you wish. Madrasa is pre-release software maintained by a small
team; there is no bug bounty.

## Supported versions

Only the latest release receives fixes. Schools should install updates when they appear.

## What we already know

`docs/SECURITY.md` has the threat model, the review of each area, what was fixed and the
risks that remain. Automated checks (`cargo audit`, `cargo deny`, `pnpm audit`, a test that
calls every API route as every role) run on every change.
