//! Database bootstrap: connect to an external PostgreSQL (`MADRASA_DATABASE_URL`) or start the
//! bundled one (feature `embedded-postgres`). See ADR 0001.

use madrasa_db::PgPool;
use std::path::PathBuf;

pub const ENV_DATABASE_URL: &str = "MADRASA_DATABASE_URL";
pub const ENV_DATA_DIR: &str = "MADRASA_DATA_DIR";

/// Where the database comes from.
#[derive(Debug, Clone)]
pub enum DatabaseSource {
    /// Connect to an existing server. The URL is a secret: never log it.
    External { url: String },
    /// Run the bundled server in `data_dir`.
    Embedded { data_dir: PathBuf },
}

impl DatabaseSource {
    /// `MADRASA_DATABASE_URL` wins; otherwise embedded under `MADRASA_DATA_DIR` or the
    /// platform data directory (`~/.local/share/madrasa`, `%APPDATA%\madrasa`).
    pub fn from_env() -> Self {
        if let Ok(url) = std::env::var(ENV_DATABASE_URL) {
            if !url.trim().is_empty() {
                return DatabaseSource::External { url };
            }
        }
        DatabaseSource::Embedded {
            data_dir: default_data_dir(),
        }
    }
}

pub fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var(ENV_DATA_DIR) {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("madrasa")
}

/// A running database and its pool. Dropping the handle stops an embedded server.
pub struct Database {
    pub pool: PgPool,
    #[cfg(feature = "embedded-postgres")]
    embedded: Option<embedded::Embedded>,
}

impl Database {
    /// Open (and migrate) the database for the given source.
    pub async fn open(source: DatabaseSource) -> anyhow::Result<Database> {
        match source {
            DatabaseSource::External { url } => {
                tracing::info!("connecting to external database");
                let pool = madrasa_db::connect(&url).await?;
                madrasa_db::migrate(&pool).await?;
                Ok(Database {
                    pool,
                    #[cfg(feature = "embedded-postgres")]
                    embedded: None,
                })
            }
            #[cfg(feature = "embedded-postgres")]
            DatabaseSource::Embedded { data_dir } => {
                let embedded = embedded::Embedded::start(&data_dir).await?;
                let pool = madrasa_db::connect(&embedded.url).await?;
                madrasa_db::migrate(&pool).await?;
                Ok(Database {
                    pool,
                    embedded: Some(embedded),
                })
            }
            #[cfg(not(feature = "embedded-postgres"))]
            DatabaseSource::Embedded { .. } => anyhow::bail!(
                "no database configured: set {ENV_DATABASE_URL}, or build with the \
                 `embedded-postgres` feature for an all-in-one install"
            ),
        }
    }

    /// Stop the embedded server (no-op for external databases).
    pub async fn shutdown(self) {
        self.pool.close().await;
        #[cfg(feature = "embedded-postgres")]
        if let Some(embedded) = self.embedded {
            embedded.stop().await;
        }
    }
}

#[cfg(feature = "embedded-postgres")]
pub mod embedded {
    //! Bundled PostgreSQL lifecycle. Binaries are extracted into `<data_dir>/pg/bin`, the cluster
    //! lives in `<data_dir>/pg/data`, and the superuser password is generated once and kept in
    //! `<data_dir>/pg/password` with user-only permissions.

    use postgresql_embedded::{PostgreSQL, Settings, Status};
    use std::path::Path;

    pub const DATABASE_NAME: &str = "madrasa";

    pub struct Embedded {
        server: PostgreSQL,
        pub url: String,
    }

    impl Embedded {
        pub async fn start(data_dir: &Path) -> anyhow::Result<Embedded> {
            let pg_dir = data_dir.join("pg");
            std::fs::create_dir_all(&pg_dir)?;
            let password = load_or_create_password(&pg_dir.join("password"))?;

            let mut settings = Settings::new();
            settings.installation_dir = pg_dir.join("bin");
            settings.data_dir = pg_dir.join("data");
            settings.password_file = pg_dir.join(".pgpass");
            settings.host = "127.0.0.1".into();
            settings.port = 0; // random free port, never exposed beyond loopback
            settings.password = password;
            settings.temporary = false;
            settings.timeout = Some(std::time::Duration::from_secs(60));
            settings
                .configuration
                .insert("listen_addresses".into(), "127.0.0.1".into());

            let mut server = PostgreSQL::new(settings);
            tracing::info!(dir = %pg_dir.display(), "starting bundled database");
            server.setup().await?;
            // A stale postmaster.pid (app killed, machine crashed) or a server left running by a
            // force-killed app would make start fail; stop first, ignoring errors.
            if server.status() == Status::Started {
                tracing::warn!("bundled database appears to be running already; stopping it first");
                let _ = server.stop().await;
            }
            server.start().await?;
            if !server.database_exists(DATABASE_NAME).await? {
                server.create_database(DATABASE_NAME).await?;
            }
            let url = server.settings().url(DATABASE_NAME);
            Ok(Embedded { server, url })
        }

        pub async fn stop(self) {
            if let Err(e) = self.server.stop().await {
                tracing::warn!(error = %e, "failed to stop bundled database");
            }
        }
    }

    fn load_or_create_password(path: &Path) -> anyhow::Result<String> {
        if let Ok(existing) = std::fs::read_to_string(path) {
            let trimmed = existing.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
        let password: String = {
            use std::fmt::Write;
            let mut s = String::new();
            for b in uuid::Uuid::new_v4()
                .as_bytes()
                .iter()
                .chain(uuid::Uuid::new_v4().as_bytes())
            {
                let _ = write!(s, "{b:02x}");
            }
            s
        };
        std::fs::write(path, &password)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(password)
    }
}
