# CLAUDE.md — Madrasa

Madrasa is a free, open-source (Apache 2.0) desktop School Information System.
Repo: quietbytehq/madrasa. Full spec: docs/spec.md (read it before starting).

## Editions
- **Lite** (build now): single school, up to ~1,200 students / 250 staff accounts (soft limits).
- **Standard** (later): all modules, server + clients.
- **Madrasa Nexus** (later): network hub joining schools into one tenant, like an AD forest root.
Lite is built first. Write Lite so Standard and Nexus plug in later without rewrites.

## Stack
- Desktop: Tauri 2 (Rust). Not Electron.
- UI: React + TypeScript + Vite + Tailwind CSS + Lucide icons + Recharts.
- Backend: Rust + Axum REST API (versioned under /api/v1). The UI talks to the API, never to the DB directly.
- Database: PostgreSQL. Migrations via sqlx. Bundled for all-in-one installs.
- Crypto: audited crates only (aes-gcm, argon2). Never hand-roll crypto.

## Non-negotiable rules
1. **IDs:** every table's primary key is a UUID v7. Every school-owned row has `school_id`.
2. **Sync-ready:** every mutable table has `created_at`, `updated_at`, `updated_by`, `deleted_at` (soft delete) and a hybrid-logical-clock `hlc` column. No sync engine yet — just the columns.
3. **Money:** use `NUMERIC(14,2)` / `rust_decimal`. Never floats. Receipts are immutable; corrections are reversal entries. Every money change is audit-logged.
4. **Audit log:** append-only table recording who, what, when, old value, new value.
5. **Custom fields:** JSONB `custom` column on core entities, defined by a `custom_field_definitions` registry.
6. **Edition flags:** features gated by an `edition` setting (lite | standard | nexus) via a single feature-flag module.
7. **Permissions are enforced in the backend**, never only in the UI.
8. **No real data:** never commit real student, staff or fee data, credentials or secrets. Use the demo data generator.
9. **i18n:** all UI strings go through the translation layer (English + Urdu). Layout must work in RTL. Urdu font: Noto Nastaliq Urdu.
10. **Tests:** fee calculations, grading, permissions and migrations must have automated tests. Don't mark a task done until its tests pass.
11. **Honesty:** never claim something works unless it has been built and tested. Stub unfinished modules with a clear "Coming soon" state.

## Design (v1 theme)
Light, compact, information-dense desktop UI. No hero banners.
- Colors (brand, assets/brand): bg #F7F9FC, card #FFFFFF, navy #0B2557, blue #0757BA,
  gold #F5B13B, selection #E6EFFC, success #19835A/#DDF3E8, warning #D99A27/#FFF0D5,
  lavender #E8E1FA, text #15264A, muted #5A6A82, border #DEE6F2. Dark values: docs/spec.md §8.1.
- Font: Inter, system sans fallback; Merriweather Black (`font-display`) only for the product
  name. Thin borders, subtle shadows, restrained radii.
- Logo and icons: assets/brand. Edit assets/brand/tools/build_svg.py, not the SVGs; regenerate
  icons with assets/brand/generate-icons.sh.
- Layout: light blue-grey sidebar (modules), 56–64px top bar (search Ctrl+K, campus, notifications, help, user), main workspace.
- Must work at 1366×768 and 1920×1080. Visible focus states, keyboard navigation.
- Dark mode via tokens (follow OS, user override). Printouts always light.

## Folder layout
```
apps/desktop/        Tauri desktop app: src-tauri/ (Rust shell: window, tray, starts the
                     server or the LAN client's proxy from crates/api), src/ (React UI)
apps/desktop/src/console/   the head office console of the Nexus hub; built on its own
                     (pnpm build:console) and served by the hub, not part of the desktop app
apps/parent/         Parent app (Tauri 2, Android): src-tauri/, src/ (React UI, sharing the
                     desktop's components and translations); talks to a Nexus hub, never
                     to a school
crates/api/          Axum API server of a school (/api/v1, /api/ro/v1), the LAN client's
                     proxy and offline queue; bin madrasa-server
crates/core/         domain logic (fees, grading, permissions, imports, backup format) — pure,
                     well tested
crates/db/           migrations, queries
crates/demo-data/    fake data generator
crates/nexus/        Nexus hub: its own API (/api/nexus/v1), schema and migrations, sync,
                     backup-key escrow, parents' accounts; bin madrasa-nexus
crates/manual/       builds the user manual PDFs from docs/manual (Typst)
assets/brand/        logo and icons; edit tools/build_svg.py, regenerate with generate-icons.sh
deploy/nexus/        the hub's Dockerfile, Compose stack, Caddyfile; azure/ (Bicep), gcp/ (Terraform)
e2e/                 browser tests (Playwright): desktop/, parent/, console/, support/
scripts/             e2e.sh, load-test.sh, soak-test.sh, upgrade-test.sh (load/ holds their
                     seed and drivers), manual-screenshots.sh, build-manual.sh,
                     build-parent-apk.sh, sign-apk.sh, nexus-smoke.sh, set-version.sh
docs/                spec.md (source of truth), PROGRESS.md, BLOCKERS.md, TASKS-*.md,
                     permissions.md, SECURITY.md, RELEASING.md, urdu-review.md
docs/adr/            architecture decisions, numbered
docs/manual/         the manual's source: manual.yaml (EN and UR text), manual.typ (layout),
                     screenshots/{en,ur}/ (made by tools/, never by hand)
docs/user-guide/     en.md, ur.md
docs/nexus/          deploy.md (running a hub)
docs/upstream/       bug reports written for libraries we use; not posted without the owner's say
docs/testing/        sample files for testers
.github/workflows/   ci.yml (every push), release.yml (a v* tag, or run by hand)
TESTING.md           what a person tests by hand, release by release
```

## Workflow
- Work through docs/TASKS-M0.md in order. Tick each task when done and tested.
- Small commits with clear messages; push often.
- Record any significant decision as a short ADR in docs/adr/.
- If blocked, write the blocker in docs/BLOCKERS.md and move to the next task instead of guessing.
- Keep a running log in docs/PROGRESS.md: what was done, what's tested, what's left.
