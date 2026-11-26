# ADR 0010: Every backup is checked by restoring it, and is made without holding the database in memory

- Status: accepted
- Date: 2026-11-26

## Context

A backup that cannot be restored is found out on the day it is needed. Until now Madrasa
wrote the file and trusted it: a full disk, a failing USB stick or a mistake in the dump
would only show at a restore. A failed nightly backup showed as one grey line on the Backup
page, which the people at a school rarely open.

The load test of pilot stage 4 (5,000 students, three years of fees and attendance, a
1.2 GB database) showed a second problem: a backup needed 1.8 GB of memory, because the
whole dump was built in memory before it was compressed.

A backup file is sealed for the school's public key. Opening it needs the backup password
or the recovery key, and neither is kept on the computer. So an unattended check cannot
decrypt the file it has just written.

## Decision

1. **Check every backup right after it is written**, in two steps:
   - read the file back from the disk and compare its SHA-256 with what was written;
   - restore the records, as they were sealed (the compressed dump, before encryption),
     into a temporary database next to the school's, count the rows of **every** table
     against the counts taken in the dump's own snapshot, and drop the temporary database
     whatever happened (and any left by a check that was cut short).
2. **The result is stored with the run** (`backup_runs.verify_status`: `verified`,
   `failed`, `skipped`, with a sentence for people). A scheduled backup counts as done only
   after its check; "Back up now" answers when the file is written and the check follows.
   `skipped` is for a database account that may not create databases (a school on its own
   PostgreSQL): the file check still ran.
3. **A failed backup or a failed check is an alert**, not a line: a red notice on the
   dashboard and the Backup page for everyone who can see the dashboard, with the reason,
   the date of the last good backup and what to do. It stays until a backup succeeds and
   passes.
4. **The dump is streamed.** Rows go from `COPY … TO STDOUT` into the gzip compressor a few
   megabytes at a time; a large table is written as several consecutive records of the dump
   container. A restore reads the container as a stream and feeds consecutive records of a
   table into one `COPY … FROM STDIN`. The `.mbk` file format does not change (header,
   AES-256-GCM over the gzip, SHA-256 trailer).

## Consequences

- What is proven after each backup: the file on the disk is what was written, and its
  contents are complete and load into this version's tables. What is **not** proven: that
  the backup password opens the file. Only a restore proves that; the manual keeps the
  advice to try a restore on another PC once a term.
- The check needs free disk space of about the size of the database, for as long as it
  runs (86 s for the 1.2 GB school of the load test), and is skipped honestly where the
  database account cannot create a database.
- Memory for a backup no longer grows with the school: 0.33 GB at the peak for the load
  test's school, from 1.8 GB.
- A dump with several records per table cannot be read by an older build. That changes
  nothing in practice: a backup is already tied to the migration set of the version that
  made it (spec §5.8), and this change comes with new migrations.
- The counts in a backup's heading are now taken in the same snapshot as its records.
  Before, they were counted just before the dump and could differ by a row saved in between.
