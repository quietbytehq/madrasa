# ADR 0005: Madrasa Nexus — hub server, school separation, sync engine and key escrow

## Status

Proposed (Nexus alpha, stage 0). Accepted when the stage 0 pull request is merged.

## Context

`docs/spec.md` §2 describes Nexus in one line: a network hub that joins several schools
into one tenant, with shared identities, cross-school reporting and parent accounts. The
foundation was laid for it from M0: every table has a UUID v7 key, a `school_id`, soft
delete, and a hybrid logical clock stamp (`hlc`), "so that a sync engine can be added
without a migration of every table" (§5.2). No sync engine, hub or deployment exists yet.

The alpha asked for is the **server**: a hub that schools can join and leave, that keeps
every school's data apart, syncs with each school, answers network-wide reports and holds
backup keys in escrow; packaged as a Docker image with Compose, Azure and Google Cloud
templates. Shared identities and parent accounts stay on the roadmap.

Facts about today's code that shape the design:

- All 84 tables have a single-column `id` primary key and an `hlc` column; all but
  `schools` have `school_id`. Nothing is ever hard-deleted; deletes set `deleted_at`.
- Every repository update already writes a fresh `hlc` (one exception, the last-used
  time of an API key, which is not synced).
- Receipts (`fee_receipts`) and the audit log are immutable by database trigger.
- Schools run on one office PC, often offline, behind home-grade routers. The hub cannot
  call a school; the school must call the hub.
- Lite and Standard must keep working with no hub at all, exactly as they do now.

## Decision

### 1. One new crate, one new binary

`crates/nexus` builds `madrasa-nexus`, an Axum server with its own API (`/api/nexus/v1`),
its own users and its own small set of tables. It depends on `madrasa-core` (rules,
crypto, HLC) and `madrasa-db` (the school schema, migrations and the sync functions).

The sync rules live where both sides can use them:

- `madrasa_core::sync`: pure code. The table policy (how each table syncs), the wire
  format and the merge decisions. Unit tested without a database.
- `madrasa_db::sync`: the SQL. Change capture, collecting what to send, applying what
  arrives, the overwrite log. Used by the school's API and by the hub, so both ends run
  the same code.
- `crates/api` gains a small `nexus_link` module (join, status, sync now, leave, the
  background sync task) and one settings page in the desktop app.

### 2. Each school's data in its own PostgreSQL schema

The hub database has a `nexus` schema (hub users, sessions, the school register,
invitations, escrow, hub audit log) and **one schema per school**, named
`t_<32 hex digits>`, holding the full Madrasa schema created by the same migrations a
school runs.

- Every statement that touches a school runs in a transaction that first sets
  `search_path` to that school's schema only. A query cannot reach another school's rows
  even by mistake, because the other tables are not on its path.
- A school's sync token maps to exactly one schema. Tokens are 32 random bytes; the hub
  stores only the SHA-256 hash.
- Removing a school's data is `DROP SCHEMA … CASCADE`: complete, and it cannot touch
  another school. No per-table deletes, no fight with the append-only triggers.
- Two schools may have the same codes, usernames, receipt numbers, even the same row ids
  (two installs of the demo school): nothing collides.

Rejected: one shared set of tables filtered by `school_id`. It is the lighter design, and
the columns exist, but separation would then depend on every query carrying the right
filter, leaving would mean deleting from 84 tables in foreign-key order, and instance-wide
rows (settings without a school, built-in roles) would clash between schools.

### 3. Joining and leaving

Joining:

1. Head office creates an **invitation** for a school in the hub. The hub shows a join
   code once (it expires, default seven days, and works once).
2. The school's principal opens Settings → Nexus, enters the hub address and the join
   code. The school's API calls the hub over HTTPS (plain HTTP is accepted only for
   `localhost`, for testing).
3. The hub creates the school's schema, runs the migrations in it, and returns a sync
   token and the hub's escrow public key. The school stores the token sealed
   (ADR 0004).
4. The school switches change capture on (section 4) and queues every existing row for
   the first upload. From then on it syncs every five minutes and on "Sync now".

Leaving, either side may start it:

- The school leaves (Settings → Nexus → Leave): it tells the hub, switches change capture
  off and forgets the token. **Its own data is untouched** and it carries on alone.
- Head office removes the school: the hub refuses its token from then on; at its next
  sync the school is told, shows it, and unlinks itself the same way.
- In both cases the hub keeps the school's schema as a read-only archive, left out of
  every report, until head office purges it (typing the school code to confirm). Purging
  also deletes the escrowed key. Rejoining needs a new invitation and starts a new schema.

A school that is not joined has no sync triggers, no background task and no extra
queries. That is how "Lite and Standard keep working on their own" is guaranteed rather
than hoped for; a test asserts the absence of triggers on an unjoined database.

