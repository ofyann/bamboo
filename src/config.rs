use crate::error::{BambooError, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default, Deserialize, Clone)]
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
    pub continue_on_error: Option<bool>,
    pub images: Option<Vec<ImageEntry>>,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct ImageEntry {
    pub image: String,
    pub source_registry: Option<String>,
    pub target_registry: Option<String>,
    pub source_creds: Option<String>,
    pub creds: Option<String>,
    pub authfile: Option<String>,
    pub insecure_src: Option<bool>,
    pub insecure_dest: Option<bool>,
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

impl ConfigFile {
    /// Merge another config into this one.
    ///
    /// - Global scalar fields: later values overwrite earlier ones.
    /// - `images`: append, allowing multiple files to contribute images.
    pub fn merge(&mut self, other: &ConfigFile) {
        if let Some(v) = other.source_registry.clone() {
            self.source_registry = Some(v);
        }
        if let Some(v) = other.target_registry.clone() {
            self.target_registry = Some(v);
        }
        if let Some(v) = other.source_creds.clone() {
            self.source_creds = Some(v);
        }
        if let Some(v) = other.creds.clone() {
            self.creds = Some(v);
        }
        if let Some(v) = other.authfile.clone() {
            self.authfile = Some(v);
        }
        if let Some(v) = other.insecure_src {
            self.insecure_src = Some(v);
        }
        if let Some(v) = other.insecure_dest {
            self.insecure_dest = Some(v);
        }
        if let Some(v) = other.retries {
            self.retries = Some(v);
        }
        if let Some(v) = other.retry_delay.clone() {
            self.retry_delay = Some(v);
        }
        if let Some(v) = other.timeout.clone() {
            self.timeout = Some(v);
        }
        if let Some(v) = other.continue_on_error {
            self.continue_on_error = Some(v);
        }
        if let Some(v) = other.images.clone() {
            self.images.get_or_insert_with(Vec::new).extend(v);
        }
    }

    /// Load multiple config files and merge them into a single config.
    pub fn load_many(paths: &[String]) -> Result<Self> {
        if paths.is_empty() {
            return Err(BambooError::Auth("至少需要一个配置文件".to_string()));
        }

        let mut merged = ConfigFile::default();
        for path in paths {
            let cfg = ConfigFile::from_path(path)?;
            merged.merge(&cfg);
        }
        Ok(merged)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_config(content: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn test_merge_globals_last_wins() {
        let mut base = ConfigFile {
            source_registry: Some("base.example.com".to_string()),
            retries: Some(3),
            ..Default::default()
        };
        let override_cfg = ConfigFile {
            source_registry: Some("override.example.com".to_string()),
            retries: Some(5),
            ..Default::default()
        };
        base.merge(&override_cfg);
        assert_eq!(base.source_registry, Some("override.example.com".to_string()));
        assert_eq!(base.retries, Some(5));
    }

    #[test]
    fn test_merge_images_appended() {
        let mut base = ConfigFile {
            images: Some(vec![ImageEntry {
                image: "nginx:1.25".to_string(),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let extra = ConfigFile {
            images: Some(vec![ImageEntry {
                image: "redis:7".to_string(),
                ..Default::default()
            }]),
            ..Default::default()
        };
        base.merge(&extra);
        let images = base.images.unwrap();
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].image, "nginx:1.25");
        assert_eq!(images[1].image, "redis:7");
    }

    #[test]
    fn test_load_many_merges_configs() {
        let base = write_temp_config(
            r#"
source_registry = "hubproxy.example.com"
target_registry = "registry.example.com:5000"
"#,
        );
        let images = write_temp_config(
            r#"
continue_on_error = true

[[images]]
image = "nginx:1.25"

[[images]]
image = "redis:7"
source_registry = "mirror-a.example.com"
"#,
        );

        let merged = ConfigFile::load_many(&[
            base.path().to_string_lossy().to_string(),
            images.path().to_string_lossy().to_string(),
        ])
        .unwrap();

        assert_eq!(merged.source_registry, Some("hubproxy.example.com".to_string()));
        assert_eq!(merged.target_registry, Some("registry.example.com:5000".to_string()));
        assert_eq!(merged.continue_on_error, Some(true));
        let imgs = merged.images.unwrap();
        assert_eq!(imgs.len(), 2);
        assert_eq!(imgs[1].image, "redis:7");
        assert_eq!(imgs[1].source_registry, Some("mirror-a.example.com".to_string()));
    }

    #[test]
    fn test_load_many_empty_paths_errors() {
        let result = ConfigFile::load_many(&[]);
        assert!(result.is_err());
    }
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
