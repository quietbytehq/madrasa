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
- Colors: bg #F7F9FC, card #FFFFFF, navy #142F50, blue #2864C5, selection #E8F1FF,
  success #19835A/#DDF3E8, warning #D99A27/#FFF0D5, lavender #E8E1FA,
  text #172B4D, muted #64748B, border #DFE7F1.
- Font: Inter, system sans fallback. Thin borders, subtle shadows, restrained radii.
- Layout: light blue-grey sidebar (modules), 56–64px top bar (search Ctrl+K, campus, notifications, help, user), main workspace.
- Must work at 1366×768 and 1920×1080. Visible focus states, keyboard navigation.
- Dark mode via tokens (follow OS, user override). Printouts always light.

## Folder layout
```
apps/desktop/        Tauri app (src-tauri/ Rust shell, src/ React UI)
crates/api/          Axum API server
crates/core/         domain logic (fees, grading, permissions) — pure, well tested
crates/db/           migrations, queries
crates/demo-data/    fake data generator
docs/                spec.md, architecture decisions (docs/adr/)
```

## Workflow
- Work through docs/TASKS-M0.md in order. Tick each task when done and tested.
- Small commits with clear messages; push often.
- Record any significant decision as a short ADR in docs/adr/.
- If blocked, write the blocker in docs/BLOCKERS.md and move to the next task instead of guessing.
- Keep a running log in docs/PROGRESS.md: what was done, what's tested, what's left.
