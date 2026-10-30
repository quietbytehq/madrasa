# Milestone M0 — Foundation

Goal: Madrasa installs on a fresh Linux or Windows PC, runs a setup wizard, lets the admin log in, and shows the themed (empty) dashboard. No school features yet.

Tick `[x]` only when built AND tested. Log progress in docs/PROGRESS.md.

## 1. Repo
- [x] Apache 2.0 LICENSE, README (what Madrasa is, editions, status: pre-alpha), .gitignore, CONTRIBUTING.md stub
- [x] Folder layout per CLAUDE.md; Cargo workspace + pnpm workspace
- [x] docs/spec.md present

## 2. App skeleton
- [x] Tauri 2 + React + TS + Vite + Tailwind app builds and launches
- [x] Lint, format and type-check scripts (eslint, prettier, clippy, rustfmt)

## 3. Design system
- [x] Theme tokens (light + dark) as CSS variables and Tailwind config
- [x] Inter + Noto Nastaliq Urdu fonts bundled locally (app must work offline)
- [x] Components: Button, Input, Select, Table (sort/filter/paginate), Card, Badge, Dialog, Toast, EmptyState, Spinner, Breadcrumbs
- [x] i18n layer with English and Urdu; RTL switch verified on all components

## 4. App shell
- [x] Sidebar: Dashboard, Students, Attendance, Examinations, Fees & Finance, Teachers, Reports, Backup, Settings (others hidden by edition flag)
- [x] Top bar: search (Ctrl+K, stub), notifications (stub), help, user menu with logout and language switch
- [x] Empty dashboard with placeholder cards; unfinished modules show "Coming soon"
- [x] Usable at 1366×768 and 1920×1080; keyboard navigation works

## 5. Backend + database
- [ ] Axum API under /api/v1 with health endpoint
- [ ] PostgreSQL + sqlx migrations; app starts and migrates automatically
- [ ] Foundation tables: schools, users, roles, permissions, user_roles, settings, audit_log, custom_field_definitions — all following CLAUDE.md rules (UUID v7, school_id, sync columns, soft delete)
- [ ] Audit log writes on every create/update/delete (tested)
- [ ] Decide bundling approach for PostgreSQL in all-in-one installs; record as ADR

## 6. Auth + roles
- [ ] Local accounts, Argon2id password hashing, session tokens
- [ ] Default roles: principal, admin, accountant, teacher (parent reserved for Nexus)
- [ ] Backend permission checks on every endpoint (tested: a teacher cannot reach admin endpoints)
- [ ] Account lockout after repeated failed logins

## 7. Edition flags
- [ ] Single feature-flag module; edition stored in settings (default: lite)
- [ ] UI and API both respect flags (tested)

## 8. First-run setup wizard
- [ ] School name, school code, city
- [ ] Academic year start month (April or August), currency PKR
- [ ] Create admin account
- [ ] Set backup password; generate and display a printable recovery key (key derivation: Argon2id)

## 9. Demo data
- [ ] Generator creates a fake school with ~1,200 students, ~120 staff, classes K-12 — clearly marked DEMO
- [ ] Never runs automatically on a real install

## 10. CI and packaging
- [ ] GitHub Actions: lint, tests, build on Linux and Windows
- [ ] Release artifacts: .deb, AppImage or Snap, Windows installer (unsigned is fine for now)

## Done when
Fresh install → setup wizard → login → themed dashboard, on both Linux and Windows CI builds, with all tests passing and docs/PROGRESS.md up to date.
