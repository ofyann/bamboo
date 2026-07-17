use crate::error::{BambooError, Result};
use crate::image::ImageRef;
use serde::Deserialize;
use std::path::Path;
use std::str::FromStr;

#[derive(Debug, Default, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    pub source_registry: Option<String>,
    pub dest_registry: Option<String>,
    pub source_creds: Option<String>,
    pub dest_creds: Option<String>,
    pub authfile: Option<String>,
    pub insecure_src: Option<bool>,
    pub insecure_dest: Option<bool>,
    pub skip_tls_verify_src: Option<bool>,
    pub skip_tls_verify_dest: Option<bool>,
    pub retries: Option<usize>,
    pub retry_delay: Option<String>,
    pub timeout: Option<String>,
    pub continue_on_error: Option<bool>,
    pub platform: Option<String>,
    pub jobs: Option<usize>,
    pub force: Option<bool>,
    pub images: Option<Vec<ImageEntry>>,
}

#[derive(Debug, Default, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ImageEntry {
    pub image: String,
    pub source_registry: Option<String>,
    pub dest_registry: Option<String>,
    pub source_creds: Option<String>,
    pub dest_creds: Option<String>,
    pub authfile: Option<String>,
    pub insecure_src: Option<bool>,
    pub insecure_dest: Option<bool>,
    pub skip_tls_verify_src: Option<bool>,
    pub skip_tls_verify_dest: Option<bool>,
    pub platform: Option<String>,
    pub force: Option<bool>,
}

impl ConfigFile {
    /// Load a TOML config file from the given path.
    pub async fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = tokio::fs::read_to_string(path.as_ref()).await?;
        let cfg: ConfigFile = toml::from_str(&contents).map_err(|e| {
            BambooError::Config(format!(
                "配置文件 {} 格式错误: {}",
                path.as_ref().display(),
                e
            ))
        })?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// 校验配置字段格式，尽早发现 typo 或无效值。
    pub fn validate(&self) -> Result<()> {
        if let Some(platform) = &self.platform {
            validate_platform(platform)?;
        }
        if let Some(jobs) = self.jobs {
            if jobs == 0 {
                return Err(BambooError::Config("jobs 必须大于 0".to_string()));
            }
        }
        if let Some(retries) = self.retries {
            if retries == 0 {
                return Err(BambooError::Config(
                    "retries 为 0 无意义，至少为 1".to_string(),
                ));
            }
        }
        if let Some(retry_delay) = &self.retry_delay {
            validate_duration(retry_delay, "retry_delay")?;
        }
        if let Some(timeout) = &self.timeout {
            validate_duration(timeout, "timeout")?;
        }
        if let Some(source_registry) = &self.source_registry {
            validate_non_empty(source_registry, "source_registry")?;
        }
        if let Some(dest_registry) = &self.dest_registry {
            validate_non_empty(dest_registry, "dest_registry")?;
        }

        if let Some(images) = &self.images {
            if images.is_empty() {
                return Err(BambooError::Config(
                    "images 列表为空，请至少配置一个镜像".to_string(),
                ));
            }
            for (idx, entry) in images.iter().enumerate() {
                validate_image_entry(entry, idx)?;
            }
        }

        Ok(())
    }
}

