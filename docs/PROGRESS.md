# Progress log

Running log of what was done, what is tested and what is left. Newest entries at the bottom
of each milestone section. See `docs/TASKS-M0.md` for the checklist.

## M0 — Foundation

### 2026-10-30 — Task 1: Repo
- Done: README (editions, status pre-alpha), CONTRIBUTING stub, `.gitignore`, `.editorconfig`,
  Cargo workspace (`crates/core`, `crates/db`, `crates/api`, `crates/demo-data`,
  `apps/desktop/src-tauri`), pnpm workspace (`apps/*`), `docs/spec.md` v0.1, ADR folder.
- Tested: `cargo check --workspace` passes with stub crates; pnpm workspace resolves.
- Left: everything else in M0.

### 2026-10-30 — Task 2: App skeleton
- Done: `apps/desktop` Tauri 2 shell (`src-tauri`, icons generated from `assets`-free source
  via `tauri icon`), React 19 + TypeScript (strict) + Vite 7 + Tailwind 3.4. Scripts: `lint`
  (eslint 9 flat config, react-hooks, typescript-eslint), `format`/`format:check` (prettier +
  tailwind plugin), `typecheck` (`tsc -b`), `test` (vitest + Testing Library, jsdom). Rust:
  `rustfmt.toml`, clippy with `-D warnings` clean.
- Tested: `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm test`, `pnpm build`,
  `cargo build -p madrasa-desktop`, `pnpm tauri build --debug --no-bundle`; the debug binary
  launches under Xvfb and runs until killed (only harmless libEGL warnings).
- Left: in-process API start-up in the shell (task 5), bundling targets in CI (task 10).

### 2026-10-30 — Task 3: Design system
- Done: tokens in `src/styles/tokens.css` (light, dark, print-forces-light) exposed through
  `tailwind.config.ts`; `ThemeProvider` (system / light / dark, persisted). Inter and Noto
  Nastaliq Urdu bundled via `@fontsource` (woff2 shipped in `dist`, no network). Components in
  `src/components/ui`: Button, Input, Select, Table (sort, filter, paginate), Card, Badge,
  Dialog (focus trap, Escape, restore focus), Toast (provider + `useToast`, aria-live),
  EmptyState, Spinner, Breadcrumbs. i18n layer `src/i18n` with `en` and `ur` dictionaries,
  `t()` with placeholders, `Intl` number/date formatting, `<html lang dir>` switching.
- Tested: 28 vitest tests: dictionary parity (every key, same placeholders), provider
  behaviour, every component, and an RTL suite that renders all components under Urdu and
  asserts `dir="rtl"`, translated chrome and no physical left/right utilities.
- Left: nothing for M0.

### 2026-10-30 — Task 4: App shell
- Done: `AppShell` (skip link, collapsible sidebar, top bar, main region), `Sidebar` driven by
  `src/app/navigation.ts` and edition flags (`FeaturesProvider` reads `/system/features`,
  Lite defaults until the API answers), `TopBar` with Ctrl+K search stub, notifications stub,
  help dialog, user menu (language, theme, logout). `SessionProvider` (token in localStorage,
  `/auth/me` on start), `RequireAuth`, `SetupGate` (routes to `/setup` when the API says
  setup is needed), `Dashboard` with four placeholder cards and an empty activity list,
  `ComingSoon` page for every unfinished module, `Login` page. Typed API client in `src/api`.
- Tested: 8 shell tests (edition-based visibility for Lite vs Nexus, dashboard placeholders,
  Coming soon navigation, Ctrl+K, language switch + logout, collapse + keyboard order,
  anonymous redirect). Playwright screenshots of the built UI at 1366×768 and 1920×1080 in
  en/ur × light/dark: no console errors, no horizontal scroll, RTL mirrored correctly.
- Left: real setup wizard page (task 8); login against the real API (task 6).

### 2026-10-30 — Tasks 5–7: Backend, database, auth, edition flags (one commit; the crates
depend on each other and were built together)
- Done, `crates/core` (pure): `edition` (Edition/Feature flags, soft limits), `permissions`
  (20 permission codes, built-in roles with default grants, `PermissionSet`), `hlc` (hybrid
  logical clock stamps), `ids` (UUID v7), `password` (Argon2id, lockout policy 5 tries/15 min),
  `session_token` (random token, SHA-256 stored), `backup` (DEK wrapped by Argon2id-derived keys
  from the backup password and a Crockford-base32 recovery key, AES-256-GCM).
- Done, `crates/db`: migration `0001_foundation.sql` (schools, users, roles, permissions,
  user_roles, sessions, settings, audit_log, custom_field_definitions; UUID v7 keys,
  `school_id`, sync columns, soft delete, JSONB custom, append-only triggers on audit_log),
  repositories that write the audit log in the same transaction, `testing::TestDb` (throwaway
  database per test, skipped when `MADRASA_TEST_DATABASE_URL` is unset).
- Done, `crates/api`: Axum router under `/api/v1`: `/health`, `/system/features`,
  `/setup/status`, `/setup`, `/auth/login|logout|me`, `/users` (admin), `/schools/current`,
  `/audit`, `/nexus/status` (feature-gated stub). `CurrentUser` extractor loads the session and
  effective permissions; handlers call `require(permission)`; `AppState::require_feature`.
  Bootstrap: external `MADRASA_DATABASE_URL` or bundled PostgreSQL (feature
  `embedded-postgres`, ADR 0001). `madrasa-server` binary for Standard/dev.
