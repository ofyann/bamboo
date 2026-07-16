use crate::auth::resolve_auth;
use crate::cli::{SyncAllArgs, SyncArgs};
use crate::config::{ConfigFile, ImageEntry};
use crate::defaults;
use crate::error::{BambooError, Result};
use crate::image::ImageRef;
use crate::sync_spec::{AuthPair, RegistryEndpoint, SyncPolicy, SyncSpec};
use std::str::FromStr;
use std::time::Duration;

/// 批量同步的执行选项。
#[derive(Debug, Clone)]
pub struct BatchOptions {
    pub jobs: usize,
    pub continue_on_error: bool,
}

/// 把 `bamboo sync` 的 CLI 参数、环境变量、可选配置文件和默认值合并成一个 `SyncSpec`。
pub async fn resolve_sync(args: &SyncArgs) -> Result<SyncSpec> {
    let mut cfg = ConfigFile::default();
    if let Some(path) = &args.config {
        cfg = ConfigFile::from_path(path).await?;
    }

    let authfile = first_non_empty(
        args.authfile.as_deref(),
        env_opt("BAMBOO_AUTHFILE").as_deref(),
        cfg.authfile.as_deref(),
        defaults::AUTHFILE,
    );

    let source_registry = first_non_empty(
        args.source_registry.as_deref(),
        env_opt("BAMBOO_SOURCE_REGISTRY").as_deref(),
        cfg.source_registry.as_deref(),
        defaults::SOURCE_REGISTRY,
    );
    let target_registry = first_non_empty(
        args.target_registry.as_deref(),
        env_opt("BAMBOO_TARGET_REGISTRY").as_deref(),
        cfg.target_registry.as_deref(),
        defaults::TARGET_REGISTRY,
    );

    let source_creds = args
        .source_creds
        .clone()
        .or_else(|| env_opt("BAMBOO_SOURCE_CREDS"))
        .or_else(|| cfg.source_creds.clone());
    let target_creds = args
        .creds
        .clone()
        .or_else(|| env_opt("BAMBOO_CREDS"))
        .or_else(|| cfg.creds.clone());

    let source_auth = resolve_auth(source_creds.as_deref(), &authfile, &source_registry).await?;
    let target_auth = resolve_auth(target_creds.as_deref(), &authfile, &target_registry).await?;

    let insecure_src = args
        .insecure_src
        .or(env_bool("BAMBOO_INSECURE_SRC"))
        .or(cfg.insecure_src)
        .unwrap_or(false);
    let insecure_dest = args
        .insecure_dest
        .or(env_bool("BAMBOO_INSECURE_DEST"))
        .or(cfg.insecure_dest)
        .unwrap_or(false);

    let skip_tls_verify_src = args
        .skip_tls_verify_src
        .or(env_bool("BAMBOO_SKIP_TLS_VERIFY_SRC"))
        .or(cfg.skip_tls_verify_src)
        .unwrap_or(false);
    let skip_tls_verify_dest = args
        .skip_tls_verify_dest
        .or(env_bool("BAMBOO_SKIP_TLS_VERIFY_DEST"))
        .or(cfg.skip_tls_verify_dest)
        .unwrap_or(false);

    let policy = SyncPolicy {
        max_attempts: args
            .retries
            .or_else(|| env_usize("BAMBOO_RETRIES"))
            .or(cfg.retries)
            .unwrap_or(defaults::RETRIES)
            .max(1),
        retry_delay: args
            .retry_delay
            .or_else(|| {
                env_duration("BAMBOO_RETRY_DELAY")
                    .transpose()
                    .ok()
                    .flatten()
            })
            .or_else(|| {
                cfg.retry_delay
                    .as_deref()
                    .and_then(|s| parse_duration(s).ok())
            })
            .unwrap_or_else(|| parse_duration(defaults::RETRY_DELAY).unwrap()),
        timeout: args
            .timeout
            .or_else(|| env_duration("BAMBOO_TIMEOUT").transpose().ok().flatten())
            .or_else(|| cfg.timeout.as_deref().and_then(|s| parse_duration(s).ok()))
            .unwrap_or_else(|| parse_duration(defaults::TIMEOUT).unwrap()),
    };

    Ok(SyncSpec {
        image: ImageRef::from_str(&args.image)?,
        source: RegistryEndpoint {
            registry: source_registry,
            insecure: insecure_src,
            skip_tls_verify: skip_tls_verify_src,
        },
        target: RegistryEndpoint {
            registry: target_registry,
            insecure: insecure_dest,
            skip_tls_verify: skip_tls_verify_dest,
        },
        auth: AuthPair {
            source: source_auth,
            target: target_auth,
        },
        authfile,
        policy,
        dry_run: args.dry_run,
        force: args.force,
    })
}