impl ConfigFile {
    /// Apply environment variable overrides on top of the merged config.
    ///
    /// This gives the same precedence as `bamboo sync`:
    /// CLI args > env vars > config file > defaults.
    /// For `sync-all`, there are no per-field CLI args, so env vars override config.
    pub fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("BAMBOO_SOURCE_REGISTRY") {
            if !v.is_empty() {
                self.source_registry = Some(v);
            }
        }
        if let Ok(v) = std::env::var("BAMBOO_DEST_REGISTRY") {
            if !v.is_empty() {
                self.dest_registry = Some(v);
            }
        }
        if let Ok(v) = std::env::var("BAMBOO_SOURCE_CREDS") {
            if !v.is_empty() {
                self.source_creds = Some(v);
            }
        }
        if let Ok(v) = std::env::var("BAMBOO_DEST_CREDS") {
            if !v.is_empty() {
                self.dest_creds = Some(v);
            }
        }
        if let Ok(v) = std::env::var("BAMBOO_AUTHFILE") {
            if !v.is_empty() {
                self.authfile = Some(v);
            }
        }
        if let Ok(v) = std::env::var("BAMBOO_INSECURE_SRC") {
            self.insecure_src = Some(v == "true" || v == "1");
        }
        if let Ok(v) = std::env::var("BAMBOO_INSECURE_DEST") {
            self.insecure_dest = Some(v == "true" || v == "1");
        }
        if let Ok(v) = std::env::var("BAMBOO_SKIP_TLS_VERIFY_SRC") {
            self.skip_tls_verify_src = Some(v == "true" || v == "1");
        }
        if let Ok(v) = std::env::var("BAMBOO_SKIP_TLS_VERIFY_DEST") {
            self.skip_tls_verify_dest = Some(v == "true" || v == "1");
        }
        if let Ok(v) = std::env::var("BAMBOO_RETRIES") {
            if let Ok(n) = v.parse::<usize>() {
                self.retries = Some(n);
            }
        }
        if let Ok(v) = std::env::var("BAMBOO_RETRY_DELAY") {
            if !v.is_empty() {
                self.retry_delay = Some(v);
            }
        }
        if let Ok(v) = std::env::var("BAMBOO_TIMEOUT") {
            if !v.is_empty() {
                self.timeout = Some(v);
            }
        }
        if let Ok(v) = std::env::var("BAMBOO_PLATFORM") {
            if !v.is_empty() {
                self.platform = Some(v);
            }
        }
    }

    /// Merge another config into this one.
    ///
    /// - Global scalar fields: later values overwrite earlier ones.
    /// - `images`: append, allowing multiple files to contribute images.
    pub fn merge(&mut self, other: &ConfigFile) {
        if let Some(v) = other.source_registry.clone() {
            self.source_registry = Some(v);
        }
        if let Some(v) = other.dest_registry.clone() {
            self.dest_registry = Some(v);
        }
        if let Some(v) = other.source_creds.clone() {
            self.source_creds = Some(v);
        }
        if let Some(v) = other.dest_creds.clone() {
            self.dest_creds = Some(v);
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
        if let Some(v) = other.skip_tls_verify_src {
            self.skip_tls_verify_src = Some(v);
        }
        if let Some(v) = other.skip_tls_verify_dest {
            self.skip_tls_verify_dest = Some(v);
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
        if let Some(v) = other.platform.clone() {
            self.platform = Some(v);
        }
        if let Some(v) = other.jobs {
            self.jobs = Some(v);
        }
        if let Some(v) = other.force {
            self.force = Some(v);
        }
        if let Some(v) = other.images.clone() {
            self.images.get_or_insert_with(Vec::new).extend(v);
        }
    }

    /// Load multiple config files and merge them into a single config.
    pub async fn load_many(paths: &[String]) -> Result<Self> {
        if paths.is_empty() {
            return Err(BambooError::Config("至少需要一个配置文件".to_string()));
        }

        let mut merged = ConfigFile::default();
        for path in paths {
            let cfg = ConfigFile::from_path(path).await?;
            merged.merge(&cfg);
        }
        merged.validate()?;
        Ok(merged)
    }
}

fn validate_image_entry(entry: &ImageEntry, idx: usize) -> Result<()> {
    if entry.image.is_empty() {
        return Err(BambooError::Config(format!(
            "images[{}].image 不能为空",
            idx
        )));
    }
    ImageRef::from_str(&entry.image)
        .map_err(|e| BambooError::Config(format!("images[{}].image 格式错误: {}", idx, e)))?;
    if let Some(platform) = &entry.platform {
        validate_platform(platform)
            .map_err(|e| BambooError::Config(format!("images[{}].platform {}", idx, e)))?;
    }
    if let Some(source_registry) = &entry.source_registry {
        validate_non_empty(source_registry, &format!("images[{}].source_registry", idx))?;
    }
    if let Some(dest_registry) = &entry.dest_registry {
        validate_non_empty(dest_registry, &format!("images[{}].dest_registry", idx))?;
    }
    Ok(())
}

fn validate_platform(platform: &str) -> Result<()> {
    let parts: Vec<&str> = platform.split('/').collect();
    if parts.len() != 2 && parts.len() != 3 {
        return Err(BambooError::Config(format!(
            "platform 格式错误: {}（应为 os/arch 或 os/arch/variant）",
            platform
        )));
    }
    for part in &parts {
        if part.is_empty() {
            return Err(BambooError::Config(format!(
                "platform 包含空字段: {}",
                platform
            )));
        }
    }
    Ok(())
}

fn validate_duration(value: &str, name: &str) -> Result<()> {
    humantime::parse_duration(value)
        .map_err(|e| BambooError::Config(format!("{} 无法解析为时长 '{}': {}", name, value, e)))?;
    Ok(())
}

fn validate_non_empty(value: &str, name: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(BambooError::Config(format!("{} 不能为空", name)));
    }
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
dest_registry = "registry.example.com:5000"

# 只同步指定平台，格式 os/arch[/variant]；不配置则同步所有架构
# platform = "linux/amd64"

# 源 Registry 认证，格式 user:pass
# source_creds = "user:pass"

# 目标 Registry 认证，格式 user:pass
# dest_creds = "user:pass"

# Docker 认证文件路径（同时用于源和目标 Registry）
authfile = "~/.docker/config.json"

# 源 Registry 使用 HTTP 协议（与 skip_tls_verify 二选一）
insecure_src = false

# 目标 Registry 使用 HTTP 协议（与 skip_tls_verify 二选一）
insecure_dest = false

# 跳过源 Registry 的 TLS 证书校验（仍使用 HTTPS）
skip_tls_verify_src = false

# 跳过目标 Registry 的 TLS 证书校验（仍使用 HTTPS）
skip_tls_verify_dest = false

# 失败时的最大尝试次数（包含首次执行），0 也会尝试一次
retries = 3

# 重试间隔
retry_delay = "5s"

# 同步超时时间，0 表示不超时
timeout = "10m"

# 即使 digest 一致也强制同步（默认 false）
force = false

# 批量同步时的并发数（仅对 sync-all 生效）
jobs = 3

# 批量同步时遇到错误是否继续同步后续镜像（仅对 sync-all 生效）
continue_on_error = false
"#
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
        assert_eq!(
            base.source_registry,
            Some("override.example.com".to_string())
        );
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

    #[tokio::test]
    async fn test_load_many_merges_configs() {
        let base = write_temp_config(
            r#"
source_registry = "hubproxy.example.com"
dest_registry = "registry.example.com:5000"
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
        .await
        .unwrap();

        assert_eq!(
            merged.source_registry,
            Some("hubproxy.example.com".to_string())
        );
        assert_eq!(
            merged.dest_registry,
            Some("registry.example.com:5000".to_string())
        );
        assert_eq!(merged.continue_on_error, Some(true));
        let imgs = merged.images.unwrap();
        assert_eq!(imgs.len(), 2);
        assert_eq!(imgs[1].image, "redis:7");
        assert_eq!(
            imgs[1].source_registry,
            Some("mirror-a.example.com".to_string())
        );
    }

    #[tokio::test]
    async fn test_load_many_empty_paths_errors() {
        let result = ConfigFile::load_many(&[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rejects_unknown_fields() {
        let cfg = write_temp_config(r#"unknown_field = "x""#);
        let result = ConfigFile::from_path(cfg.path()).await;
        assert!(result.is_err(), "应拒绝未知字段");
    }

    #[tokio::test]
    async fn test_rejects_invalid_platform_format() {
        let cfg = write_temp_config(r#"platform = "linux""#);
        let result = ConfigFile::from_path(cfg.path()).await;
        assert!(result.is_err(), "应拒绝 platform 格式错误");
    }

    #[tokio::test]
    async fn test_rejects_invalid_timeout() {
        let cfg = write_temp_config(r#"timeout = "1x""#);
        let result = ConfigFile::from_path(cfg.path()).await;
        assert!(result.is_err(), "应拒绝无法解析的 timeout");
    }

    #[tokio::test]
    async fn test_rejects_zero_jobs() {
        let cfg = write_temp_config(r#"jobs = 0""#);
        let result = ConfigFile::from_path(cfg.path()).await;
        assert!(result.is_err(), "应拒绝 jobs=0");
    }

    #[tokio::test]
    async fn test_rejects_invalid_image_entry() {
        let cfg = write_temp_config(
            r#"
[[images]]
image = "   "
"#,
        );
        let result = ConfigFile::from_path(cfg.path()).await;
        assert!(result.is_err(), "应拒绝空 image");
    }
}
