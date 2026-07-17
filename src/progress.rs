use std::collections::HashMap;
use std::sync::Mutex;

const TIMESTAMP_FORMAT: &[time::format_description::FormatItem<'_>] =
    time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

/// Blob 传输方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Pull,
    Push,
}

impl Direction {
    fn as_str(&self) -> &'static str {
        match self {
            Direction::Pull => "拉取",
            Direction::Push => "推送",
        }
    }
}

/// 正在传输的 blob 上下文。
#[derive(Debug, Clone)]
pub struct BlobContext {
    pub digest: String,
    pub size: Option<u64>,
}

/// 进度报告 sink。
///
/// Registry 在拉取/推送 blob 时通过此 trait 报告进度，
/// 实现者负责决定如何展示或丢弃这些事件。
pub trait ProgressSink: Send + Sync {
    /// 设置当前正在处理的 platform，用于多架构镜像 index。
    fn set_platform(&self, platform: Option<String>);

    /// 开始同步一个 manifest（单架构镜像或多架构镜像的某个平台）。
    ///
    /// `total_blobs` 包含 config 与所有 layers；`total_bytes` 为预估总字节数，
    /// 可能为 0（大小全部未知时）。
    fn init_manifest(&self, total_blobs: usize, total_bytes: u64);

    /// 某个 blob 开始传输。
    fn on_start(&self, ctx: &BlobContext, direction: Direction);

    /// 某个 blob 传输进度更新。
    fn on_progress(&self, ctx: &BlobContext, direction: Direction, current: u64);

    /// 某个 blob 传输完成。
    fn on_complete(&self, ctx: &BlobContext, direction: Direction, total: u64);

    /// 某个 blob 已存在，跳过传输。
    fn on_skip(&self, ctx: &BlobContext, direction: Direction);
}

/// 不输出任何内容的 progress sink。
pub struct NoopProgressSink;

impl ProgressSink for NoopProgressSink {
    fn set_platform(&self, _platform: Option<String>) {}
    fn init_manifest(&self, _total_blobs: usize, _total_bytes: u64) {}
    fn on_start(&self, _ctx: &BlobContext, _direction: Direction) {}
    fn on_progress(&self, _ctx: &BlobContext, _direction: Direction, _current: u64) {}
    fn on_complete(&self, _ctx: &BlobContext, _direction: Direction, _total: u64) {}
    fn on_skip(&self, _ctx: &BlobContext, _direction: Direction) {}
}

/// 全局输出锁，保证多并发同步任务在终端的进度行不会交错。
static PROGRESS_LOCK: Mutex<()> = Mutex::new(());

const THROTTLE_PERCENT_STEP: u64 = 10;
const THROTTLE_BYTES_UNKNOWN_SIZE: u64 = 1024 * 1024;

/// 单个 manifest（单架构镜像或某个平台）的聚合进度。
#[derive(Debug, Default)]
struct ManifestProgress {
    total_blobs: usize,
    total_bytes: u64,
    done_blobs: usize,
    done_bytes: u64,
    in_flight: HashMap<String, u64>,
    last_reported_pct: u64,
}

/// 在终端以人类可读的格式输出进度。
pub struct TerminalProgressSink {
    label: String,
    platform: Mutex<Option<String>>,
    manifests: Mutex<HashMap<Option<String>, ManifestProgress>>,
    last_printed: Mutex<HashMap<(Direction, String), u64>>,
    lock: &'static Mutex<()>,
}