### 4. The sync engine

**Two replicas per school.** A school's data lives in two places: the school's own
database and its schema in the hub. Head office users edit the hub copy; school users edit
the school copy. Sync is always between these two.

**Change capture.** A migration adds one generic trigger function and five tables to the
school schema (they exist on every install and stay empty until a school joins):

| Table | Holds |
| --- | --- |
| `sync_state` | One row: whether sync is on, and whether this copy is the `school` or the `head_office` side |
| `sync_changes` | What still has to be sent: one row per changed record (table, row id, the changed fields, a sequence number) |
| `sync_field_stamps` | The field-level timestamps: for each field changed after its record was created, the HLC stamp of the last change and which side made it |
| `sync_overwrites` | The overwrite log: every value that sync replaced or refused, with both values, both stamps and the time |
| `sync_parked` | Incoming records that could not be applied yet (for example a duplicate admission number), with the reason; retried on every sync |

When a school joins, the trigger is attached to every synced table. On each insert or
update it compares the old and new row, notes which fields changed in `sync_changes`, and
stamps each changed field in `sync_field_stamps` with the row's new `hlc`. No repository
function changes: capture happens in the database, so no code path can forget it.

**The round.** The school calls the hub; the hub never calls the school.

1. *Push.* The school sends up to 500 changed records (the changed fields with their
   stamps; the whole row for new records). The hub applies them and answers with what it
   refused.
2. *Pull.* The school asks for head office's changes and applies them.
3. *Acknowledge.* The school confirms; each side forgets what the other now has.

Every batch carries a sequence number, so a batch sent twice after a dropped connection
is applied once. An interrupted round is simply repeated. The school's clock folds in
every stamp it receives (`Hlc::observe`), so a later local edit always carries a later
stamp, whatever the PC's wall clock says.

**Merging, by kind of table.** The table policy in `madrasa_core::sync` puts every table
in exactly one class. A test fails if a migration adds a table without classifying it.

*Field-level (the default: students, guardians, staff, classes, fee structures, vouchers,
marks, and so on).*

- Different fields of the same record changed on the two sides: both changes are kept.
- The same field changed on one side only since the last sync: that change is applied.
- **The same field changed on both sides since the last sync: head office wins.** At the
  hub the school's value is refused; at the school the head office value replaces the
  local one. Either way the losing value goes into `sync_overwrites` on both sides, with
  the field, both values, both stamps and who made the losing edit, so nothing vanishes
  silently and the school can see what head office changed under it.
- A change that arrives with a stamp older than the one already recorded for that field
  (a replay, or a restore from an old backup) is ignored.
- `updated_at`, `updated_by` and `hlc` follow the later stamp and are never reported as
  conflicts; they change with every edit.

*Append-only (receipts, the audit log, admission events, stock movements, API call
log).* Records are never changed after they are written, so there is nothing to
overwrite: **the result is always the union of both sides.** For receipts:

- After receipts arrive, each affected voucher's paid amount and status are recomputed
  from its receipts on that side. They are derived values and are not synced as fields, so
  two receipts taken on the two sides for one voucher add up instead of one replacing the
  other.
- If two different receipts carry the same receipt number (possible only after a restore
  from an old backup), both are kept: the one created later gets a suffix (`-2`) on both
  sides, and the renumbering is logged.
- If both sides reversed the same receipt, the reversal is counted once.

*Attendance (student sheets and entries, staff attendance).* **Marks from both sides are
always merged; head office does not win here**, because the school is the one that saw
who was present.

- Entries for different students or days are simply combined.
- Two sheets for the same section and day, or two entries for the same student and day,
  created separately on the two sides, are folded into one (the earlier record survives;
  the other is soft-deleted and its entries move across).
- Where both sides marked the same student on the same day, the later mark stands and the
  earlier one is logged in `sync_overwrites`.
- Sheet totals (present, absent, late, leave) are recomputed after every merge.

*Not synced.* Sessions, API keys and their call log, backup runs, import batches, number
sequences and settings stay on the side that made them. Password hashes, lockout counters
and sealed secrets are never sent: the hub stores a placeholder that matches no password.

**When a record cannot be applied.** A record that breaks a database rule on arrival (a
second student with the same admission number, a child before its parent) is retried at
the end of the batch; if it still fails it is parked with the reason and retried at every
sync. Parked records are shown on the school's Nexus page and in the hub. They never stop
the rest of the sync.

**Versions.** Each request carries the school's schema version. The hub must be the same
version or newer; if the school is newer the hub refuses with "update the hub first".
Fields the receiver does not know are ignored.

### 5. Network-wide reports

The hub answers reports by running the same query in each active school's schema and
combining the rows, with the school as a column. The alpha has: network summary
(students, staff, sections, last sync per school), enrolment by school, class and gender,
fee billing and collection by school and month, attendance by school and month, and sync
health (pending, parked, overwritten). JSON, CSV and Excel. Money is never added across
currencies: totals are per currency.

