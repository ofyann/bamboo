use std::sync::atomic::{AtomicU8, Ordering};
use std::time::SystemTime;

static LOG_LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

impl LogLevel {
    fn current() -> Self {
        match LOG_LEVEL.load(Ordering::Relaxed) {
            0 => LogLevel::Error,
            1 => LogLevel::Warn,
            3 => LogLevel::Debug,
            _ => LogLevel::Info,
        }
    }
}

pub fn set_level(level: LogLevel) {
    LOG_LEVEL.store(level as u8, Ordering::Relaxed);
}

/// Initialize the log level from CLI-style quiet/verbose flags.
///
/// - `quiet = true`  -> Warn
/// - `verbose = true` -> Debug
/// - both false/none  -> Info
pub fn init_from_flags(quiet: Option<bool>, verbose: Option<bool>) {
    let level = match (quiet, verbose) {
        (_, Some(true)) => LogLevel::Debug,
        (Some(true), _) => LogLevel::Warn,
        _ => LogLevel::Info,
    };
    set_level(level);
}

fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    match time::OffsetDateTime::from_unix_timestamp(secs as i64) {
        Ok(datetime) => {
            const FORMAT: &[time::format_description::FormatItem<'_>] =
                time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
            datetime
                .format(&FORMAT)
                .unwrap_or_else(|_| secs.to_string())
        }
        Err(_) => secs.to_string(),
    }
}

pub fn debug(msg: &str) {
    if LogLevel::current() >= LogLevel::Debug {
        println!("{} [DEBUG] {}", timestamp(), msg);
    }
}

pub fn info(msg: &str) {
    if LogLevel::current() >= LogLevel::Info {
        println!("{} [INFO] {}", timestamp(), msg);
    }
}

pub fn warn(msg: &str) {
    if LogLevel::current() >= LogLevel::Warn {
        eprintln!("{} [WARN] {}", timestamp(), msg);
    }
}

pub fn error(msg: &str) {
    if LogLevel::current() >= LogLevel::Error {
        eprintln!("{} [ERROR] {}", timestamp(), msg);
    }
}