/// 把 `bamboo sync-all` 的多个配置文件、环境变量和默认值合并成一组 `SyncSpec`。
pub async fn resolve_sync_all(args: &SyncAllArgs) -> Result<(Vec<SyncSpec>, BatchOptions)> {
    let mut cfg = ConfigFile::load_many(&args.config).await?;
    cfg.apply_env_overrides();

    let images = cfg.images.clone().unwrap_or_default();
    if images.is_empty() {
        return Err(BambooError::Config(
            "配置文件中没有找到 images 列表".to_string(),
        ));
    }

    let authfile = cfg
        .authfile
        .clone()
        .unwrap_or_else(|| defaults::AUTHFILE.to_string());

    let global_source_registry = cfg
        .source_registry
        .clone()
        .unwrap_or_else(|| defaults::SOURCE_REGISTRY.to_string());
    let global_target_registry = cfg
        .target_registry
        .clone()
        .unwrap_or_else(|| defaults::TARGET_REGISTRY.to_string());

    let global_source_auth = resolve_auth(
        cfg.source_creds.as_deref(),
        &authfile,
        &global_source_registry,
    )
    .await?;
    let global_target_auth =
        resolve_auth(cfg.creds.as_deref(), &authfile, &global_target_registry).await?;

    let policy = SyncPolicy {
        max_attempts: cfg.retries.unwrap_or(defaults::RETRIES).max(1),
        retry_delay: cfg
            .retry_delay
            .as_deref()
            .map(parse_duration)
            .transpose()?
            .unwrap_or_else(|| parse_duration(defaults::RETRY_DELAY).unwrap()),
        timeout: cfg
            .timeout
            .as_deref()
            .map(parse_duration)
            .transpose()?
            .unwrap_or_else(|| parse_duration(defaults::TIMEOUT).unwrap()),
    };

    let global_insecure_src = args.insecure_src.or(cfg.insecure_src).unwrap_or(false);
    let global_insecure_dest = args.insecure_dest.or(cfg.insecure_dest).unwrap_or(false);

    let global_skip_tls_verify_src = args
        .skip_tls_verify_src
        .or(cfg.skip_tls_verify_src)
        .unwrap_or(false);
    let global_skip_tls_verify_dest = args
        .skip_tls_verify_dest
        .or(cfg.skip_tls_verify_dest)
        .unwrap_or(false);

    let mut specs = Vec::with_capacity(images.len());
    for entry in images {
        specs.push(
            resolve_sync_all_entry(
                &cfg,
                &entry,
                &authfile,
                &global_source_registry,
                &global_target_registry,
                &global_source_auth,
                &global_target_auth,
                &policy,
                global_insecure_src,
                global_insecure_dest,
                global_skip_tls_verify_src,
                global_skip_tls_verify_dest,
                args.dry_run,
                args.force,
            )
            .await?,
        );
    }

    let options = BatchOptions {
        jobs: args.jobs.unwrap_or(defaults::JOBS).max(1),
        continue_on_error: cfg.continue_on_error.unwrap_or(false),
    };

    Ok((specs, options))
}

