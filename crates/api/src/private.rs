//! Files and folders only the owning user may read.
//!
//! Madrasa's data folder holds the bundled database's password, the school server's TLS key,
//! the key that seals the offline queue, and the (encrypted) backups. None of it is for other
//! accounts on the same PC. On Linux and macOS files are made `0600` and folders `0700`, from
//! the moment they are created. On Windows the folder's access list is replaced by one that
//! names only the user, SYSTEM and the Administrators group, and no longer inherits from the
//! folder above.
//!
//! [`harden_data_dir`] runs at every start, so an install made by an earlier version (whose
//! `pg/.pgpass` was readable by everyone) is put right too.

use std::path::{Path, PathBuf};

/// Write `bytes` to `path` so that only the owner can read it, at no moment readable by
/// anyone else: the file is created with the right mode rather than narrowed afterwards.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all().ok();
    drop(file);
    // An existing file keeps its old mode when reopened: narrow it as well.
    restrict_file(path)
}

/// Create `path` (and its parents) as a folder only the owner can open.
pub fn create_private_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    restrict_dir(path)
}

/// Only the owner may read or write this file.
pub fn restrict_file(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(windows)]
    windows::owner_only(path, false)?;
    #[cfg(not(any(unix, windows)))]
    let _ = path;
    Ok(())
}

/// Only the owner may list, enter or change this folder.
pub fn restrict_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(windows)]
    windows::owner_only(path, true)?;
    #[cfg(not(any(unix, windows)))]
    let _ = path;
    Ok(())
}

/// The parts of the data folder Madrasa does not write itself: PostgreSQL's programs (as
/// unpacked, some of them must stay executable) and its cluster (PostgreSQL insists on its
/// own `0700` and manages the files inside).
fn left_alone(data_dir: &Path, path: &Path) -> bool {
    let pg = data_dir.join("pg");
    path.starts_with(pg.join("bin")) || path.starts_with(pg.join("data"))
}

/// Make the whole data folder private: the folder itself, and every file and folder Madrasa
/// has written in it. Returns how many entries were narrowed. Errors on single entries are
/// logged and do not stop the rest (a start must not fail over a file that is already gone).
pub fn harden_data_dir(data_dir: &Path) -> usize {
    if !data_dir.is_dir() {
        return 0;
    }
    let mut changed = 0;
    let mut note = |r: std::io::Result<()>, path: &Path| match r {
        Ok(()) => changed += 1,
        Err(e) => tracing::warn!(error = %e, path = %path.display(), "could not make private"),
    };
    // First the door: with the folder itself closed, nothing inside can be reached.
    note(restrict_dir(data_dir), data_dir);
    // The file an earlier version left readable by everyone. It was only ever needed to set
    // up the database the first time, and `pg/password` holds the same thing.
    let legacy = data_dir.join("pg").join(".pgpass");
    if legacy.exists() {
        match std::fs::remove_file(&legacy) {
            Ok(()) => tracing::info!("removed the old pg/.pgpass (replaced by pg/password)"),
            Err(_) => note(restrict_file(&legacy), &legacy),
        }
    }
    let mut stack = vec![data_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if left_alone(data_dir, &path) {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                // On Windows the folder's list is inherited from the data folder; on Unix
                // every folder is narrowed by itself.
                #[cfg(unix)]
                note(restrict_dir(&path), &path);
                stack.push(path);
            } else {
                #[cfg(unix)]
                note(restrict_file(&path), &path);
            }
        }
    }
    changed
}

/// Everything in the data folder that someone other than the owner could read, enter or
/// change. Empty after [`harden_data_dir`]. On Windows, where the access list of the data
/// folder decides, the folder itself is checked.
pub fn exposed(data_dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let open = |p: &Path| {
            std::fs::symlink_metadata(p)
                .map(|m| !m.file_type().is_symlink() && m.permissions().mode() & 0o077 != 0)
                .unwrap_or(false)
        };
        if open(data_dir) {
            found.push(data_dir.to_path_buf());
        }
        let mut stack = vec![data_dir.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if left_alone(data_dir, &path) {
                    continue;
                }
                if open(&path) {
                    found.push(path.clone());
                }
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    stack.push(path);
                }
            }
        }
    }
    #[cfg(windows)]
    if windows::others_listed(data_dir) {
        found.push(data_dir.to_path_buf());
    }
    #[cfg(not(any(unix, windows)))]
    let _ = data_dir;
    found
}

#[cfg(windows)]
mod windows {
    //! Access lists, through Windows' own `icacls` (no window is shown).
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    /// SYSTEM and the Administrators group, by their well-known identifiers (their names
    /// differ between Windows languages).
    const SYSTEM: &str = "*S-1-5-18";
    const ADMINISTRATORS: &str = "*S-1-5-32-544";

    fn me() -> Option<String> {
        let user = std::env::var("USERNAME").ok().filter(|u| !u.is_empty())?;
        Some(match std::env::var("USERDOMAIN") {
            Ok(domain) if !domain.is_empty() => format!("{domain}\\{user}"),
            _ => user,
        })
    }

