use crate::cli::{SyncAllArgs, SyncArgs};
use crate::config::{ConfigFile, ImageEntry};
use crate::error::{BambooError, Result};
use crate::logging;
use crate::sync;
use std::time::Duration;

pub async fn run(args: SyncAllArgs) -> Result<()> {
    let config = ConfigFile::load_many(&args.config)?;

    let images = config.images.as_deref().unwrap_or(&[]);
    if images.is_empty() {
        logging::warn("配置文件中没有找到 images 列表，无需同步");
        return Ok(());
    }

    let continue_on_error = config.continue_on_error.unwrap_or(false);
    let mut errors: Vec<(String, String)> = Vec::new();

    logging::info(&format!("开始批量同步，共 {} 个镜像", images.len()));

    for (idx, entry) in images.iter().enumerate() {
        logging::info(&format!(
            "[{}/{}] 处理镜像: {}",
            idx + 1,
            images.len(),
            entry.image
        ));

        let sync_args = build_sync_args(&config, entry, args.dry_run)?;

        match sync::run(sync_args).await {
            Ok(()) => {}
            Err(e) => {
                let msg = e.to_string();
                logging::error(&format!("同步 {} 失败: {}", entry.image, msg));
                if continue_on_error {
                    errors.push((entry.image.clone(), msg));
                } else {
                    return Err(e);
                }
            }
        }
    }

    if errors.is_empty() {
        if args.dry_run {
            logging::info("空跑完成，所有镜像解析正常");
        } else {
            logging::info("✅ 批量同步全部完成");
        }
        Ok(())
    } else {
        logging::error(&format!(
            "批量同步完成，但 {} / {} 个镜像失败：",
            errors.len(),
            images.len()
        ));
        for (image, msg) in &errors {
            logging::error(&format!("  - {}: {}", image, msg));
        }
        Err(BambooError::Sync(format!(
            "{} 个镜像同步失败",
            errors.len()
        )))
    }
}

fn build_sync_args(global: &ConfigFile, entry: &ImageEntry, dry_run: bool) -> Result<SyncArgs> {
    let source_registry = entry
        .source_registry
        .clone()
        .or_else(|| global.source_registry.clone())
        .unwrap_or_else(|| "hubproxy.example.com".to_string());

    let target_registry = entry
        .target_registry
        .clone()
        .or_else(|| global.target_registry.clone())
        .unwrap_or_else(|| "registry.example.com:5000".to_string());

    let authfile = entry
        .authfile
        .clone()
        .or_else(|| global.authfile.clone())
        .unwrap_or_else(|| "~/.docker/config.json".to_string());

    let retries = global.retries.unwrap_or(3);

    let retry_delay = parse_duration(
        "retry_delay",
        &global.retry_delay.clone().unwrap_or_else(|| "5s".to_string()),
    )?;

    let timeout = parse_duration(
        "timeout",
        &global.timeout.clone().unwrap_or_else(|| "10m".to_string()),
    )?;

    Ok(SyncArgs {
        image: entry.image.clone(),
        config: None,
        source_registry,
        target_registry,
        dry_run,
        source_creds: entry.source_creds.clone().or_else(|| global.source_creds.clone()),
        creds: entry.creds.clone().or_else(|| global.creds.clone()),
        authfile,
        insecure_src: entry.insecure_src.or(global.insecure_src).unwrap_or(false),
        insecure_dest: entry.insecure_dest.or(global.insecure_dest).unwrap_or(false),
        retries,
        retry_delay,
        timeout,
        force: false,
        quiet: false,
        verbose: false,
    })
}

fn parse_duration(field: &str, s: &str) -> Result<Duration> {
    humantime::parse_duration(s).map_err(|e| {
        BambooError::Sync(format!("无法解析配置项 {} 的时长 '{}': {}", field, s, e))
    })
}
