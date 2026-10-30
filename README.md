# Madrasa

**Madrasa** is a free, open-source desktop School Information System (SIS) for schools
that need reliable, offline-capable administration: students, attendance, examinations,
fees and finance, teachers, reports and backups.

Status: **pre-alpha**. Nothing here is ready for a real school yet.

## Editions

| Edition | Scope | Status |
| --- | --- | --- |
| **Lite** | One school, up to ~1,200 students and ~250 staff accounts (soft limits). All-in-one desktop install with a bundled database. | In development (milestone M0: foundation) |
| **Standard** | All modules, server plus multiple clients. | Planned |
| **Madrasa Nexus** | Network hub that joins several schools into one tenant, like an Active Directory forest root. | Planned |

Lite is built first, but the data model and backend are written so Standard and Nexus plug in
later without rewrites (UUID v7 keys, `school_id` on every school-owned row, sync-ready
columns, edition feature flags).

## Stack

- Desktop shell: [Tauri 2](https://tauri.app) (Rust)
- UI: React + TypeScript + Vite + Tailwind CSS, Lucide icons, Recharts
- Backend: Rust + Axum REST API under `/api/v1`
- Database: PostgreSQL with `sqlx` migrations (bundled for all-in-one installs)
- Crypto: `argon2` (Argon2id) and `aes-gcm` only

## Repository layout

```
apps/desktop/        Tauri app (src-tauri/ Rust shell, src/ React UI)
crates/api/          Axum API server
crates/core/         domain logic (fees, grading, permissions): pure, well tested
crates/db/           migrations, queries
crates/demo-data/    fake data generator
docs/                spec.md, task lists, progress log, architecture decisions (docs/adr/)
```

## Getting started (developers)

Prerequisites: Rust (stable), Node 22, pnpm, PostgreSQL 16 for running the tests, and the
[Tauri 2 Linux/Windows prerequisites](https://tauri.app/start/prerequisites/).

```bash
pnpm install
cargo build --workspace
pnpm --filter madrasa-desktop tauri dev
```

Database-backed tests need a PostgreSQL server. Set `MADRASA_TEST_DATABASE_URL` (for example
`postgres://postgres:postgres@localhost:5432/madrasa_test`) before running `cargo test`;
without it those tests are skipped.

To try the app with fake data, load the demo school into a database (never run this against a
real school's database; it refuses if one is present):

```bash
MADRASA_DATABASE_URL=postgres://postgres:postgres@localhost:5432/madrasa cargo run -p madrasa-demo-data --bin madrasa-demo -- load --yes
```

Continuous integration (`.github/workflows/ci.yml`) runs lint, type checks, tests and desktop
builds on Linux and Windows; tagged `v*` releases publish unsigned `.deb`, AppImage and Windows
NSIS installers as a draft pre-release.

See `docs/spec.md` for the product specification, `docs/TASKS-M0.md` for the current milestone
and `docs/PROGRESS.md` for what has actually been built and tested.

## License

Apache License 2.0. See `LICENSE`.
