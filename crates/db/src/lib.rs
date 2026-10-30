//! Madrasa database layer: migrations, models and repositories.
//!
//! All mutations go through repository functions that write the audit log inside the same
//! transaction. Queries use runtime-checked SQL (no compile-time `DATABASE_URL`).

pub mod academics;
pub mod audit;
pub mod custom_fields;
pub mod fees;
pub mod guardians;
pub mod models;
pub mod roles;
pub mod schools;
pub mod sessions;
pub mod settings;
pub mod staff;
pub mod students;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
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
}

pub type DbResult<T> = Result<T, DbError>;

/// Open a pool. Secrets stay in the URL; never log it.
pub async fn connect(database_url: &str) -> DbResult<PgPool> {
    let pool = PgPoolOptions::new()
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
