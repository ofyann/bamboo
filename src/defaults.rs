//! 集中管理所有用户可见的默认值，避免在 CLI、模板、解析器和测试中重复定义。

pub const SOURCE_REGISTRY: &str = "hubproxy.example.com";
pub const TARGET_REGISTRY: &str = "registry.example.com:5000";
pub const AUTHFILE: &str = "~/.docker/config.json";
pub const RETRIES: usize = 3;
pub const RETRY_DELAY: &str = "5s";
pub const TIMEOUT: &str = "10m";
pub const JOBS: usize = 3;
