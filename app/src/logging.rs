/*
 * Logging of application activity in a text file.
 *
 * Writes timestamped entries to a file under the XDG data directory
 * (`$XDG_DATA_HOME` or `~/.local/share`), allowing later review of whether
 * playback started correctly and, if not, which error prevented it.
 *
 * Writes are serialized with a global `Mutex` so they can be used from the
 * mpv engine thread (which runs in parallel to the UI) without clobbering
 * entries. It never causes application failures: if the file cannot be
 * opened or written, the error is ignored (the log is diagnostic, not a
 * critical part of the flow).
 */

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use crate::constants::logging::{
    env as log_env, levels, APP_DATA_DIR, DIR_LOCAL, DIR_SHARE, FILE_NAME, LINE_FORMAT,
};

/** Serializes writes to the file between threads. */
static LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/** Severity level of a log entry. */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /** Normal information (e.g. "playback started"). */
    Info,
    /** Something unexpected but not fatal. */
    Warning,
    /** A failure that prevents completing the requested action. */
    Error,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Info => levels::INFO,
            Level::Warning => levels::WARN,
            Level::Error => levels::ERROR,
        }
    }
}

/**
 * Location of the log file on the system (XDG data directory).
 *
 * Prioritizes `$XDG_DATA_HOME`; if it is not defined, uses `~/.local/share`.
 */
pub fn log_file_path() -> PathBuf {
    let base = std::env::var_os(log_env::XDG_DATA_HOME)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            let home = std::env::var_os(log_env::HOME).unwrap_or_default();
            PathBuf::from(home).join(DIR_LOCAL).join(DIR_SHARE)
        });
    base.join(APP_DATA_DIR).join(FILE_NAME)
}

/**
 * Writes a log entry with the given level and message.
 *
 * Returns `false` if the file could not be opened or written (diagnostic),
 * but never fails the application.
 */
pub fn log(level: Level, message: &str) -> bool {
    let lock = LOG_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = match lock.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = LINE_FORMAT
        .replace("{now}", &now.to_string())
        .replace("{}", level.tag())
        .replace("{message}", message);

    let path = log_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut file) => {
            let result = file.write_all(line.as_bytes());
            let _ = file.flush();
            result.is_ok()
        }
        Err(_) => false,
    }
}

/** Logs an informational message (playback state). */
pub fn info(message: impl AsRef<str>) -> bool {
    log(Level::Info, message.as_ref())
}

/** Logs a warning. */
pub fn warn(message: impl AsRef<str>) -> bool {
    log(Level::Warning, message.as_ref())
}

/** Logs an error. */
pub fn error(message: impl AsRef<str>) -> bool {
    log(Level::Error, message.as_ref())
}
