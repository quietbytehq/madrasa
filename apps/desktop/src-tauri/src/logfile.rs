//! Madrasa's log file: `<data folder>/logs/madrasa.log`.
//!
//! Until v0.5.1 the desktop app wrote its log to the terminal only, which a school never
//! sees, while its error screens said "see the log file". The same lines now also go to a
//! file a tester can attach to a report. It is private to the user like everything else in
//! the data folder, is started afresh when it passes 5 MB (the previous one is kept as
//! `madrasa.log.1`), and holds what the terminal held: no passwords, tokens or records
//! (docs/SECURITY.md lists what is and is not logged).

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use madrasa_api::private;
use tracing_subscriber::fmt::MakeWriter;

/// A log this large is set aside at the next start.
const ROTATE_AT: u64 = 5 * 1024 * 1024;

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("logs").join("madrasa.log")
}

/// Open (and if needed rotate) the log file. `None` when it cannot be written: the app then
/// logs to the terminal only, as before.
pub fn open(data_dir: &Path) -> Option<LogFile> {
    let file = path(data_dir);
    let dir = file.parent()?;
    private::create_private_dir(data_dir).ok()?;
    private::create_private_dir(dir).ok()?;
    if std::fs::metadata(&file).is_ok_and(|m| m.len() >= ROTATE_AT) {
        let _ = std::fs::rename(&file, file.with_extension("log.1"));
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let handle = options.open(&file).ok()?;
    private::restrict_file(&file).ok()?;
    Some(LogFile {
        file: Arc::new(Mutex::new(handle)),
    })
}

/// Writes every log line to the terminal and to the file.
#[derive(Clone)]
pub struct LogFile {
    file: Arc<Mutex<File>>,
}

pub struct Both {
    file: Arc<Mutex<File>>,
}

impl Write for Both {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        if let Ok(mut file) = self.file.lock() {
            let _ = file.write_all(buf);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
        std::io::stderr().flush()
    }
}

impl<'a> MakeWriter<'a> for LogFile {
    type Writer = Both;
    fn make_writer(&'a self) -> Self::Writer {
        Both {
            file: Arc::clone(&self.file),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_file_is_private_appended_to_and_set_aside_when_large() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("madrasa-log-{}-{stamp}", std::process::id()));
        let log = open(&dir).expect("log file");
        log.make_writer().write_all(b"first line\n").unwrap();
        drop(log);
        let log = open(&dir).expect("log file again");
        log.make_writer().write_all(b"second line\n").unwrap();
        drop(log);
        let file = path(&dir);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "first line\nsecond line\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&file), 0o600);
            assert_eq!(mode(file.parent().unwrap()), 0o700);
            assert_eq!(mode(&dir), 0o700);
        }
        assert!(private::exposed(&dir).is_empty());

        // Past the limit: set aside at the next start, and the new file starts empty.
        std::fs::write(&file, vec![b'x'; ROTATE_AT as usize]).unwrap();
        let log = open(&dir).expect("after rotation");
        log.make_writer().write_all(b"fresh\n").unwrap();
        drop(log);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "fresh\n");
        let old = file.with_extension("log.1");
        assert_eq!(std::fs::metadata(&old).unwrap().len(), ROTATE_AT);
        // The one set aside is made private at the next start with the rest of the folder.
        private::harden_data_dir(&dir);
        assert!(private::exposed(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