#[allow(clippy::too_many_arguments)]
async fn resolve_sync_all_entry(
    _cfg: &ConfigFile,
    entry: &ImageEntry,
    authfile: &str,
    global_source_registry: &str,
    global_target_registry: &str,
    global_source_auth: &Option<crate::auth::Auth>,
    global_target_auth: &Option<crate::auth::Auth>,
    global_policy: &SyncPolicy,
    global_insecure_src: bool,
    global_insecure_dest: bool,
    global_skip_tls_verify_src: bool,
    global_skip_tls_verify_dest: bool,
    dry_run: bool,
    force: bool,
) -> Result<SyncSpec> {
    let source_registry = entry
        .source_registry
        .clone()
        .unwrap_or_else(|| global_source_registry.to_string());
    let target_registry = entry
        .target_registry
        .clone()
        .unwrap_or_else(|| global_target_registry.to_string());

    let source_auth = if let Some(creds) = &entry.source_creds {
        resolve_auth(Some(creds), authfile, &source_registry).await?
    } else {
        global_source_auth.clone()
    };
    let target_auth = if let Some(creds) = &entry.creds {
        resolve_auth(Some(creds), authfile, &target_registry).await?
    } else {
        global_target_auth.clone()
    };

    let insecure_src = entry.insecure_src.unwrap_or(global_insecure_src);
    let insecure_dest = entry.insecure_dest.unwrap_or(global_insecure_dest);
    let skip_tls_verify_src = entry
        .skip_tls_verify_src
        .unwrap_or(global_skip_tls_verify_src);
    let skip_tls_verify_dest = entry
        .skip_tls_verify_dest
        .unwrap_or(global_skip_tls_verify_dest);

    Ok(SyncSpec {
        image: ImageRef::from_str(&entry.image)?,
        source: RegistryEndpoint {
            registry: source_registry,
            insecure: insecure_src,
            skip_tls_verify: skip_tls_verify_src,
        },
        target: RegistryEndpoint {
            registry: target_registry,
            insecure: insecure_dest,
            skip_tls_verify: skip_tls_verify_dest,
        },
        auth: AuthPair {
            source: source_auth,
            target: target_auth,
        },
        authfile: authfile.to_string(),
        policy: global_policy.clone(),
        dry_run,
        force,
    })
}

fn first_non_empty(
    cli: Option<&str>,
    env: Option<&str>,
    cfg: Option<&str>,
    default: &str,
) -> String {
    cli.filter(|s| !s.is_empty())
        .or(env.filter(|s| !s.is_empty()))
        .or(cfg.filter(|s| !s.is_empty()))
        .unwrap_or(default)
        .to_string()
}

