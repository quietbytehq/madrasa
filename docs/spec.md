# Madrasa — Product and Technical Specification

Status: draft v0.1 (written during milestone M0). `CLAUDE.md` holds the non-negotiable rules;
this document expands them into product scope, architecture, data model and conventions.
When code and spec disagree, fix one in the same change so they never drift.

## 1. What Madrasa is

Madrasa is a free, open-source (Apache 2.0) desktop School Information System. It targets
schools in Pakistan and similar markets first (PKR, Urdu, intermittent connectivity), but
nothing in it is country-specific beyond defaults.

Design goals, in priority order:

1. **Works offline.** A single PC with no internet must run the full Lite product, including
   fonts, database and backups.
2. **Trustworthy money and records.** Fee and finance data is exact, immutable where it must
   be, and fully audited.
3. **Grows without rewrites.** Lite is one school on one PC. Standard adds a server and
   several clients. Nexus joins many schools into one network. The schema, IDs and API are
   designed for the largest case from day one.
4. **Fast for clerks.** Compact, information-dense, keyboard-friendly UI.

## 2. Editions

| Edition | Deployment | Scope |
| --- | --- | --- |
| Lite | All-in-one desktop install, bundled PostgreSQL | One school. Soft limits: ~1,200 students, ~250 staff accounts. Core modules. |
| Standard | Server (API + PostgreSQL) plus desktop clients | All modules, multi-user over LAN/WAN, campuses. |
| Nexus | Network hub | Joins several schools into one tenant (an "AD forest root"): shared identities, cross-school reporting, parent accounts spanning schools. |

The edition is a value of the `edition` setting (`lite` | `standard` | `nexus`). One
feature-flag module (`madrasa_core::edition`) maps edition to enabled features. Both the API
and the UI consult it; the API is authoritative (a flag hidden in the UI is still enforced by
the backend).

Soft limits are advisory: the UI warns when a Lite school approaches them; nothing is
blocked.

## 3. Modules

M0 ships the shell only. Modules below are the planned Lite set; those marked *later* are
Standard/Nexus.

| Module | Lite | Notes |
| --- | --- | --- |
| Dashboard | yes | Summary cards, today's attendance, fee collection, alerts |
| Students | yes | Admissions, profiles, classes/sections, promotions, custom fields |
| Attendance | yes | Daily and per-period, staff attendance |
| Examinations | yes | Exams, marks entry, grading schemes, report cards |
| Fees & Finance | yes | Fee structures, invoices, receipts, concessions, ledgers, expenses |
| Teachers | yes | Staff profiles, assignments, timetable |
| Reports | yes | Printable reports (always light theme) |
| Backup | yes | Encrypted local backups, restore, recovery key |
| Settings | yes | School profile, academic years, roles, custom fields, edition |
| Campuses | later | Multi-campus school (Standard) |
| Parents portal | later | Parent accounts (Nexus-wide identities) |
| Transport, Library, Hostel, HR/Payroll | later | Standard |
| Network admin | later | Nexus hub console |

Unfinished modules are routable and show a clear "Coming soon" state; they are never
pretended to work.

## 4. Architecture

```
┌─────────────────────────── Desktop (Tauri 2) ───────────────────────────┐
│  React UI  ──HTTP/JSON──▶  Axum API (in-process in Lite)  ──sqlx──▶  PostgreSQL │
│  (apps/desktop/src)        (crates/api)                              (bundled)   │
└──────────────────────────────────────────────────────────────────────────┘
```

- The UI **only** talks to the REST API (`/api/v1`). It never touches the database.
- In Lite the Tauri shell starts the API on `127.0.0.1` on a free port and hands the base URL
  to the UI through a Tauri command. In Standard the same API crate runs as a standalone
  server (`madrasa-server` binary) and clients point at it. Nothing in the UI depends on where
  the API lives.
- `crates/core` holds pure domain logic (permissions, editions, fee and grading maths, crypto
  helpers, HLC). It has no I/O and is exhaustively unit tested.
- `crates/db` owns migrations and all SQL. Every mutation goes through a repository function
  that also writes the audit log inside the same transaction.
- `crates/demo-data` generates a clearly marked fake school. It never runs automatically.

### 4.1 PostgreSQL bundling

Decision recorded in `docs/adr/0001-postgresql-bundling.md`. Summary: Lite embeds PostgreSQL
binaries into the application (the `postgresql_embedded` crate with the `bundled` feature) and
runs a private cluster in the app data directory on a random localhost port with a generated
password. Standard and Nexus point at an external server through `MADRASA_DATABASE_URL`.

### 4.2 Runtime directories

