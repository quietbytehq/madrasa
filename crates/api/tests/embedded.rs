//! The bundled PostgreSQL (the all-in-one install): what it leaves on disk is private to the
//! user, and stopping Madrasa shuts it down cleanly. Both were found wanting when v0.5.0 was
//! tested: `pg/.pgpass` held the database password readable by every account, and nothing
//! showed that closing the server PC's window stopped the database properly.
#![cfg(feature = "embedded-postgres")]

use std::path::{Path, PathBuf};

use madrasa_api::bootstrap::{Database, DatabaseSource};
use madrasa_api::private;

/// PostgreSQL refuses to run as root; the build machines and schools do not, some test
/// sandboxes do.
fn running_as_root() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata("/proc/self")
            .map(|m| m.uid() == 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find(&path, name) {
                return Some(found);
            }
        } else if path.file_stem().and_then(|s| s.to_str()) == Some(name) {
            return Some(path);
        }
    }
    None
}

/// What PostgreSQL itself says about how the cluster was left.
fn cluster_state(data_dir: &Path) -> String {
    let tool = find(&data_dir.join("pg").join("bin"), "pg_controldata").expect("pg_controldata");
    let out = std::process::Command::new(tool)
        .arg("-D")
        .arg(data_dir.join("pg").join("data"))
        .env("LC_ALL", "C")
        .output()
        .expect("run pg_controldata");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.starts_with("Database cluster state"))
        .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
        .unwrap_or_default()
}

fn port(data_dir: &Path) -> u16 {
    std::fs::read_to_string(data_dir.join("pg").join("data").join("postmaster.pid"))
        .expect("postmaster.pid")
        .lines()
        .nth(3)
        .and_then(|l| l.trim().parse().ok())
        .expect("the port in postmaster.pid")
}

#[tokio::test]
async fn the_bundled_database_keeps_its_files_private_and_stops_cleanly() {
    if running_as_root() {
        eprintln!("skipped: PostgreSQL does not run as root");
        return;
    }
    let dir = std::env::temp_dir().join(format!("madrasa-embedded-{}", uuid::Uuid::now_v7()));
    let pid_file = dir.join("pg").join("data").join("postmaster.pid");

    // ---- a fresh install ---------------------------------------------------------------
    let db = Database::open(DatabaseSource::Embedded {
        data_dir: dir.clone(),
    })
    .await
    .expect("the bundled database starts");
    sqlx::query("CREATE TABLE kept (note text)")
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO kept VALUES ('written before the stop')")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(pid_file.exists(), "running");
    let listening = port(&dir);
    assert!(std::net::TcpStream::connect(("127.0.0.1", listening)).is_ok());
    assert!(
        !dir.join("pg").join(".pgpass").exists(),
        "no second copy of the database password"
    );
    assert_eq!(
        private::exposed(&dir),
        Vec::<PathBuf>::new(),
        "nothing in the data folder is open to other accounts"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("pg").join("password")), 0o600);
        assert_eq!(mode(&dir.join("pg").join("data")), 0o700);
    }

    // ---- closing Madrasa: the database is shut down, not cut off ------------------------
    db.shutdown().await;
    assert!(!pid_file.exists(), "a clean stop removes postmaster.pid");
    assert!(
        std::net::TcpStream::connect(("127.0.0.1", listening)).is_err(),
        "nothing listens any more"
    );
    assert_eq!(
        cluster_state(&dir),
        "shut down",
        "PostgreSQL's own word for it"
    );

    // ---- an install made by v0.5.0 or earlier is put right at the next start -----------
    std::fs::write(
        dir.join("pg").join(".pgpass"),
        "the old world-readable copy",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for (p, mode) in [
            (dir.clone(), 0o755),
            (dir.join("pg"), 0o755),
            (dir.join("pg").join(".pgpass"), 0o644),
        ] {
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        assert!(!private::exposed(&dir).is_empty(), "as v0.5.0 left it");
    }
    let db = Database::open(DatabaseSource::Embedded {
        data_dir: dir.clone(),
    })
    .await
    .expect("it starts again");
    assert!(
        !dir.join("pg").join(".pgpass").exists(),
        "the old copy is gone"
    );
    assert_eq!(private::exposed(&dir), Vec::<PathBuf>::new());
    let (note,): (String,) = sqlx::query_as("SELECT note FROM kept")
        .fetch_one(&db.pool)
        .await
        .expect("what was written before the stop is there");
    assert_eq!(note, "written before the stop");
    db.shutdown().await;
    assert_eq!(cluster_state(&dir), "shut down");
    let _ = std::fs::remove_dir_all(&dir);
}
