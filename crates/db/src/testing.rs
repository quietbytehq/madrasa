//! Test helpers: a throwaway PostgreSQL database per test.
//!
//! Set `MADRASA_TEST_DATABASE_URL` to a URL with permission to `CREATE DATABASE`
//! (for example `postgres://postgres:postgres@localhost:5432/postgres`). When unset, tests
//! that call [`TestDb::new`] return `None` and should skip.

use crate::PgPool;
use sqlx::postgres::PgPoolOptions;
use sqlx::Connection;

pub const ENV_VAR: &str = "MADRASA_TEST_DATABASE_URL";

pub struct TestDb {
    pub pool: PgPool,
    pub url: String,
    admin_url: String,
    name: String,
    cleaned: bool,
}

impl TestDb {
    /// Create and migrate a fresh database, or `None` when no test server is configured.
    pub async fn new() -> Option<TestDb> {
        let admin_url = match std::env::var(ENV_VAR) {
            Ok(v) if !v.trim().is_empty() => v,
            _ => {
                eprintln!("skipping: {ENV_VAR} is not set");
                return None;
            }
        };
        let name = format!("madrasa_test_{}", uuid::Uuid::new_v4().simple());
        let mut admin = sqlx::PgConnection::connect(&admin_url)
            .await
            .unwrap_or_else(|e| panic!("connect to {ENV_VAR}: {e}"));
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{name}\"")))
            .execute(&mut admin)
            .await
            .expect("create test database");
        admin.close().await.ok();
        let url = replace_database(&admin_url, &name);
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .expect("connect test database");
        crate::migrate(&pool).await.expect("migrate test database");
        Some(TestDb {
            pool,
            url,
            admin_url,
            name,
            cleaned: false,
        })
    }

    /// Drop the database. Best effort; also attempted on drop.
    pub async fn cleanup(mut self) {
        self.cleaned = true;
        self.pool.close().await;
        drop_database(&self.admin_url, &self.name).await;
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        if self.cleaned {
            return;
        }
        // Best effort from a separate thread with its own runtime; the pool's connections belong
        // to the (possibly panicking) test runtime, so we never close the pool here: dropping
        // the database WITH (FORCE) terminates them server-side.
        let admin_url = self.admin_url.clone();
        let name = self.name.clone();
        let handle = std::thread::spawn(move || {
            if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                rt.block_on(async move {
                    let _ = tokio::time::timeout(
                        std::time::Duration::from_secs(15),
                        drop_database(&admin_url, &name),
                    )
                    .await;
                });
            }
        });
        let _ = handle.join();
    }
}

async fn drop_database(admin_url: &str, name: &str) {
    if let Ok(mut admin) = sqlx::PgConnection::connect(admin_url).await {
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"
        )))
        .execute(&mut admin)
        .await;
        admin.close().await.ok();
    }
}

/// Replace the database path segment of a postgres URL.
pub fn replace_database(url: &str, db: &str) -> String {
    // postgres://user:pass@host:port/dbname?params
    let (head, query) = match url.split_once('?') {
        Some((h, q)) => (h, Some(q)),
        None => (url, None),
    };
    let scheme_end = head.find("://").map(|i| i + 3).unwrap_or(0);
    let after_scheme = &head[scheme_end..];
    let base = match after_scheme.find('/') {
        Some(i) => &head[..scheme_end + i],
        None => head,
    };
    match query {
        Some(q) => format!("{base}/{db}?{q}"),
        None => format!("{base}/{db}"),
    }
}

/// Skip the current test unless a test database is available.
#[macro_export]
macro_rules! test_db {
    () => {
        match $crate::testing::TestDb::new().await {
            Some(db) => db,
            None => return,
        }
    };
}
