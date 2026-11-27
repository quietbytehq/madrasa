# sqlx 0.9.0 (PostgreSQL): a `begin()` cancelled half-way leaves the connection inside a transaction

**Status (2026-11-27): already fixed upstream. Do not post this.** sqlx **0.9.1** (released
2026-11-17) contains the fix: "fix(postgres): roll back a transaction cancelled during
BEGIN", pull request 4394 in sqlx's repository. The program below prints the wrong answers
with sqlx 0.9.0 and the right ones with 0.9.1; both were run here (see "Checked"). Madrasa
is still on 0.9.0, so the fault is still in what we ship, behind our workaround. What is
left is ours to do, not sqlx's: see "What Madrasa should do now".

Two more things the owner should know before anything is ever posted to that project:

- Its changelog for 0.9.1 announces an `AI_POLICY.md`: issues and pull requests "authored
  primarily or entirely by agents like Claude Code" are not accepted. This report was
  written by one. If something has to be reported there in future, a person should write it
  in their own words, with the reproduction below as their evidence.
- Nothing was posted anywhere. This file is the whole of it.

The rest is kept as the record of the fault, in the shape of a bug report, because it
explains why `madrasa_db::pool_options` and ADR 0011 exist.

## Summary

With sqlx 0.9.0 and PostgreSQL, if the future returned by `Connection::begin()` (or
`Pool::begin()`) is dropped after `BEGIN` has been sent and before its reply has been read,
the server is left inside a transaction while sqlx believes there is none. Nothing rolls it
back. Every later statement on that connection, each of which the caller thinks commits on
its own, runs inside that transaction and is never committed. With a pool, the connection
goes back to the pool in that state and the statements belong to other, unrelated callers.

