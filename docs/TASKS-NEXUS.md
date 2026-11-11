# Madrasa Nexus — alpha server

Goal: the Nexus hub (docs/spec.md §2) as a working alpha server in `crates/nexus`: schools
join and leave, each school's data is kept apart, the hub and each school stay in sync,
head office gets network-wide reports and can hold backup keys in escrow. Packaged as a
Docker image with Compose, Azure and Google Cloud templates.

The design is in `docs/adr/0005-nexus-hub-and-sync.md`. Read it first.

**Lite and Standard stay exactly as they are.** A school that has not joined a hub has no
sync triggers, no background task and no new behaviour; every existing test keeps passing.
Joining is a choice made in Settings → Nexus.

Not in this alpha (roadmap): shared identities and parent accounts across schools, head
office entering receipts or attendance, hub-to-hub links, pre-computed report warehouse.

## How to work

- Other sessions may work on this repository at the same time. **Start every stage from
  the latest `main`** (`git fetch origin && git checkout -b dev/nexus-N-name origin/main`),
  use the **next free migration number after pulling `main`**, and **rebase on `main`
  before opening the PR**.
- One stage at a time, in order. One branch and one pull request per stage. When CI is
  green, merge it into `main` and continue with the next stage.
- Follow every rule in CLAUDE.md. Use the demo data generator for all testing; never real
  data. No secret, token, passphrase or database URL is committed or logged.
- Pure rules go in `madrasa_core::sync`; SQL in `madrasa_db::sync`; the hub in
  `crates/nexus`; the school's side in `crates/api` (`nexus_link`).
- Tick `[x]` only when built and tested. Keep docs/PROGRESS.md updated after each stage;
  log blockers in docs/BLOCKERS.md and move on rather than guess.
- Be exact about what was verified. A template that was validated but never deployed is
  written down as "validated, not deployed".

## Stage 0 — Design

- [x] ADR 0005: hub, school separation, joining and leaving, sync engine, reports,
      escrow, packaging
- [x] This task list

## Stage 1 — Hub foundation and container

- [x] `crates/nexus` in the workspace: `madrasa-nexus` binary, configuration from
      environment variables (`NEXUS_DATABASE_URL`, `NEXUS_BIND` / `PORT`,
      `NEXUS_PUBLIC_URL`, `NEXUS_SETUP_TOKEN`), graceful shutdown, structured logs that
      never contain secrets
- [x] Hub migrations in a `nexus` schema: hub users, sessions, roles and permissions
      (`nexus.admin`, `nexus.reports`, `nexus.escrow`), hub settings, append-only hub
      audit log
- [x] API `/api/nexus/v1`: `GET /health`, first-run setup guarded by a one-time setup
      token, login, logout, `GET /auth/me`, hub users (list, create, disable, reset
      password); lockout after repeated failures; errors in the standard shape
- [x] Dockerfile (multi-stage, slim runtime, non-root, health check) and
      `deploy/docker-compose.yml` (hub + PostgreSQL 16 with a volume, optional Caddy for
      HTTPS), `.env.example` with no real secrets
- [x] CI: a job that builds the image, starts the Compose stack and waits for the health
      endpoint
- [x] Tests: setup once only, login and lockout, permission checks, audit rows; spec
      section for the hub

## Stage 2 — Sync engine: capture and field-level merge

- [x] `madrasa_core::sync`: the table policy (every table classified as field-level,
      append-only, attendance or not synced; excluded columns such as password hashes),
      the wire format, and the merge decision for one field (apply, ignore as stale, head
      office wins) as pure functions
- [x] Migration: `sync_state`, `sync_changes`, `sync_field_stamps`, `sync_overwrites`,
      `sync_parked` and the generic capture trigger function (unused until a school joins)
- [x] `madrasa_db::sync`: switch capture on and off, queue a full snapshot, collect a
      batch to send, apply a batch (new rows, changed fields, stamps), acknowledge
- [x] Head office wins when both sides changed the same field since the last sync; the
      losing value is written to `sync_overwrites` on both sides
- [x] Replays and stale changes are ignored; the clock folds in received stamps
- [x] Records that cannot be applied are retried, then parked with the reason
- [x] Tests: policy covers every table in the migrations; an unjoined database has no
      triggers; two databases converge after edits on each side; different fields of one
      record both survive; same-field conflict resolves to head office with a log row on
      both sides; a replayed batch changes nothing; password hashes never leave

## Stage 3 — Sync engine: receipts and attendance always merge

- [x] Receipts and other append-only tables: union of both sides; voucher paid amount and
      status recomputed from receipts after every merge
