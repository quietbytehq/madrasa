//! The connection pool gives every borrower a clean connection.

use madrasa_db::testing::TestDb;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

/// Borrow a connection, start a transaction on it and walk away after `steps` steps of the
/// exchange with the database (a timer is too coarse for this: the whole exchange takes a
/// fraction of a millisecond). The connection goes back to the pool as a dropped request's
/// would.
async fn abandon_begin_after(pool: &sqlx::PgPool, steps: usize) {
    use sqlx::Connection;
    use std::future::Future;
    let mut conn = pool.acquire().await.unwrap();
    let fut = conn.begin();
    tokio::pin!(fut);
    for _ in 0..steps {
        let done =
            std::future::poll_fn(|cx| std::task::Poll::Ready(fut.as_mut().poll(cx).is_ready()))
                .await;
        if done {
            return;
        }
        // A moment for the database to answer (the other worker thread reads it).
        std::thread::sleep(Duration::from_micros(15));
        tokio::task::yield_now().await;
    }
}

/// Is the pool's one connection inside a transaction block? (A savepoint is allowed inside
/// one and is an error outside.) Leaves the connection as it found it.
async fn inside_a_transaction(pool: &sqlx::PgPool) -> bool {
    let mut conn = pool.acquire().await.unwrap();
    let inside = sqlx::query("SAVEPOINT probe")
        .execute(&mut *conn)
        .await
        .is_ok();
    if inside {
        sqlx::query("RELEASE SAVEPOINT probe")
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    inside
}

/// How many times, out of the abandoned starts, the next borrower got a connection that was
/// still inside the abandoned transaction. One connection, so the next borrower always gets
/// the one that was just given back.
async fn times_left_open(options: PgPoolOptions, url: &str) -> usize {
    let pool = options.max_connections(1).connect(url).await.unwrap();
    let mut left = 0;
    for _ in 0..30 {
        for steps in 0..5 {
            abandon_begin_after(&pool, steps).await;
            if inside_a_transaction(&pool).await {
                left += 1;
                // Put right for the next round.
                sqlx::query("ROLLBACK").execute(&pool).await.unwrap();
            }
        }
    }
    pool.close().await;
    left
}

/// Found by the long-run test. A request that is dropped at the very moment it starts a
/// transaction (the person closed the window, the client PC went away) used to give its
/// connection back with that transaction open. Everything done on that connection
/// afterwards (sign-ins, payments) sat in a transaction nobody would commit. The app's pool
/// now clears a connection every time it comes back (`madrasa_db::pool_options`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_begin_abandoned_half_way_never_leaves_a_transaction_open() {
    let Some(db) = TestDb::new().await else {
        return;
    };
    assert_eq!(
        times_left_open(madrasa_db::pool_options(), &db.url).await,
        0
    );

    // And what is saved after such a moment is really saved: another connection sees it at
    // once.
    let pool = madrasa_db::connect(&db.url).await.unwrap();
    for n in 0..40 {
        abandon_begin_after(&pool, 1).await;
        sqlx::query(
            "INSERT INTO settings (id, key, value, hlc) VALUES (gen_random_uuid(), $1, '1'::jsonb, 'test')",
        )
        .bind(format!("pool.test.{n}"))
        .execute(&pool)
        .await
        .unwrap();
    }
    let other = madrasa_db::connect(&db.url).await.unwrap();
    let seen: i64 =
        sqlx::query_scalar("SELECT count(*) FROM settings WHERE key LIKE 'pool.test.%'")
            .fetch_one(&other)
            .await
            .unwrap();
    assert_eq!(seen, 40);
    other.close().await;
    pool.close().await;
    db.cleanup().await;
}

/// The fault itself, on a pool without the guard: the reason the guard exists. If this
/// test fails, sqlx has fixed it (0.9.0 has not) and the guard can go.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_the_guard_sqlx_leaves_the_transaction_open() {
    let Some(db) = TestDb::new().await else {
        return;
    };
    let left = times_left_open(PgPoolOptions::new(), &db.url).await;
    assert!(
        left > 0,
        "sqlx no longer leaves an abandoned BEGIN open: madrasa_db::pool_options may drop its guard"
    );
    db.cleanup().await;
}