    /// Replace the access list: no inheritance from above; full control for the user, SYSTEM
    /// and Administrators, and nobody else. For a folder the entries apply to everything in it.
    pub fn owner_only(path: &Path, dir: bool) -> std::io::Result<()> {
        let Some(me) = me() else {
            return Err(std::io::Error::other("the Windows user name is not known"));
        };
        let rights = if dir { "(OI)(CI)F" } else { "F" };
        let out = Command::new("icacls")
            .arg(path)
            .arg("/inheritance:r")
            .arg("/grant:r")
            .arg(format!("{me}:{rights}"))
            .arg("/grant:r")
            .arg(format!("{SYSTEM}:{rights}"))
            .arg("/grant:r")
            .arg(format!("{ADMINISTRATORS}:{rights}"))
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;
        if out.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "icacls: {}",
                String::from_utf8_lossy(&out.stdout).trim()
            )))
        }
    }

    /// Does the access list name anyone besides the user, SYSTEM and Administrators?
    /// (`icacls` with `/save`-style SDDL would be exact; the listing is enough to see the
    /// broad groups that matter: Everyone, Users, Authenticated Users.)
    pub fn others_listed(path: &Path) -> bool {
        let Ok(out) = Command::new("icacls")
            .arg(path)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        else {
            return true;
        };
        let listing = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
        [
            "everyone",
            "builtin\\users",
            "authenticated users",
            "s-1-1-0",
            "s-1-5-11",
            "s-1-5-32-545",
        ]
        .iter()
        .any(|group| listing.contains(group))
            || listing.contains("(i)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("madrasa-private-{name}-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A data folder as an earlier version left it: everything readable by everyone,
    /// including the database password in `pg/.pgpass`.
    fn old_install(dir: &Path) {
        for sub in [
            "pg",
            "pg/bin/16/bin",
            "pg/data",
            "tls",
            "outbox/abc/queue",
            "outbox/abc/cache",
            "backups",
        ] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        for (file, content) in [
            ("mode.json", "{}"),
            ("pg/.pgpass", "the-database-password"),
            ("pg/password", "the-database-password"),
            ("pg/bin/16/bin/postgres", "#!program"),
            ("tls/key.der", "key"),
            ("tls/cert.der", "cert"),
            ("outbox/abc/outbox.key", "0123456789abcdef0123456789abcdef"),
            ("outbox/abc/book.bin", "sealed"),
            ("outbox/abc/queue/000000000001-x.bin", "sealed"),
            ("outbox/abc/cache/aa.bin", "sealed"),
            ("backups/madrasa-demo01-20261116-104142.mbk", "sealed"),
        ] {
            std::fs::write(dir.join(file), content).unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut stack = vec![dir.to_path_buf()];
            while let Some(d) = stack.pop() {
                std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o755)).unwrap();
                for e in std::fs::read_dir(&d).unwrap().flatten() {
                    if e.file_type().unwrap().is_dir() {
                        stack.push(e.path());
                    } else {
                        std::fs::set_permissions(e.path(), std::fs::Permissions::from_mode(0o644))
                            .unwrap();
                    }
                }
            }
        }
    }

    /// The v0.5.0 finding: `pg/.pgpass` was `0644`. After a start nothing Madrasa wrote in
    /// its data folder can be read by another account, and the old file is gone.
    #[test]
    fn the_whole_data_folder_is_private_after_a_start() {
        let dir = temp("harden");
        old_install(&dir);
        #[cfg(unix)]
        assert!(
            exposed(&dir).iter().any(|p| p.ends_with("pg/.pgpass")),
            "the old install really was open"
        );
        let changed = harden_data_dir(&dir);
        assert!(changed > 0);
        assert_eq!(exposed(&dir), Vec::<PathBuf>::new(), "nothing is left open");
        assert!(
            !dir.join("pg/.pgpass").exists(),
            "the old password file is removed"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("pg/password")).unwrap(),
            "the-database-password",
            "the owner still reads its own files"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode =
                |p: &str| std::fs::metadata(dir.join(p)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(""), 0o700);
            for f in [
                "pg/password",
                "tls/key.der",
                "outbox/abc/outbox.key",
                "mode.json",
                "backups/madrasa-demo01-20261116-104142.mbk",
            ] {
                assert_eq!(mode(f), 0o600, "{f}");
            }
            for d in ["pg", "tls", "outbox", "outbox/abc/queue", "backups"] {
                assert_eq!(mode(d), 0o700, "{d}");
            }
            // PostgreSQL's own programs keep their modes (they must stay executable).
            assert_eq!(mode("pg/bin/16/bin/postgres"), 0o644);
        }
        // Running it again changes nothing that matters and does not fail.
        harden_data_dir(&dir);
        assert!(exposed(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_private_file_is_private_from_the_start_and_when_rewritten() {
        let dir = temp("write");
        let file = dir.join("secret.key");
        write_private(&file, b"one").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"one");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o600
            );
            // A file someone made readable is narrowed again when Madrasa rewrites it.
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
            write_private(&file, b"two").unwrap();
            assert_eq!(
                std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        #[cfg(windows)]
        {
            write_private(&file, b"two").unwrap();
            assert!(
                !super::windows::others_listed(&file),
                "only the user, SYSTEM and Administrators"
            );
        }
        assert_eq!(std::fs::read(&file).unwrap(), b"two");
        let sub = dir.join("a").join("b");
        create_private_dir(&sub).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&sub).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