- [x] Receipt number collisions keep both receipts (the later one renumbered with a
      suffix, logged); a receipt reversed on both sides is reversed once
- [x] Attendance: sheets and entries created separately on both sides for the same
      section, student and day are folded into one; the later mark stands and the earlier
      is logged; sheet totals recomputed; staff attendance the same
- [x] Tests with the fee and attendance repository functions on both sides: receipts on
      both sides add up on the voucher; nothing is lost in either direction; duplicate
      sheets merge to one with every student present; totals match the entries; the
      merge is the same whichever side syncs first

## Stage 4 — Joining and leaving

- [x] Hub: school register, invitations with one-time expiring join codes, a schema per
      school created and migrated on join, sync tokens (hash stored), `POST /join`,
      `/sync/push`, `/sync/pull`, `/sync/ack`, `/sync/leave`; remove a school; purge an
      archive (confirm with the school code)
- [x] Every school query runs with `search_path` set to that school's schema only
- [x] School side (`crates/api`): feature `nexus_link`; `GET /nexus/link`, join, sync
      now, leave; the token sealed in settings; a background task that syncs every five
      minutes and survives being offline; HTTPS required except for `localhost`
- [x] Desktop: Settings → Nexus page (join form, status, last sync, pending and parked
      counts, the overwrite log, leave), English and Urdu
- [x] Version check: the hub refuses a school on a newer schema with a clear message
- [x] Tests: end to end over real HTTP (a school API and a hub in one test): join, first
      upload, edits both ways, leave; a removed school is told and unlinks; two schools
      with identical ids and codes never see each other; one school's token cannot reach
      another's data; purge leaves the other school intact; after leaving, the school has
      no triggers and its data is unchanged

## Stage 5 — Network-wide reports

- [ ] Hub reports across all active schools: network summary, enrolment by school, class
      and gender, fee billing and collection by school and month (totals per currency),
      attendance by school and month, sync health
- [ ] JSON, CSV and Excel; filters for school and period; permission `nexus.reports`
- [ ] Schools that left or were removed are excluded
- [ ] Tests against two demo schools: figures equal each school's own reports

## Stage 6 — Backup key escrow

- [ ] `madrasa_core::escrow`: escrow key pair, private key wrapped by the escrow
      passphrase, seal a school's backup data key to the public key, open it; unit tests
      including wrong passphrase and tampering
- [ ] Hub: escrow key created at setup; deposits stored per school; release with
      permission `nexus.escrow`, the passphrase and a reason; shown once; hub audit row
- [ ] School side: deposit from Settings → Nexus by entering the backup password;
      releases are reported to the school at its next sync and shown there
- [ ] Restore accepts a key released by head office as a third way to open a backup
- [ ] Tests: deposit, release, restore a real backup with the released key; a release
      without the passphrase fails; the school sees the release; purge deletes the deposit

## Stage 7 — Hub console and head office corrections

- [ ] Web console served by the hub, built from the desktop app's components and
      translations (English and Urdu, RTL): sign in, schools and invitations, sync status,
      overwrite log, parked records, reports with downloads, key release, hub users
- [ ] Head office corrections: search a school's students, guardians and staff and edit
      names and contact details; edit the school profile; the change syncs down
- [ ] Tests: component tests for the console; an end-to-end test that a head office
      correction reaches the school and beats a concurrent school edit, with the log row

## Stage 8 — Deployment templates

- [ ] Azure: Bicep template (Container Apps + PostgreSQL Flexible Server), parameters
      file, deployment guide
- [ ] Google Cloud: Terraform module (Cloud Run + Cloud SQL + Secret Manager), example
      variables, deployment guide
- [ ] Release workflow publishes the image to GitHub Container Registry
- [ ] CI validates both templates; the guides state plainly that they were validated but
      not deployed from the build environment
- [ ] Operations notes: backups of the hub database, upgrades, rotating a sync token

## Stage 9 — Close-out

- [ ] Demo network: a command that seeds a hub with demo schools for evaluation
- [ ] TESTING.md section for Nexus (hand tests, including a first real cloud deployment)
- [ ] Spec sections for the hub, the sync tables and the endpoints; permissions matrix
- [ ] docs/PROGRESS.md: summary for the owner, what to test by hand, decisions left

## Done when

A school running Lite or Standard can join a hub started with `docker compose up`, upload
its data, receive a head office correction, see what was overwritten, deposit its backup
key, and leave with its data intact; head office sees network-wide reports across at
least two schools whose data never mixes; and every existing Lite and Standard test still
passes.
