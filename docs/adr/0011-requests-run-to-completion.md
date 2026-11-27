# ADR 0011: A request is carried out to its end, and a connection comes back to the pool clean

- Status: accepted; decision 2 withdrawn on 2026-11-27 (see "Update" at the end)
- Date: 2026-11-26

## Context

The web framework drops the work of a request at whatever line it has reached the moment
its client goes away: a closed window, a client PC switched off, a proxy that stopped
waiting. Madrasa's own client proxy does this on purpose (it stops waiting for a slow read),
so on a school network it happens every day.

The long-run test of pilot stage 4 closes connections in the middle of requests all the
time, and found what that can do:

1. **A request dropped while it opens a transaction left its database connection inside
   that transaction.** sqlx 0.9.0 rolls an abandoned `BEGIN` back only once it has counted
   it, and it counts after the database has answered; dropped in between, the connection
   goes back to the pool with the transaction open and sqlx believing there is none
   (`crates/db/tests/pool.rs` shows it on a pool without the guard). Whatever later requests
   then saved on that connection, each on its own, sat in a transaction nobody would commit:
   it was undone by the next rollback on that connection or lost at a restart, after the
   person had been told it was saved. In every version up to 0.5.1.
2. A hand-written `BEGIN` in "Export everything" (stage 3, never released) left the
   connection in a *read-only* transaction when the export was abandoned; from then on
   every request that got that connection failed.
3. In general, a request dropped between two steps that are not in one transaction leaves
   the first done and the second not.

## Decision

1. **Every request runs to its end** (`madrasa_api::to_completion`, and the same layer in
   the hub): the router hands each request to a task of its own and waits for it; when the
   client goes away the waiting stops, the task does not.
2. **Every pool clears a connection when it comes back** (`madrasa_db::pool_options`, used
   by the school server, the hub and the tests): `BEGIN; ROLLBACK`, one short exchange,
   silent on a clean connection; on a connection left inside a transaction it ends that
   transaction and PostgreSQL's warning goes to the log.
3. No hand-written `BEGIN`/`COMMIT`/`ROLLBACK` on pooled connections: transactions are
   sqlx's, which roll back when dropped.

Either of 1 and 2 would have stopped the fault that was found. Both are kept: 1 also covers
what is not a transaction (3 above), 2 also covers work that is dropped for other reasons
than a client leaving (a timeout around database work, a task that is cancelled).

## Consequences

- Work nobody waits for is still done: an abandoned "Export everything" is built to the end
  and thrown away. Accepted; the routes that cost much need a signed-in user with the right
  permission.
- A change whose answer never reached the client is applied all the same. A client PC that
  sends it again is recognised by its `Idempotency-Key` (ADR 0009) and it is not applied
  twice. A person at the server PC who saw no answer sees the saved record when the screen
  reloads.
- One more exchange with the database each time a connection is given back (measured
  together with everything else in the load test: every screen still opens within a second).
- `tests/pool.rs` has a test that fails the day sqlx no longer leaves the transaction open;
  the guard can go then.

## Update, 2026-11-27: the pool's guard is removed (sqlx 0.9.1)

sqlx 0.9.1 (released 2026-11-17) fixes the fault in point 1 of the context: it counts the
transaction before it waits for the database's answer, so a `BEGIN` abandoned half-way is
rolled back like any other dropped transaction.

- Madrasa moved to sqlx 0.9.1 (`Cargo.toml` asks for at least that version). With it, the
  test that was written to notice the fix failed as intended ("sqlx no longer leaves an
  abandoned BEGIN open").
- **Decision 2 is withdrawn.** `madrasa_db::pool_options` and its `BEGIN; ROLLBACK` on every
  returned connection are gone; the school server, the hub and the tests open plain sqlx
  pools. It was a workaround for a fault that no longer exists, and it cost an exchange with
  the database on every request.
- **Decisions 1 and 3 stand.** Every request still runs to its end (`to_completion`): that
  was never only about sqlx, it is what keeps a request from being dropped between two
  steps that are not in one transaction. And no hand-written `BEGIN` on pooled connections.
- **The test stays, turned round** (`crates/db/tests/pool.rs`): 150 starts abandoned
  half-way on a plain pool must leave no connection inside a transaction, and rows saved
  afterwards must be seen by another connection. It was run against both versions on the
  day of the change: with sqlx 0.9.0 it fails (29 of 150 connections came back inside a
  transaction), with 0.9.1 it passes. It also checks itself: that enough of the starts
  really were dropped between `BEGIN` going out and its answer, and, in a second test, that
  its way of looking does see a transaction left open.
- What is given up: the guard also covered any *other* way a connection might come back
  inside a transaction. None is known; `to_completion` removes the commonest cause of
  dropped work, and the regression test covers the one that was found.
