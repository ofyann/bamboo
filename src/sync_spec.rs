use crate::auth::Auth;
use crate::image::ImageRef;
use std::time::Duration;

/// 一次同步任务的领域描述。
///
/// 它不包含任何 CLI 或 TOML 的特定结构，只包含执行一次 Sync 所需的最小事实集合。
#[derive(Debug, Clone)]
pub struct SyncSpec {
    pub image: ImageRef,
    pub source: RegistryEndpoint,
    pub target: RegistryEndpoint,
    pub auth: AuthPair,
    pub authfile: String,
    pub policy: SyncPolicy,
    pub platform: Option<String>,
    pub dry_run: bool,
    pub force: bool,
}

/// Registry 端点：目标或源的连接信息。
#[derive(Debug, Clone)]
pub struct RegistryEndpoint {
    pub registry: String,
    /// 使用 HTTP 协议。
    pub insecure: bool,
    /// 使用 HTTPS 但跳过 TLS 证书校验。
    pub skip_tls_verify: bool,
}

/// 源/目标 Registry 的认证对。
#[derive(Debug, Clone, Default)]
pub struct AuthPair {
    pub source: Option<Auth>,
    pub target: Option<Auth>,
}

/// 同步执行策略：重试、超时、强制覆盖等。
#[derive(Debug, Clone)]
pub struct SyncPolicy {
    pub max_attempts: usize,
    pub retry_delay: Duration,
    pub timeout: Duration,
}
