//! Madrasa database layer: migrations, models and repositories.
//!
//! All mutations go through repository functions that write the audit log inside the same
//! transaction. Queries use runtime-checked SQL (no compile-time `DATABASE_URL`).

pub mod academics;
pub mod admissions;
pub mod allocation;
pub mod api_keys;
pub mod attendance;
pub mod audit;
pub mod backup;
pub mod boards;
pub mod consents;
pub mod custom_fields;
pub mod directory;
pub mod exams;
pub mod fees;
pub mod guardians;
pub mod hostel;
pub mod imports;
pub mod inventory;
pub mod leave;
pub mod library;
pub mod models;
pub mod nexus_link;
pub mod parent_app;
pub mod report_definitions;
pub mod reports;
pub mod results;
pub mod ro;
pub mod roles;
pub mod rollover;
pub mod schools;
pub mod sessions;
pub mod settings;
pub mod staff;
pub mod students;
pub mod subjects;
pub mod sync;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod timetable;
pub mod transport;
pub mod users;

use sqlx::postgres::PgPoolOptions;
pub use sqlx::PgPool;
use std::time::Duration;

/// Embedded migrations (`crates/db/migrations`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    NotFound(String),
    /// The request cannot be carried out as asked (a rule of the data, not a clash).
    #[error("{0}")]
    Invalid(String),
}

pub type DbResult<T> = Result<T, DbError>;

/// What every Madrasa connection pool starts from: a connection that comes back to the pool
/// is cleared of any transaction before the next borrower gets it.
///
/// Why: when the work that started a transaction is dropped at that very moment (the client
/// went away and the request was abandoned), sqlx 0.9.0 gives the connection back with the
/// transaction open, because it rolls an abandoned `BEGIN` back only once it has counted
/// it, and it counts after the database has answered. Everything the next borrowers then
/// "saved" on that connection sat in a transaction nobody would commit (pilot stage 4, found
/// by the long-run test; `tests/pool.rs`). `BEGIN; ROLLBACK` costs one short exchange, says
/// nothing on a clean connection, and on a connection left inside a transaction ends it
/// with a warning from PostgreSQL in the log.
pub fn pool_options() -> PgPoolOptions {
    PgPoolOptions::new().after_release(|conn, _meta| {
        Box::pin(async move {
            sqlx::raw_sql("BEGIN; ROLLBACK").execute(&mut *conn).await?;
            Ok(true)
        })
    })
}

/// Open a pool. Secrets stay in the URL; never log it.
pub async fn connect(database_url: &str) -> DbResult<PgPool> {
    let pool = pool_options()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(30))
        .connect(database_url)
        .await?;
    Ok(pool)
}

/// Apply pending migrations. Safe to call on every start.
pub async fn migrate(pool: &PgPool) -> DbResult<()> {
    MIGRATOR.run(pool).await?;
    Ok(())
}

/// Who is performing a change, for audit and `updated_by`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Actor {
    pub user_id: Option<uuid::Uuid>,
    pub school_id: Option<uuid::Uuid>,
}

impl Actor {
    pub const SYSTEM: Actor = Actor {
        user_id: None,
        school_id: None,
    };

    pub fn user(user_id: uuid::Uuid, school_id: uuid::Uuid) -> Self {
        Self {
            user_id: Some(user_id),
            school_id: Some(school_id),
        }
    }

    pub fn system_for(school_id: uuid::Uuid) -> Self {
        Self {
            user_id: None,
            school_id: Some(school_id),
        }
    }
}

/// Map a unique-violation into a friendly conflict error.
pub(crate) fn map_unique(err: sqlx::Error, what: &str) -> DbError {
    if let sqlx::Error::Database(db) = &err {
        if db.is_unique_violation() {
            return DbError::Conflict(format!("{what} already exists"));
        }
    }
    DbError::Sqlx(err)
}
