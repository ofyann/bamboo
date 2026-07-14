use clap::{Parser, Subcommand};
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "bamboo")]
#[command(about = "在 OCI Registry 之间同步容器镜像")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Sync(SyncArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct SyncArgs {
    /// 要同步的镜像引用，例如 nginx:1.25 或 quay.io/coreos/etcd:v3.5
    pub image: String,

    /// 源 Registry 地址（例如 HubProxy 镜像代理）
    #[arg(long, env = "BAMBOO_SOURCE_REGISTRY", default_value = "hubproxy.example.com")]
    pub source_registry: String,

    /// 目标 Registry 地址（你的私有 Docker Distribution）
    #[arg(long, env = "BAMBOO_TARGET_REGISTRY", default_value = "registry.example.com:5000")]
    pub target_registry: String,

    /// 空跑模式：仅打印解析后的源/目标地址，不执行同步
    #[arg(long, short, default_value_t = false)]
    pub dry_run: bool,

    /// 源 Registry 认证，格式 user:pass
    #[arg(long, env = "BAMBOO_SOURCE_CREDS")]
    pub source_creds: Option<String>,

    /// 目标 Registry 认证，格式 user:pass
    #[arg(long, env = "BAMBOO_CREDS")]
    pub creds: Option<String>,

    /// Docker 认证文件路径（同时用于源和目标 Registry）
    #[arg(long, env = "BAMBOO_AUTHFILE", default_value = "~/.docker/config.json")]
    pub authfile: String,

    /// 跳过源 Registry 的 TLS 验证
    #[arg(long, env = "BAMBOO_INSECURE_SRC", default_value_t = false)]
    pub insecure_src: bool,

    /// 跳过目标 Registry 的 TLS 验证
    #[arg(long, env = "BAMBOO_INSECURE_DEST", default_value_t = false)]
    pub insecure_dest: bool,

    /// 失败时的重试次数
    #[arg(long, env = "BAMBOO_RETRIES", default_value_t = 3)]
    pub retries: usize,

    /// 重试间隔
    #[arg(long, env = "BAMBOO_RETRY_DELAY", value_parser = parse_duration, default_value = "5s")]
    pub retry_delay: Duration,

    /// 同步超时时间，0 表示不超时
    #[arg(long, env = "BAMBOO_TIMEOUT", value_parser = parse_duration, default_value = "10m")]
    pub timeout: Duration,

    /// 即使 digest 一致也强制同步
    #[arg(long, default_value_t = false)]
    pub force: bool,

    /// 只输出 WARN 及以上级别日志
    #[arg(long, short, conflicts_with = "verbose", default_value_t = false)]
    pub quiet: bool,

    /// 输出 DEBUG 级别日志
    #[arg(long, short, conflicts_with = "quiet", default_value_t = false)]
    pub verbose: bool,
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    humantime::parse_duration(s).map_err(|e| format!("无法解析时长 '{}': {}", s, e))
}