- Done, desktop shell: starts database + API on a background runtime, exposes `api_base_url`
  command, stops both when the window closes.
- Tested: core 27 unit tests; db 9 integration tests (schema rules for every table, append-only
  audit log, create/update/delete audited with old/new values and no secrets, unique school
  codes, settings upsert + redaction, default roles, sessions, lockout counters); api 6
  end-to-end tests (health, setup once + validation + key material, login/me/logout, lockout
  after 5 failures incl. audit, teacher 403 on `/users` and `/audit` while admin 200, inactive
  account refused and existing session dies, edition flags: `/nexus/status` 403
  `feature_unavailable` in Lite, 403 `forbidden` without `nexus.admin` after switching to
  Nexus, 200 once granted). `madrasa-server` built with `embedded-postgres` started bundled
  PostgreSQL 16.15.1 from scratch, migrated, answered `/health`, completed `/setup` and
  `/auth/login` (run as a non-root user; PostgreSQL refuses to run as root).
- Notes: the bundled archive is downloaded by the `postgresql_embedded` build script from
  GitHub releases (cached under `~/.theseus/postgresql`); this sandbox's proxy blocks the GitHub
  API, so the archive was pre-seeded into the cache by direct download. CI has normal access.
- Left: Windows run of the bundled database (CI build only, not executed here).

### 2026-10-30 — Task 8: First-run setup wizard
- Done: `POST /setup` (one transaction: school, default roles + grants, admin user with
  principal + admin roles, settings incl. academic year month, currency PKR, wrapped backup
  keys, setup marker; returns the recovery key once). UI wizard `src/pages/Setup.tsx`: steps
  School → Academic year → Administrator → Backup security (+ summary) → Recovery key (print
  button with print-only stylesheet, acknowledgement checkbox, go to login). `SetupGate` routes
  to `/setup` while setup is needed.
- Tested: api `setup_runs_once_and_validates_input`; vitest: step validation, request mapping,
  full wizard walk-through with mocked API (validation messages, recovery key shown, ack gate,
  navigation to login), server-side error surfaced.
- Left: nothing for M0.

### 2026-10-30 — Task 9: Demo data
- Done: `crates/demo-data`: deterministic generator (seeded) for "DEMO Gulshan Model School"
  (code `DEMO01`, `is_demo = true`): 1,200 students across KG–12 × sections A/B/C (39
  classes, 30–31 each, plausible ages, unique `DEMO-nnnnn` admission numbers), 120 staff
  (1 principal, 2 admins, 2 accountants, 115 teachers with subjects, usernames `demo.*`).
  Loader inserts the school, default roles and staff accounts (shared documented password)
  in one transaction and marks setup complete; students and classes are written as JSON
  until their tables arrive in M1. CLI `madrasa-demo generate --out <dir>` /
  `madrasa-demo load --yes [--out <dir>]`.
- Safety: the loader errors without `confirmed`/`--yes`, when a non-demo school exists, and
  when demo data is already present. Nothing calls it from the app.
- Tested: 3 generator unit tests (determinism, sizes/classes/roles, DEMO marking and
  uniqueness), 2 loader integration tests (loads into an empty DB and verifies accounts,
  refuses with a real school present and inserts nothing). CLI run manually against a
  scratch database: refused without `--yes`, loaded 120 accounts with it.
- Left: persisting students/classes once M1 adds their tables.

### 2026-10-30 — Task 10: CI and packaging
- Done: `.github/workflows/ci.yml` (frontend lint/format/types/tests/build; Rust fmt, clippy
  incl. the `embedded-postgres` feature, tests with a PostgreSQL 16 service on Linux and
  skipped DB tests on Windows; desktop bundles `deb,appimage` on Linux and `nsis` on Windows
  uploaded as artifacts) and `release.yml` (tag `v*` → draft pre-release with unsigned
  installers via `tauri-apps/tauri-action`). Bundled-PostgreSQL archive cached in CI.
- Tested locally (Linux): `pnpm tauri build --bundles deb,appimage` produced
  `Madrasa_0.1.0_amd64.deb` (45 MiB, depends on libwebkit2gtk-4.1-0 and libgtk-3-0) and
  `Madrasa_0.1.0_amd64.AppImage` (120 MiB); the `.deb` installs with `dpkg -i` (binary,
  desktop entry, icons). Full flow driven with xdotool under Xvfb on a fresh data directory:
  bundled PostgreSQL starts → setup wizard (5 steps) → recovery key page → login → themed
  dashboard. Screenshots reviewed at 1366×768.
- Verified in CI after the push: all jobs green on Linux and Windows, NSIS installer
  artifact produced. The release workflow (tag `v*`) has not been exercised yet.

### M0 status
- Linux: "fresh install → setup wizard → login → themed dashboard" works in the packaged app;
  all tests pass (core 27, db 9, api 6, demo-data 5, vitest 45); fmt, clippy, eslint,
  prettier and tsc are clean.
- Windows: CI run 41022747057 passed: fmt, clippy, unit tests, the bundled-PostgreSQL server
  build and the NSIS desktop installer all succeed on windows-latest. Together with the Linux
  verification this meets M0's "both Linux and Windows CI builds" condition. (The installed
  Windows app has not been exercised by hand; that is the first thing to do on a Windows PC.)
- Fixes found during verification: the setup gate used to redirect to login before the
  recovery key could be read (fixed and tested); the embedded database start now stops a
  stale or orphaned server first; `madrasa-server` handles SIGTERM so the bundled database
  shuts down cleanly.