| Purpose | Linux | Windows |
| --- | --- | --- |
| App data (cluster, config, logs) | `$XDG_DATA_HOME/madrasa` (default `~/.local/share/madrasa`) | `%APPDATA%\madrasa` |
| Backups (default) | `~/Documents/Madrasa Backups` | `Documents\Madrasa Backups` |

No secrets are written anywhere except the app data directory, with user-only permissions.

## 5. Data model rules

All tables follow these rules (enforced by migration tests):

1. Primary key `id UUID` generated as UUID v7 by the application.
2. Every school-owned table has `school_id UUID NOT NULL REFERENCES schools(id)`.
3. Every mutable table has `created_at TIMESTAMPTZ`, `updated_at TIMESTAMPTZ`,
   `updated_by UUID` (user id, nullable for system), `deleted_at TIMESTAMPTZ` (soft delete)
   and `hlc TEXT` (hybrid logical clock stamp, see 5.2).
4. Money is `NUMERIC(14,2)`; in Rust `rust_decimal::Decimal`. Never floats.
5. Receipts and other financial documents are immutable; corrections are reversal entries.
6. Core entities have a `custom JSONB NOT NULL DEFAULT '{}'` column. Allowed keys and types
   are declared in `custom_field_definitions`.
7. Deletes are soft (`deleted_at`). Hard deletes are reserved for purge jobs.

### 5.1 Foundation tables (M0)

| Table | Purpose |
| --- | --- |
| `schools` | One row per school. `code` is unique. `is_demo` marks generator output. |
| `users` | Local accounts: `username`, `full_name`, `email`, `password_hash` (Argon2id PHC string), `is_active`, `failed_login_count`, `locked_until`. |
| `roles` | Per-school roles. Built-in: `principal`, `admin`, `accountant`, `teacher`; `parent` reserved (Nexus). `is_system` rows cannot be deleted. |
| `permissions` | Grants: one row per (`role_id`, `code`). Codes are defined in `madrasa_core::permissions`. |
| `user_roles` | Many-to-many users ↔ roles. |
| `sessions` | Session tokens (hash only), expiry, last seen. |
| `settings` | Key/value (`value JSONB`) per school; `school_id` NULL for instance-wide keys such as `edition`. |
| `audit_log` | Append-only: `actor_user_id`, `action`, `entity`, `entity_id`, `old_value`, `new_value`, `at`. Triggers forbid UPDATE/DELETE. |
| `custom_field_definitions` | Registry of custom fields: `entity`, `key`, `label`, `field_type`, `required`, `options`. |

### 5.2 Hybrid logical clock

`hlc` is a 32-character, lexicographically sortable string:
`<unix_ms:013 zero-padded>-<counter:05 hex>-<node:12 hex>`. The node id is generated per
install and stored in settings. There is no sync engine yet; the column exists so that one can
be added without a migration of every table.

### 5.3 Audit log

Every create, update and delete executed by the application writes one `audit_log` row in the
same transaction. `old_value`/`new_value` are JSON snapshots of the row (secrets such as
password hashes are redacted before logging). Money changes are additionally required to
reference the financial document they touch (M1+).

## 6. API conventions

- Base path `/api/v1`. JSON request and response bodies. `snake_case` keys.
- Authentication: `Authorization: Bearer <session token>`. Tokens are 32 random bytes,
  base64url; the server stores only a SHA-256 hash. Default lifetime 12 hours, sliding.
- Errors: `{ "error": { "code": "...", "message": "..." } }` with an appropriate HTTP status
  (`400 validation`, `401 unauthenticated`, `403 forbidden` / `feature_unavailable`,
  `404 not_found`, `409 conflict`, `423 locked`, `500 internal`). Messages are plain language.
- Every endpoint except `/health`, `/setup/*` (before setup is complete), `/auth/login` and
  `/system/features` requires a session and a permission check in the handler. Permission
  checks live in the backend; the UI only hides things.
- Feature-gated endpoints return `403 feature_unavailable` when the edition does not enable
  the feature.
- Pagination: `?page=1&page_size=50` with `{ items, page, page_size, total }`.

### 6.1 M0 endpoints

| Method | Path | Auth | Purpose |
| --- | --- | --- | --- |
| GET | `/health` | none | `{ status: "ok", version, database: "ok" }` |
| GET | `/system/features` | none | `{ edition, features: [...] }` |
| GET | `/setup/status` | none | `{ needs_setup }` |
| POST | `/setup` | none, only while `needs_setup` | Creates school, settings, admin account, backup keys; returns recovery key once |
| POST | `/auth/login` | none | `{ username, password }` → `{ token, expires_at, user }` |
| POST | `/auth/logout` | session | Revokes the token |
| GET | `/auth/me` | session | Current user, roles, permissions |
| GET | `/users` | `users.manage` | List users (admin only) |
| GET | `/schools/current` | `school.read` | School profile |
| GET | `/audit` | `audit.read` | Recent audit entries |
| GET | `/nexus/status` | `nexus.admin`, feature `nexus_hub` | Stub, proves feature gating |