Dropping a future at an `.await` is ordinary in async Rust: `tokio::select!`, a timeout, and
every HTTP handler whose client disconnects (axum and hyper drop the handler's future).

## Versions

- sqlx 0.9.0, features `runtime-tokio`, `postgres` (no TLS needed to see it)
- tokio 1.x, multi-thread runtime
- PostgreSQL 16.15
- rustc 1.97.0, Linux x86-64

## Steps to reproduce

`Cargo.toml`:

```toml
[package]
name = "sqlx-abandoned-begin"
version = "0.0.0"
edition = "2021"

[dependencies]
sqlx = { version = "=0.9.0", default-features = false, features = ["runtime-tokio", "postgres"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

`src/main.rs`:

```rust
use sqlx::{Connection, PgConnection};
use std::future::Future;
use std::task::Poll;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
    let mut a = PgConnection::connect(&url).await?;
    let mut b = PgConnection::connect(&url).await?;
    sqlx::query("DROP TABLE IF EXISTS abandoned_begin").execute(&mut a).await?;
    sqlx::query("CREATE TABLE abandoned_begin (n int)").execute(&mut a).await?;

    // Start a transaction and drop the future after its first poll: `BEGIN` has been
    // sent, its reply has not been read. This is what a cancelled request does
    // (tokio::select!, a timeout, an HTTP handler whose client went away).
    {
        let begin = a.begin();
        tokio::pin!(begin);
        let finished =
            std::future::poll_fn(|cx| Poll::Ready(begin.as_mut().poll(cx).is_ready())).await;
        assert!(!finished, "BEGIN was answered within one poll; run it again");
    }

    // No transaction as far as sqlx and the caller know: this is a plain statement.
    sqlx::query("INSERT INTO abandoned_begin VALUES (1)").execute(&mut a).await?;

    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM abandoned_begin")
        .fetch_one(&mut b)
        .await?;
    let in_tx = sqlx::query("SAVEPOINT probe").execute(&mut a).await.is_ok();
    println!("rows another connection sees after the INSERT returned Ok: {seen} (expected 1)");
    println!("connection is inside a transaction block: {in_tx} (expected false)");

    a.close().await?;
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM abandoned_begin")
        .fetch_one(&mut b)
        .await?;
    println!("rows left after the connection was closed: {seen} (expected 1)");
    Ok(())
}
```

Run it against any empty PostgreSQL database:

```bash
DATABASE_URL=postgres://postgres@127.0.0.1:5432/scratch cargo run
```

## Expected

The abandoned `begin()` leaves no trace, as a dropped `Transaction` leaves none: the
`INSERT` commits on its own.

```
rows another connection sees after the INSERT returned Ok: 1 (expected 1)
connection is inside a transaction block: false (expected false)
rows left after the connection was closed: 1 (expected 1)
```

## Actual (sqlx 0.9.0)

```
rows another connection sees after the INSERT returned Ok: 0 (expected 1)
connection is inside a transaction block: true (expected false)
rows left after the connection was closed: 0 (expected 1)
```

The `INSERT` returned `Ok`, was visible to nobody else, and was lost when the connection
closed. No error and no warning anywhere.

## Why

`sqlx-postgres-0.9.0/src/transaction.rs`, `PgTransactionManager::begin`:

```rust
let rollback = Rollback::new(conn);
rollback.conn.queue_simple_query(statement.as_str())?;
rollback.conn.wait_until_ready().await?;          // <- dropped here
if !rollback.conn.in_transaction() {
    return Err(Error::BeginFailed);
}
rollback.conn.inner.transaction_depth += 1;       // <- never reached
rollback.defuse();
```

`Rollback`'s `Drop` calls `start_rollback`, which queues a `ROLLBACK` only
`if conn.inner.transaction_depth > 0`. The depth is raised after the await, so when the
future is dropped at the await the guard does nothing: `BEGIN` is on its way to the server,
no `ROLLBACK` follows it, and the depth says zero. 0.9.1 raises the depth before the await
and lowers it again if the `BEGIN` did not take.

## What it did to Madrasa

Found by the long-run test of pilot stage 4 (PROGRESS.md, 2026-11-26), which closes
connections in the middle of requests. A request dropped at the moment it opened a
transaction gave its pooled connection back inside that transaction. Sign-ins, payments and
attendance saved by later requests on that connection were answered "saved" and sat in a
transaction nobody would commit: undone by the next rollback on that connection, or lost at
a restart. In every Madrasa version up to 0.5.1.

## Our workaround (in Madrasa since pilot stage 4, ADR 0011)

1. **Every pool clears a connection when it comes back**: `madrasa_db::pool_options()`
   (`crates/db/src/lib.rs`), used by the school server, the hub and the tests:

   ```rust
   PgPoolOptions::new().after_release(|conn, _meta| {
       Box::pin(async move {
           sqlx::raw_sql("BEGIN; ROLLBACK").execute(&mut *conn).await?;
           Ok(true)
       })
   })
   ```

   `BEGIN; ROLLBACK` rather than a bare `ROLLBACK`: it is silent on a clean connection, and
   on one left inside a transaction it ends that transaction with a warning from PostgreSQL
   in the log. One short exchange each time a connection is given back.
2. **Every request runs to its end**: `madrasa_api::to_completion` (and the same layer in
   the hub) hands each request to a task of its own, so a client going away no longer drops
   the work at an `.await`.

Either would have stopped the fault. Both are kept: 2 also covers steps that are not in one
transaction; 1 also covers work dropped for other reasons (a timeout, a cancelled task).

## The test that detects the fix

`crates/db/tests/pool.rs`:

- `a_begin_abandoned_half_way_never_leaves_a_transaction_open`: with our pool, 150 abandoned
  starts leave no transaction open, and 40 rows saved after such moments are all seen by
  another connection.
- `without_the_guard_sqlx_leaves_the_transaction_open`: the fault itself, on a pool without
  the guard. **It passes while sqlx has the fault and fails once sqlx is fixed**, with the
  message "sqlx no longer leaves an abandoned BEGIN open: madrasa_db::pool_options may drop
  its guard".

## Checked

| | sqlx 0.9.0 | sqlx 0.9.1 |
| --- | --- | --- |
| The program above, three runs each, PostgreSQL 16.15 | wrong every time: 0, true, 0 | right every time: 1, false, 1 |
| `PgTransactionManager::begin` | depth raised after the await | depth raised before the await |

Not checked: MySQL and SQLite. Read only: the MySQL driver of 0.9.0 and of 0.9.1 also raises
its depth after `conn.execute(statement).await`, which looks like the same order; whether a
cancelled `begin()` leaves a MySQL connection inside a transaction was not run. Madrasa does
not use MySQL.

## What Madrasa should do now

Nothing has been changed for this in the pull request that added this file; these are for
the owner to decide.

1. **Move to sqlx 0.9.1.** It is a patch release with many fixes; its changelog names one
   behaviour change (`hostaddr` in a connection string is no longer an alias for `host`),
   which Madrasa's connection strings do not use. The whole test suite, the upgrade test and
   the long-run test should be run on it before a release.
2. When that is done, `without_the_guard_sqlx_leaves_the_transaction_open` will fail, as it
   is meant to. Then choose: delete that test and keep the guard as a cheap second line of
   defence (one exchange per returned connection; suggested), or remove the guard as
   ADR 0011 allows. `to_completion` stays either way.
3. Until then, nothing is at risk: the workaround does not depend on sqlx's behaviour.