Rejected for now: a warehouse of pre-computed figures. At the size of a school network
(tens of schools) per-schema queries are fast enough and are always exact.

### 6. Backup key escrow

Today a school that loses both its backup password and its recovery key has lost its
backups for good. Escrow lets head office help, without head office being able to read
backups casually.

- At hub setup the hub creates an X25519 **escrow key pair**. The private key is stored
  only wrapped with a key derived (Argon2id) from an **escrow passphrase** that the first
  administrator chooses and that is never stored. A copy of the hub database does not
  reveal any school's key.
- A school deposits its key from Settings → Nexus by entering its backup password once:
  the app unwraps the backup data key, seals it to the hub's escrow public key
  (X25519 + HKDF + AES-256-GCM, the construction backups already use) and uploads the
  sealed blob. Depositing is the school's choice, not a condition of joining.
- Release: a hub user with the `nexus.escrow` permission enters the escrow passphrase and
  a reason. The hub shows the school's key once as a printable code. Restore accepts it
  as a third way to open a backup ("key released by head office").
- Every release is written to the hub's audit log and is shown to the school on its next
  sync (who, when, why). A release cannot be hidden from the school.

If the escrow passphrase is lost, the escrowed keys cannot be released; schools still
have their own password and recovery key. Only audited crates are used (`aes-gcm`,
`argon2`, `x25519-dalek`, `hkdf`); nothing is hand-rolled.

### 7. Hub users and the console

The hub has its own accounts (Argon2id, lockout, session tokens: the same code as the
school API) and three permissions: `nexus.admin` (schools, invitations, users),
`nexus.reports` (reports) and `nexus.escrow` (release keys). The first administrator is
created at first run with a one-time setup token printed in the server log (or supplied
in `NEXUS_SETUP_TOKEN`).

A small web console is served by the hub: schools and invitations, sync status, the
overwrite log, reports, key release, and correcting a few fields of a school's records
(school profile, student, guardian and staff names and contact details) so that head
office edits flow down and the conflict rule can be seen working. It is built from the
desktop app's own components and translation layer (English and Urdu, RTL), as a second
build of `apps/desktop` without the Tauri shell.

### 8. Packaging and deployment

- **Docker image**: multi-stage build, Debian slim runtime, non-root user, health check.
  Published to GitHub Container Registry by the release workflow.
- **Docker Compose**: the hub, PostgreSQL 16 with a named volume, and an optional Caddy
  service for automatic HTTPS. Secrets come from an `.env` file that is not committed.
- **Azure**: a Bicep template for Azure Container Apps with Azure Database for
  PostgreSQL Flexible Server.
- **Google Cloud**: a Terraform module for Cloud Run with Cloud SQL for PostgreSQL and
  Secret Manager.
- Configuration is by environment variables only (`NEXUS_DATABASE_URL`, `NEXUS_BIND` or
  `PORT`, `NEXUS_PUBLIC_URL`, `NEXUS_SETUP_TOKEN`).

CI builds the image, starts the Compose stack and checks the health endpoint, and
validates both templates (`az bicep build`, `terraform validate`). **The templates are
not deployed by CI and have not been deployed from this workspace**: no cloud
subscription is available here. The first real deployment is a manual test for the owner
(TESTING.md), and the documents say so.

## Consequences

- Lite and Standard are unchanged unless a school chooses to join. The migration adds
  empty tables and an unused function to every install.
- The hub stores a full copy of each joined school's records (without password hashes or
  secrets). Head office can read them; that is the purpose of a network, and it is stated
  on the join screen.
- One schema per school means migrations run once per school on a hub upgrade. With tens
  of schools this takes seconds; with thousands it would need a queue.
- Reports cost one query per school. Fine for the alpha; a warehouse can be added behind
  the same endpoints.
- "Head office wins" is decided by which side edited since the last sync, not by clocks,
  so a school PC with a wrong clock cannot win or lose a conflict because of it. The
  stamps are kept for ordering, replays and the log.
- Records that collide on a natural key other than attendance and receipts (two students
  with one admission number) are parked for a person to resolve, not merged by guesswork.
- Shared identities, parent accounts across schools, head office entering receipts or
  attendance through the console, and hub-to-hub links are not in the alpha.

## Decisions left for the owner

1. Whether depositing the backup key should be required to join (the alpha makes it
   optional).
2. Whether a key release should need two different hub users (the alpha needs one user
   plus the passphrase, and always tells the school).
3. How long the hub keeps the archive of a school that left before it must be purged
   (the alpha keeps it until someone purges it).
4. Which fields head office may correct from the console (the alpha allows names and
   contact details only).
