use crate::auth::resolve_auth;
use crate::cli::SyncArgs;
use crate::error::{BambooError, Result};
use crate::image::ImageRef;
use crate::logging;
use crate::registry::RegistryClient;
use std::str::FromStr;
use tokio::time::sleep;

pub async fn run(args: SyncArgs) -> Result<()> {
    let image_ref = ImageRef::from_str(&args.image)?;
    let normalized = image_ref.normalize();

    let source_path = normalized.hubproxy_path();
    let target_path = normalized.target_path();

    let source_uri = format!("https://{}/{}", args.source_registry, source_path);
    let target_uri = format!("https://{}/{}", args.target_registry, target_path);

    logging::info(&format!(
        "处理镜像: {} -> 目标: {}/{}",
        args.image, args.target_registry, target_path
    ));

    if args.dry_run {
        logging::info(&format!("[空跑模式] 源地址: {}", source_uri));
        logging::info(&format!("[空跑模式] 目标地址: {}", target_uri));
        return Ok(());
    }

    let auth = resolve_auth(args.creds.as_deref(), &args.authfile, &args.target_registry)?;

    let source = RegistryClient::new(
        &args.source_registry,
        &source_path,
        &normalized.tag,
        args.insecure_src,
    )?;
    let target = RegistryClient::new(
        &args.target_registry,
        &target_path,
        &normalized.tag,
        args.insecure_dest,
    )?;

    let src_digest = source.digest(&None).await?;
    let dest_digest = target.digest(&auth).await?;

    if let (Some(src), Some(dest)) = (&src_digest, &dest_digest) {
        if src == dest && !args.force {
            logging::info(&format!(
                "⏭️ 幂等跳过: 目标仓库已存在一致的版本 (Digest: {}...)",
                &src[..15.min(src.len())]
            ));
            return Ok(());
        }
    }

    logging::info("开始网络流式同步...");

    let mut last_err = None;
    for attempt in 1..=args.retries {
        match target.copy_from(&source, &auth).await {
            Ok(()) => {
                logging::info("✅ 同步成功完成！");
                return Ok(());
            }
            Err(e) => {
                last_err = Some(e);
                if attempt < args.retries {
                    logging::warn(&format!(
                        "执行失败，等待 {:?} 秒后重试 ({}/{})...",
                        args.retry_delay, attempt, args.retries
                    ));
                    sleep(args.retry_delay).await;
                }
            }
        }
    }

    Err(BambooError::Sync(last_err.unwrap().to_string()))
}
