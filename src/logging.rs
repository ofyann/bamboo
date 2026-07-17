use std::time::SystemTime;
use tracing::Level;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    pub fn to_tracing_level(self) -> Level {
        match self {
            LogLevel::Error => Level::ERROR,
            LogLevel::Warn => Level::WARN,
            LogLevel::Info => Level::INFO,
            LogLevel::Debug => Level::DEBUG,
        }
    }
}

/// 根据 quiet/verbose 标志决定日志级别。
pub fn level_from_flags(quiet: Option<bool>, verbose: Option<bool>) -> LogLevel {
    match (quiet, verbose) {
        (_, Some(true)) => LogLevel::Debug,
        (Some(true), _) => LogLevel::Warn,
        _ => LogLevel::Info,
    }
}

/// 初始化 tracing subscriber，保持与原先类似的时间戳+级别输出格式。
pub fn init_subscriber(level: LogLevel) {
    // 默认过滤掉 oci_distribution 的非致命 WARN（如 NoSignatureComponent），
    // 减少无决策价值的刷屏；可用 RUST_LOG 覆盖。
    let filter_str = format!("{},oci_distribution=error", level.to_tracing_level());
    let filter = EnvFilter::try_new(filter_str)
        .unwrap_or_else(|_| EnvFilter::default().add_directive(level.to_tracing_level().into()));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_timer(BambooTimer)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_level(true)
        .with_writer(std::io::stdout);

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();
}

struct BambooTimer;

impl FormatTime for BambooTimer {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();
        match time::OffsetDateTime::from_unix_timestamp(secs as i64) {
            Ok(datetime) => {
                const FORMAT: &[time::format_description::FormatItem<'_>] = time::macros::format_description!(
                    "[year]-[month]-[day] [hour]:[minute]:[second]"
                );
                let s = datetime
                    .format(&FORMAT)
                    .unwrap_or_else(|_| secs.to_string());
                write!(w, "{}", s)
            }
            Err(_) => write!(w, "{}", secs),
        }
    }
}
