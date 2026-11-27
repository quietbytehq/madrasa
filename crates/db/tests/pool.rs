//! The connection pool gives every borrower a clean connection: the regression check for
//! the sqlx 0.9.0 fault (ADR 0011).

use madrasa_db::testing::TestDb;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

/// Borrow a connection, start a transaction on it and walk away after `steps` steps of the
/// exchange with the database (a timer is too coarse for this: the whole exchange takes a
/// fraction of a millisecond). The connection goes back to the pool as a dropped request's
/// would.
///
/// Says whether the start was really abandoned half-way: begun (polled at least once, so
/// `BEGIN` was on its way) and not yet answered when it was dropped.
async fn abandon_begin_after(pool: &sqlx::PgPool, steps: usize) -> bool {
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
            return false;
        }
        // A moment for the database to answer (the other worker thread reads it).
        std::thread::sleep(Duration::from_micros(15));
        tokio::task::yield_now().await;
    }
    steps > 0
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

/// Out of 150 starts abandoned after 0 to 4 steps: how many times the next borrower got a
/// connection that was still inside the abandoned transaction, and how many of the starts
/// were really abandoned half-way. One connection, so the next borrower always gets the one
/// that was just given back.
async fn times_left_open(options: PgPoolOptions, url: &str) -> (usize, usize) {
    let pool = options.max_connections(1).connect(url).await.unwrap();
    let mut left = 0;
    let mut half_way = 0;
    for _ in 0..30 {
        for steps in 0..5 {
            if abandon_begin_after(&pool, steps).await {
                half_way += 1;
            }
            if inside_a_transaction(&pool).await {
                left += 1;
                // Put right for the next round.
                sqlx::query("ROLLBACK").execute(&pool).await.unwrap();
            }
        }
    }
    pool.close().await;
    (left, half_way)
}

/// Found by the long-run test of pilot stage 4, with sqlx 0.9.0. A request that was dropped
/// at the very moment it started a transaction (the person closed the window, the client PC
/// went away) gave its connection back with that transaction open. Everything done on that
/// connection afterwards (sign-ins, payments) sat in a transaction nobody would commit.
///
/// sqlx 0.9.1 fixed it, and the guard Madrasa's pools carried in the meantime (they cleared
/// every connection as it came back) was removed. This is the regression check that stays:
/// it fails on sqlx 0.9.0 and must keep passing on every later one. If it fails, do not
/// ship: read ADR 0011 and docs/upstream/sqlx-abandoned-begin.md first.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_begin_abandoned_half_way_never_leaves_a_transaction_open() {
    let Some(db) = TestDb::new().await else {
        return;
    };
    // A plain sqlx pool, which is what `madrasa_db::connect` and the hub now open.
    let (left, half_way) = times_left_open(PgPoolOptions::new(), &db.url).await;
    assert_eq!(
        left, 0,
        "a connection came back inside an abandoned transaction"
    );
    // The check only means something if starts were in fact dropped between `BEGIN` going
    // out and its answer coming back, which is the moment sqlx 0.9.0 got wrong.
    assert!(
        half_way >= 10,
        "only {half_way} of 150 starts were abandoned half-way: this test no longer tests anything"
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

/// The check's own eyes: it does see a transaction that was left open, and none where there
/// is none. (Without this, a check that always said "none" would pass on any sqlx.)
#[tokio::test]
async fn the_check_sees_a_transaction_left_open() {
    let Some(db) = TestDb::new().await else {
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&db.url)
        .await
        .unwrap();
    assert!(!inside_a_transaction(&pool).await);
    // Left open by hand, behind sqlx's back, as the fault left it.
    sqlx::raw_sql("BEGIN").execute(&pool).await.unwrap();
    assert!(inside_a_transaction(&pool).await);
    sqlx::raw_sql("ROLLBACK").execute(&pool).await.unwrap();
    assert!(!inside_a_transaction(&pool).await);
    pool.close().await;
    db.cleanup().await;
}
