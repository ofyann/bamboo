use crate::error::{BambooError, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
pub struct ConfigFile {
    pub source_registry: Option<String>,
    pub target_registry: Option<String>,
    pub source_creds: Option<String>,
    pub creds: Option<String>,
    pub authfile: Option<String>,
    pub insecure_src: Option<bool>,
    pub insecure_dest: Option<bool>,
    pub retries: Option<usize>,
    pub retry_delay: Option<String>,
    pub timeout: Option<String>,
}

impl ConfigFile {
    /// Load a TOML config file from the given path.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = std::fs::read_to_string(path.as_ref())?;
        toml::from_str(&contents).map_err(|e| {
            BambooError::Auth(format!(
                "配置文件 {} 格式错误: {}",
                path.as_ref().display(),
                e
            ))
        })
    }

    /// Apply config values to environment variables, but only if the env var
    /// is not already set. This preserves the precedence:
    /// CLI args > env vars > config file > defaults.
    pub fn apply_to_env(&self) {
        if let Some(v) = &self.source_registry {
            set_env("BAMBOO_SOURCE_REGISTRY", v);
        }
        if let Some(v) = &self.target_registry {
            set_env("BAMBOO_TARGET_REGISTRY", v);
        }
        if let Some(v) = &self.source_creds {
            set_env("BAMBOO_SOURCE_CREDS", v);
        }
        if let Some(v) = &self.creds {
            set_env("BAMBOO_CREDS", v);
        }
        if let Some(v) = &self.authfile {
            set_env("BAMBOO_AUTHFILE", v);
        }
        if let Some(v) = self.insecure_src {
            set_env("BAMBOO_INSECURE_SRC", if v { "true" } else { "false" });
        }
        if let Some(v) = self.insecure_dest {
            set_env("BAMBOO_INSECURE_DEST", if v { "true" } else { "false" });
        }
        if let Some(v) = self.retries {
            set_env("BAMBOO_RETRIES", &v.to_string());
        }
        if let Some(v) = &self.retry_delay {
            set_env("BAMBOO_RETRY_DELAY", v);
        }
        if let Some(v) = &self.timeout {
            set_env("BAMBOO_TIMEOUT", v);
        }
    }
}

fn set_env(key: &str, value: &str) {
    if std::env::var(key).is_err() {
        std::env::set_var(key, value);
    }
}

/// Pre-parse CLI arguments to find `--config <path>` or `BAMBOO_CONFIG`,
/// then load the config file and apply its values to environment variables.
///
/// This should be called before `Cli::parse()` so that clap can pick up the
/// environment variables as usual.
pub fn preload_from_args() {
    let mut args = std::env::args().skip(1);
    let mut config_path_from_cli: Option<String> = None;

    while let Some(arg) = args.next() {
        if arg == "--config" {
            config_path_from_cli = args.next();
        } else if let Some(value) = arg.strip_prefix("--config=") {
            config_path_from_cli = Some(value.to_string());
        }
    }

    let config_path = config_path_from_cli
        .or_else(|| std::env::var("BAMBOO_CONFIG").ok())
        .filter(|s| !s.is_empty());

    if let Some(path) = config_path {
        if let Err(e) = load_and_apply(&path) {
            eprintln!("加载配置文件失败: {}", e);
            std::process::exit(1);
        }
    }
}

fn load_and_apply(path: &str) -> Result<()> {
    let config = ConfigFile::from_path(path)?;
    config.apply_to_env();
    Ok(())
}

/// Generate a default config file template.
pub fn default_template() -> &'static str {
    r#"# bamboo 配置文件模板
# 用法: bamboo sync --config ./bamboo.toml nginx:1.25
# 优先级: 命令行参数 > 环境变量 > 本配置文件 > 默认值

# 源 Registry 地址（例如 HubProxy 镜像代理）
source_registry = "hubproxy.example.com"

# 目标 Registry 地址（你的私有 Docker Distribution）
target_registry = "registry.example.com:5000"

# 源 Registry 认证，格式 user:pass
# source_creds = "user:pass"

# 目标 Registry 认证，格式 user:pass
# creds = "user:pass"

# Docker 认证文件路径（同时用于源和目标 Registry）
authfile = "~/.docker/config.json"

# 跳过源 Registry 的 TLS 验证
insecure_src = false

# 跳过目标 Registry 的 TLS 验证
insecure_dest = false

# 失败时的重试次数
retries = 3

# 重试间隔
retry_delay = "5s"

# 同步超时时间，0 表示不超时
timeout = "10m"
"#
}