fn env_opt(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

fn env_bool(key: &str) -> Option<bool> {
    env_opt(key).map(|v| v == "true" || v == "1")
}

fn env_usize(key: &str) -> Option<usize> {
    env_opt(key).and_then(|v| v.parse().ok())
}

fn env_duration(key: &str) -> Option<std::result::Result<Duration, BambooError>> {
    env_opt(key).map(|v| parse_duration(&v))
}

fn parse_duration(s: &str) -> std::result::Result<Duration, BambooError> {
    humantime::parse_duration(s)
        .map_err(|e| BambooError::Config(format!("无法解析时长 '{}': {}", s, e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{SyncAllArgs, SyncArgs};
    use std::io::Write;

    fn sync_args(image: &str) -> SyncArgs {
        SyncArgs {
            image: image.to_string(),
            config: None,
            source_registry: None,
            target_registry: None,
            dry_run: false,
            source_creds: None,
            creds: None,
            authfile: None,
            insecure_src: None,
            insecure_dest: None,
            skip_tls_verify_src: None,
            skip_tls_verify_dest: None,
            retries: None,
            retry_delay: None,
            timeout: None,
            force: false,
            quiet: None,
            verbose: None,
        }
    }

    fn sync_all_args(configs: Vec<String>) -> SyncAllArgs {
        SyncAllArgs {
            config: configs,
            dry_run: false,
            force: false,
            jobs: None,
            insecure_src: None,
            insecure_dest: None,
            skip_tls_verify_src: None,
            skip_tls_verify_dest: None,
            quiet: None,
            verbose: None,
        }
    }

    fn write_config(content: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn test_resolve_sync_uses_defaults() {
        temp_env::with_var("BAMBOO_TARGET_REGISTRY", None::<&str>, || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let spec = resolve_sync(&sync_args("nginx:1.25")).await.unwrap();
                assert_eq!(spec.source.registry, defaults::SOURCE_REGISTRY);
                assert_eq!(spec.target.registry, defaults::TARGET_REGISTRY);
                assert_eq!(spec.policy.max_attempts, defaults::RETRIES);
                assert_eq!(spec.image.name, "nginx");
                assert_eq!(spec.image.tag, "1.25");
            });
        });
    }

    #[tokio::test]
    async fn test_resolve_sync_cli_overrides_config() {
        let cfg = write_config(r#"source_registry = "cfg-source.example.com""#);
        let mut args = sync_args("redis:7");
        args.config = Some(cfg.path().to_string_lossy().to_string());
        args.source_registry = Some("cli-source.example.com".to_string());

        let spec = resolve_sync(&args).await.unwrap();
        assert_eq!(spec.source.registry, "cli-source.example.com");
    }

    #[test]
    fn test_resolve_sync_config_overrides_defaults() {
        temp_env::with_var("BAMBOO_TARGET_REGISTRY", None::<&str>, || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let cfg = write_config(r#"target_registry = "cfg-target.example.com:5000""#);
                let mut args = sync_args("alpine");
                args.config = Some(cfg.path().to_string_lossy().to_string());

                let spec = resolve_sync(&args).await.unwrap();
                assert_eq!(spec.target.registry, "cfg-target.example.com:5000");
            });
        });
    }

    #[test]
    fn test_resolve_sync_env_overrides_config() {
        temp_env::with_var(
            "BAMBOO_TARGET_REGISTRY",
            Some("env-target.example.com:5000"),
            || {
                tokio::runtime::Runtime::new().unwrap().block_on(async {
                    let cfg = write_config(r#"target_registry = "cfg-target.example.com:5000""#);
                    let mut args = sync_args("alpine");
                    args.config = Some(cfg.path().to_string_lossy().to_string());

                    let spec = resolve_sync(&args).await.unwrap();
                    assert_eq!(spec.target.registry, "env-target.example.com:5000");
                });
            },
        );
    }

    #[tokio::test]
    async fn test_resolve_sync_all_global_and_per_image() {
        let cfg = write_config(
            r#"
source_registry = "global-source.example.com"
target_registry = "global-target.example.com:5000"

[[images]]
image = "nginx:1.25"

[[images]]
image = "redis:7"
source_registry = "per-image-source.example.com"
"#,
        );
        let args = sync_all_args(vec![cfg.path().to_string_lossy().to_string()]);
        let (specs, options) = resolve_sync_all(&args).await.unwrap();

        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].source.registry, "global-source.example.com");
        assert_eq!(specs[1].source.registry, "per-image-source.example.com");
        assert_eq!(options.jobs, defaults::JOBS);
    }

    #[tokio::test]
    async fn test_resolve_sync_all_inherits_global_insecure() {
        let cfg = write_config(
            r#"
source_registry = "global-source.example.com"
target_registry = "global-target.example.com:5000"
insecure_dest = true
skip_tls_verify_src = true

[[images]]
image = "nginx:1.25"

[[images]]
image = "redis:7"
insecure_dest = false
"#,
        );
        let args = sync_all_args(vec![cfg.path().to_string_lossy().to_string()]);
        let (specs, _options) = resolve_sync_all(&args).await.unwrap();

        assert_eq!(specs.len(), 2);
        // 第一个镜像继承全局 insecure_dest = true
        assert!(specs[0].target.insecure);
        assert!(specs[0].source.skip_tls_verify);
        // 第二个镜像被显式覆盖为 false
        assert!(!specs[1].target.insecure);
        assert!(specs[1].source.skip_tls_verify);
    }

    #[tokio::test]
    async fn test_resolve_sync_all_requires_images() {
        let cfg = write_config(r#"source_registry = "global-source.example.com""#);
        let args = sync_all_args(vec![cfg.path().to_string_lossy().to_string()]);
        let result = resolve_sync_all(&args).await;
        assert!(result.is_err());
    }
}