## 7. Security

- Passwords: Argon2id (`argon2` crate, OWASP-recommended parameters: m=19 MiB, t=2, p=1),
  PHC string stored in `users.password_hash`.
- Lockout: after 5 consecutive failed logins the account is locked for 15 minutes
  (`users.locked_until`); successful login resets the counter. Lockouts are audit-logged.
- Backup encryption: at setup a random 256-bit data key (DEK) is generated. It is wrapped
  twice with AES-256-GCM: once with a key derived (Argon2id) from the backup password, once
  with a key derived from a random recovery key. The recovery key is shown once, printable, as
  8 groups of 4 Crockford-base32 characters. Only wrapped keys and salts are stored (in
  `settings`); the DEK and the passwords are never stored.
- All crypto via audited crates: `argon2`, `aes-gcm`, `sha2`, `rand`. No hand-rolled crypto.
- No real data in the repository. Tests and screenshots use the demo generator.

## 8. UI

### 8.1 Theme (v1)

Light, compact, information-dense. No hero banners. Tokens are CSS variables (light and dark)
exposed through Tailwind:

| Token | Light |
| --- | --- |
| `--bg` | `#F7F9FC` |
| `--card` | `#FFFFFF` |
| `--navy` | `#142F50` |
| `--blue` | `#2864C5` |
| `--selection` | `#E8F1FF` |
| `--success` / `--success-bg` | `#19835A` / `#DDF3E8` |
| `--warning` / `--warning-bg` | `#D99A27` / `#FFF0D5` |
| `--lavender` | `#E8E1FA` |
| `--text` | `#172B4D` |
| `--muted` | `#64748B` |
| `--border` | `#DFE7F1` |

Font: Inter (Latin), Noto Nastaliq Urdu (Urdu), both bundled. Thin borders, subtle shadows,
restrained radii. Dark mode via the same tokens (follow OS, user override). Printouts are
always light.

### 8.2 Layout

- Left sidebar (light blue-grey) listing modules; collapsible.
- Top bar 56–64 px: global search (Ctrl+K), campus selector (Standard+), notifications, help,
  user menu (profile, language, theme, logout).
- Main workspace with breadcrumbs.
- Must be usable at 1366×768 and 1920×1080. Visible focus rings; everything reachable by
  keyboard.

### 8.3 Internationalisation

All strings go through `t()` from the i18n layer (`apps/desktop/src/i18n`). Languages:
English (`en`, LTR) and Urdu (`ur`, RTL). Switching language sets `<html lang dir>` and the
layout uses logical CSS properties so it mirrors correctly. Numbers and dates are formatted
with `Intl` for the active locale.

## 9. Setup wizard (first run)

Shown when `/setup/status` reports `needs_setup`. Steps:

1. School: name, code (short, unique, uppercase), city.
2. Academic year start month (April or August), currency (PKR, fixed in M0).
3. Admin account: full name, username, password (min 10 chars).
4. Backup password and recovery key: user sets a backup password; the app generates and shows
   a printable recovery key which must be acknowledged before finishing.

Completing the wizard creates the school, settings, default roles with permissions, the admin
user (roles `principal` and `admin`) and the wrapped backup keys, all in one transaction, and
audit-logs the creation.

## 10. Demo data

`crates/demo-data` builds a fake school ("DEMO School") with ~1,200 students, ~120 staff and
classes K–12 (sections A/B/C) from a fixed seed, so output is reproducible. Everything is
visibly marked DEMO (school name, `is_demo = true`, staff usernames prefixed `demo.`).

It runs only when explicitly invoked (`madrasa-demo load --yes`) and refuses to run when the
database already contains a non-demo school.

## 11. Testing

| Area | How |
| --- | --- |
| Domain logic (core) | Rust unit tests, no I/O |
| Migrations and repositories (db) | Integration tests against PostgreSQL (skipped when `MADRASA_TEST_DATABASE_URL` is unset); each test gets its own schema |
| API | Axum router tests with `tower::ServiceExt`, same DB rule |
| UI helpers and components | Vitest + Testing Library, including RTL rendering |
| CI | GitHub Actions on Linux and Windows: lint, format, type-check, tests, Tauri build |

## 12. Milestones

- **M0 Foundation** (`docs/TASKS-M0.md`): installs, setup wizard, login, themed empty
  dashboard, CI builds.
- **M1 Students & Attendance**, **M2 Fees & Finance**, **M3 Examinations & Reports**,
  **M4 Backup/Restore & Packaging polish**, then Standard.

## 13. Open questions

Tracked in `docs/BLOCKERS.md`. Decisions are recorded as ADRs in `docs/adr/`.