impl TerminalProgressSink {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            platform: Mutex::new(None),
            manifests: Mutex::new(HashMap::new()),
            last_printed: Mutex::new(HashMap::new()),
            lock: &PROGRESS_LOCK,
        }
    }

    fn current_platform(&self) -> Option<String> {
        self.platform.lock().unwrap().clone()
    }

    fn prefix(&self, platform: Option<&String>) -> String {
        match platform {
            Some(p) => format!("[{} ({})]", self.label, p),
            None => format!("[{}]", self.label),
        }
    }

    fn print(&self, line: String) {
        let _guard = self.lock.lock().unwrap();
        println!("{} {}", now_timestamp(), line);
    }

    fn with_manifest_progress<F>(&self, f: F)
    where
        F: FnOnce(&mut ManifestProgress),
    {
        let key = self.current_platform();
        let mut manifests = self.manifests.lock().unwrap();
        let progress = manifests.entry(key).or_default();
        f(progress);
    }

    fn report_aggregate_if_needed(&self, platform: Option<String>) {
        let mut manifests = self.manifests.lock().unwrap();
        let progress = manifests.entry(platform.clone()).or_default();

        if progress.total_blobs == 0 {
            return;
        }

        let in_flight_bytes: u64 = progress.in_flight.values().sum();
        let current_bytes = progress.done_bytes.saturating_add(in_flight_bytes);

        let pct = if progress.total_bytes > 0 {
            (current_bytes * 100)
                .checked_div(progress.total_bytes)
                .unwrap_or(0)
        } else {
            let active_blobs = progress.done_blobs.saturating_add(progress.in_flight.len()) as u64;
            let total_blobs = progress.total_blobs as u64;
            (active_blobs * 100)
                .checked_div(total_blobs.max(1))
                .unwrap_or(0)
        }
        .min(100);

        if pct
            >= progress
                .last_reported_pct
                .saturating_add(THROTTLE_PERCENT_STEP)
            || (pct == 100 && progress.last_reported_pct < 100)
        {
            let prefix = self.prefix(platform.as_ref());
            let active_blobs = progress
                .done_blobs
                .saturating_add(progress.in_flight.len())
                .min(progress.total_blobs);
            let total_bytes_desc = if progress.total_bytes > 0 {
                human_bytes(progress.total_bytes)
            } else {
                "?".to_string()
            };
            self.print(format!(
                "{} 总进度 {}/{} blobs，{} / {} ({}%)",
                prefix,
                active_blobs,
                progress.total_blobs,
                human_bytes(current_bytes),
                total_bytes_desc,
                pct
            ));
            progress.last_reported_pct = pct;
        }
    }

    fn should_print(&self, ctx: &BlobContext, direction: Direction, current: u64) -> bool {
        let mut last = self.last_printed.lock().unwrap();
        let key = (direction, ctx.digest.clone());
        let last_value = last.get(&key).copied().unwrap_or(0);

        let should = match ctx.size {
            Some(total) if total > 0 => {
                let last_pct = last_value * 100 / total;
                let current_pct = current * 100 / total;
                current_pct >= last_pct + THROTTLE_PERCENT_STEP
            }
            _ => {
                // 大小未知时，每 1 MiB 打印一次。
                current >= last_value + THROTTLE_BYTES_UNKNOWN_SIZE
            }
        };

        if should {
            last.insert(key, current);
        }

        should
    }
}

impl ProgressSink for TerminalProgressSink {
    fn set_platform(&self, platform: Option<String>) {
        let mut p = self.platform.lock().unwrap();
        *p = platform;
    }

    fn init_manifest(&self, total_blobs: usize, total_bytes: u64) {
        let key = self.current_platform();
        let mut manifests = self.manifests.lock().unwrap();
        let progress = ManifestProgress {
            total_blobs,
            total_bytes,
            ..Default::default()
        };
        manifests.insert(key, progress);
    }

    fn on_start(&self, ctx: &BlobContext, direction: Direction) {
        self.with_manifest_progress(|p| {
            p.in_flight.insert(ctx.digest.clone(), 0);
        });

        self.print(format!(
            "{} {} blob {} ({})",
            self.prefix(self.current_platform().as_ref()),
            direction.as_str(),
            short_digest(&ctx.digest),
            match ctx.size {
                Some(size) => format!("大小 {}", human_bytes(size)),
                None => "大小未知".to_string(),
            }
        ));
    }

    fn on_progress(&self, ctx: &BlobContext, direction: Direction, current: u64) {
        if !self.should_print(ctx, direction, current) {
            return;
        }

        self.with_manifest_progress(|p| {
            p.in_flight.insert(ctx.digest.clone(), current);
        });

        match ctx.size {
            Some(total) => {
                self.print(format!(
                    "{} {} blob {} {} / {} ({})",
                    self.prefix(self.current_platform().as_ref()),
                    direction.as_str(),
                    short_digest(&ctx.digest),
                    human_bytes(current),
                    human_bytes(total),
                    percent(current, total)
                ));
            }
            None => {
                self.print(format!(
                    "{} {} blob {} {} / ?",
                    self.prefix(self.current_platform().as_ref()),
                    direction.as_str(),
                    short_digest(&ctx.digest),
                    human_bytes(current)
                ));
            }
        }

        self.report_aggregate_if_needed(self.current_platform());
    }

