# 0001 — Bundling PostgreSQL for all-in-one (Lite) installs
Date: 2026-10-30
Status: accepted

## Context
Lite must run on a single PC with no internet and no separately installed database, on Linux
and Windows. The data model is PostgreSQL-only (JSONB, NULLS NOT DISTINCT, plpgsql triggers),
so SQLite is not an option, and asking a school clerk to install and configure PostgreSQL is not
acceptable.

Options considered:
1. **Embed binaries in the executable** with the `postgresql_embedded` crate (`bundled`
   feature). Portable PostgreSQL builds from theseus-rs/postgresql-binaries are downloaded at
   build time and included in the binary; at first run they are extracted to the app data
   directory, a cluster is initialised and started on a random loopback port.
2. **Ship binaries as installer resources** and drive `initdb`/`pg_ctl` ourselves.
3. **Require an external server** (document how to install PostgreSQL).

## Decision
Option 1 for Lite; option 3 (external URL) for Standard/Nexus and for development.

- `crates/api::bootstrap::DatabaseSource::from_env()` picks `External` when
  `MADRASA_DATABASE_URL` is set, otherwise `Embedded` under `MADRASA_DATA_DIR` or the platform
  data directory (`~/.local/share/madrasa`, `%APPDATA%\madrasa`).
- The `embedded-postgres` cargo feature (default-on in the desktop crate, off in `madrasa-api`
  and CI test builds) pulls `postgresql_embedded` with `bundled`. The major version is pinned
  in `.cargo/config.toml` via `POSTGRESQL_VERSION`.
- Layout: `<data>/pg/bin` (extracted binaries), `<data>/pg/data` (cluster), `<data>/pg/password`
  (generated superuser password, mode 0600). The server listens on `127.0.0.1` only.
- The desktop shell starts the database and the API on a tokio thread and stops both when the
  window closes.

## Consequences
- Build machines need internet once per PostgreSQL version (archive cached under
  `~/.theseus/postgresql`). CI caches it.
- The executable grows by the compressed PostgreSQL archive (~30 MB).
- Upgrading the major version later requires a `pg_upgrade`/dump-restore step; this is the
  Backup module's responsibility (M4). Minor upgrades are in place.
- Standard edition reuses the exact same API crate against an external server; nothing in the
  schema or queries depends on the bundling.
