//! Minimal event log written next to the configuration file.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

static LOG: Mutex<Option<File>> = Mutex::new(None);

const MAX_SIZE: u64 = 512 * 1024;

/// Opens the log file (rotating it when it becomes large).
pub fn init(path: &Path) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if fs::metadata(path).is_ok_and(|m| m.len() > MAX_SIZE) {
        let _ = fs::rename(path, path.with_extension("log.old"));
    }
    if let Ok(file) = OpenOptions::new().create(true).append(true).open(path)
        && let Ok(mut guard) = LOG.lock()
    {
        *guard = Some(file);
    }
}

pub fn write(level: &str, message: &str) {
    let now = declic_win::system::local_now();
    let line = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} [{level}] {message}\n",
        now.year, now.month, now.day, now.hour, now.minute, now.second
    );
    if cfg!(debug_assertions) {
        eprint!("{line}");
    }
    if let Ok(mut guard) = LOG.lock()
        && let Some(file) = guard.as_mut()
    {
        let _ = file.write_all(line.as_bytes());
    }
}

macro_rules! info {
    ($($arg:tt)*) => { $crate::log::write("INFO", &format!($($arg)*)) };
}

macro_rules! warning {
    ($($arg:tt)*) => { $crate::log::write("WARN", &format!($($arg)*)) };
}

pub(crate) use {info, warning};