    fn on_complete(&self, ctx: &BlobContext, direction: Direction, total: u64) {
        self.with_manifest_progress(|p| {
            p.in_flight.remove(&ctx.digest);
            p.done_blobs += 1;
            p.done_bytes += total;
        });

        self.print(format!(
            "{} {} blob {} 完成 ({})",
            self.prefix(self.current_platform().as_ref()),
            direction.as_str(),
            short_digest(&ctx.digest),
            human_bytes(total)
        ));

        self.report_aggregate_if_needed(self.current_platform());
    }

    fn on_skip(&self, ctx: &BlobContext, direction: Direction) {
        self.with_manifest_progress(|p| {
            p.in_flight.remove(&ctx.digest);
            p.done_blobs += 1;
            if let Some(size) = ctx.size {
                p.done_bytes += size;
            }
        });

        self.print(format!(
            "{} {} blob {} 已存在，跳过",
            self.prefix(self.current_platform().as_ref()),
            direction.as_str(),
            short_digest(&ctx.digest)
        ));

        self.report_aggregate_if_needed(self.current_platform());
    }
}

fn now_timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(TIMESTAMP_FORMAT)
        .unwrap_or_else(|_| "????-??-?? ??:??:??".to_string())
}

fn short_digest(digest: &str) -> &str {
    // 保留 algorithm 前缀，并截断 hash 部分以提高可读性。
    if let Some(pos) = digest.find(':') {
        let hash = &digest[pos + 1..];
        if hash.len() > 16 {
            return &digest[..pos + 1 + 16];
        }
        return digest;
    }
    if digest.len() > 16 {
        &digest[..16]
    } else {
        digest
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut value = bytes as f64;
    let mut unit_index = 0;
    while value >= 1024.0 && unit_index + 1 < UNITS.len() {
        value /= 1024.0;
        unit_index += 1;
    }
    format!("{:.1} {}", value, UNITS[unit_index])
}

fn percent(current: u64, total: u64) -> String {
    if total == 0 {
        return "0%".to_string();
    }
    format!("{}%", current * 100 / total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_formats_correctly() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512.0 B");
        assert_eq!(human_bytes(1024), "1.0 KiB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MiB");
        assert_eq!(human_bytes(10 * 1024 * 1024), "10.0 MiB");
        assert_eq!(human_bytes(1024 * 1024 * 1024), "1.0 GiB");
    }

    #[test]
    fn short_digest_truncates_hash() {
        assert_eq!(
            short_digest("sha256:5d2c68e0b3e2f9b3e4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7"),
            "sha256:5d2c68e0b3e2f9b3"
        );
        assert_eq!(short_digest("sha256:abcd"), "sha256:abcd");
    }

    #[test]
    fn percent_calculates_correctly() {
        assert_eq!(percent(0, 100), "0%");
        assert_eq!(percent(34, 100), "34%");
        assert_eq!(percent(100, 100), "100%");
        assert_eq!(percent(0, 0), "0%");
    }

    #[test]
    fn terminal_progress_sink_reports_aggregate_milestones() {
        let sink = TerminalProgressSink::new("redis:8");
        sink.init_manifest(2, 100);

        let ctx1 = BlobContext {
            digest: "sha256:aaa".to_string(),
            size: Some(40),
        };
        sink.on_start(&ctx1, Direction::Pull);
        sink.on_progress(&ctx1, Direction::Pull, 10);
        sink.on_progress(&ctx1, Direction::Pull, 20);
        sink.on_complete(&ctx1, Direction::Pull, 40);

        let ctx2 = BlobContext {
            digest: "sha256:bbb".to_string(),
            size: Some(60),
        };
        sink.on_start(&ctx2, Direction::Pull);
        sink.on_progress(&ctx2, Direction::Pull, 60);
        sink.on_complete(&ctx2, Direction::Pull, 60);

        // 至少应该打印 100% 的汇总行。
        let manifests = sink.manifests.lock().unwrap();
        let progress = manifests.get(&None).unwrap();
        assert_eq!(progress.last_reported_pct, 100);
        assert_eq!(progress.done_blobs, 2);
    }
}
