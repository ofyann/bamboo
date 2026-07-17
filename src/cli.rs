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
    SyncAll(SyncAllArgs),
    Init(InitArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct SyncArgs {
    /// 要同步的镜像引用，例如 nginx:1.25 或 quay.io/coreos/etcd:v3.5
    pub image: String,

    /// TOML 配置文件路径
    #[arg(long, env = "BAMBOO_CONFIG")]
    pub config: Option<String>,

    /// 源 Registry 地址（例如 HubProxy 镜像代理），默认 hubproxy.example.com
    #[arg(long, env = "BAMBOO_SOURCE_REGISTRY")]
    pub source_registry: Option<String>,

    /// 目标 Registry 地址（你的私有 Docker Distribution），默认 registry.example.com:5000
    #[arg(long, env = "BAMBOO_TARGET_REGISTRY")]
    pub target_registry: Option<String>,

    /// 空跑模式：仅打印解析后的源/目标地址，不执行同步
    #[arg(long, short, default_value_t = false)]
    pub dry_run: bool,

    /// 源 Registry 认证，格式 user:pass
    #[arg(long, env = "BAMBOO_SOURCE_CREDS")]
    pub source_creds: Option<String>,

    /// 目标 Registry 认证，格式 user:pass
    #[arg(long = "target-creds", visible_alias = "creds", env = "BAMBOO_CREDS")]
    pub creds: Option<String>,

    /// Docker 认证文件路径（同时用于源和目标 Registry），默认 ~/.docker/config.json
    #[arg(long, env = "BAMBOO_AUTHFILE")]
    pub authfile: Option<String>,

    /// 源 Registry 使用 HTTP 协议
    #[arg(long, env = "BAMBOO_INSECURE_SRC", num_args = 0..=1, default_missing_value = "true")]
    pub insecure_src: Option<bool>,

    /// 目标 Registry 使用 HTTP 协议
    #[arg(long, env = "BAMBOO_INSECURE_DEST", num_args = 0..=1, default_missing_value = "true")]
    pub insecure_dest: Option<bool>,

    /// 跳过源 Registry 的 TLS 证书校验（仍使用 HTTPS）
    #[arg(long, env = "BAMBOO_SKIP_TLS_VERIFY_SRC", num_args = 0..=1, default_missing_value = "true")]
    pub skip_tls_verify_src: Option<bool>,

    /// 跳过目标 Registry 的 TLS 证书校验（仍使用 HTTPS）
    #[arg(long, env = "BAMBOO_SKIP_TLS_VERIFY_DEST", num_args = 0..=1, default_missing_value = "true")]
    pub skip_tls_verify_dest: Option<bool>,

    /// 失败时的最大尝试次数（包含首次执行），0 也会尝试一次，默认 3
    #[arg(long, env = "BAMBOO_RETRIES")]
    pub retries: Option<usize>,

    /// 重试间隔，默认 5s
    #[arg(long, env = "BAMBOO_RETRY_DELAY", value_parser = parse_duration)]
    pub retry_delay: Option<Duration>,

    /// 同步超时时间，0 表示不超时，默认 10m
    #[arg(long, env = "BAMBOO_TIMEOUT", value_parser = parse_duration)]
    pub timeout: Option<Duration>,

    /// 即使 digest 一致也强制同步
    #[arg(long, default_value_t = false)]
    pub force: bool,

    /// 只同步指定平台，格式 os/arch[/variant]，例如 linux/amd64、linux/arm64/v8
    #[arg(long, short, env = "BAMBOO_PLATFORM")]
    pub platform: Option<String>,

    /// 只输出 WARN 及以上级别日志
    #[arg(long, short, conflicts_with = "verbose", env = "BAMBOO_QUIET", num_args = 0..=1, default_missing_value = "true")]
    pub quiet: Option<bool>,

    /// 输出 DEBUG 级别日志
    #[arg(long, short, conflicts_with = "quiet", env = "BAMBOO_VERBOSE", num_args = 0..=1, default_missing_value = "true")]
    pub verbose: Option<bool>,
}

#[derive(Parser, Debug, Clone)]
pub struct SyncAllArgs {
    /// 要加载的 TOML 配置文件，可多次指定，后加载的配置会覆盖前者同名全局字段，images 会追加
    #[arg(long, env = "BAMBOO_CONFIG", required = true, action = clap::ArgAction::Append)]
    pub config: Vec<String>,

    /// 空跑模式：仅打印将要同步的镜像和解析后的源/目标地址
    #[arg(long, short, default_value_t = false)]
    pub dry_run: bool,

    /// 即使 digest 一致也强制同步所有镜像
    #[arg(long, default_value_t = false)]
    pub force: bool,

    /// 并发同步的镜像数量，默认 3
    #[arg(long, env = "BAMBOO_JOBS")]
    pub jobs: Option<usize>,

    /// 只同步指定平台，格式 os/arch[/variant]，例如 linux/amd64、linux/arm64/v8
    #[arg(long, short, env = "BAMBOO_PLATFORM")]
    pub platform: Option<String>,

    /// 源 Registry 使用 HTTP 协议
    #[arg(long, env = "BAMBOO_INSECURE_SRC", num_args = 0..=1, default_missing_value = "true")]
    pub insecure_src: Option<bool>,

    /// 目标 Registry 使用 HTTP 协议
    #[arg(long, env = "BAMBOO_INSECURE_DEST", num_args = 0..=1, default_missing_value = "true")]
    pub insecure_dest: Option<bool>,

    /// 跳过源 Registry 的 TLS 证书校验（仍使用 HTTPS）
    #[arg(long, env = "BAMBOO_SKIP_TLS_VERIFY_SRC", num_args = 0..=1, default_missing_value = "true")]
    pub skip_tls_verify_src: Option<bool>,

    /// 跳过目标 Registry 的 TLS 证书校验（仍使用 HTTPS）
    #[arg(long, env = "BAMBOO_SKIP_TLS_VERIFY_DEST", num_args = 0..=1, default_missing_value = "true")]
    pub skip_tls_verify_dest: Option<bool>,

    /// 只输出 WARN 及以上级别日志
    #[arg(long, short, conflicts_with = "verbose", env = "BAMBOO_QUIET", num_args = 0..=1, default_missing_value = "true")]
    pub quiet: Option<bool>,

    /// 输出 DEBUG 级别日志
    #[arg(long, short, conflicts_with = "quiet", env = "BAMBOO_VERBOSE", num_args = 0..=1, default_missing_value = "true")]
    pub verbose: Option<bool>,
}

#[derive(Parser, Debug, Clone)]
pub struct InitArgs {
    /// 输出文件路径
    #[arg(short, long, default_value = "bamboo.toml")]
    pub output: String,

    /// 强制覆盖已存在的文件
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    humantime::parse_duration(s).map_err(|e| format!("无法解析时长 '{}': {}", s, e))
}
