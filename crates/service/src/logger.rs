//! Journalisation minimale.
//!
//! - Windows : `<dossier d'installation>\logs\service.log`. Le dossier
//!   d'installation (Program Files) n'est modifiable que par les administrateurs :
//!   un utilisateur ne peut donc pas y placer de lien symbolique pour détourner
//!   les écritures du service.
//! - Linux / macOS : sortie d'erreur, collectée par journald ou launchd.

use std::fs::File;
#[cfg(windows)]
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(windows)]
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{Level, LevelFilter, Log, Metadata, Record};

#[cfg(windows)]
const MAX_LOG_SIZE: u64 = 2 * 1024 * 1024;

struct Logger {
    file: Mutex<Option<File>>,
    console: bool,
}

#[cfg(windows)]
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(windows)]
fn open_log_file() -> Option<File> {
    let dir = exe_dir().join("logs");
    fs::create_dir_all(&dir).ok()?;
    let path = dir.join("service.log");
    if fs::metadata(&path).map(|m| m.len() > MAX_LOG_SIZE).unwrap_or(false) {
        let _ = fs::rename(&path, dir.join("service.old.log"));
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}

/// Sous Linux et macOS, le gestionnaire de services collecte la sortie d'erreur.
#[cfg(not(windows))]
fn open_log_file() -> Option<File> {
    None
}

/// `console` : écrit aussi sur la sortie d'erreur (toujours le cas hors Windows).
pub fn init(console: bool) {
    let logger = Logger {
        file: Mutex::new(open_log_file()),
        console: console || cfg!(not(windows)),
    };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
}

/// Horodatage UTC au format ISO 8601, sans dépendance externe.
fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Algorithme « civil_from_days » de Howard Hinnant.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!("{} [{:<5}] {}\n", timestamp(), record.level(), record.args());
        if self.console {
            eprint!("{line}");
        }
        if let Ok(mut guard) = self.file.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = self.file.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = f.flush();
            }
        }
    }
}
