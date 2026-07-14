use crate::auth::{resolve_auth, Auth};
use crate::cli::SyncArgs;
use crate::error::{BambooError, Result};
use crate::image::ImageRef;
use crate::logging;
use crate::registry::RegistryClient;
use std::str::FromStr;
use std::time::Duration;
use tokio::time::{sleep, timeout};

pub async fn run(args: SyncArgs) -> Result<()> {
    let image_ref = ImageRef::from_str(&args.image)?;
    let normalized = image_ref.normalize();

    let source_path = normalized.hubproxy_path();
    let target_path = normalized.target_path();

    let source_scheme = if args.insecure_src { "http" } else { "https" };
    let target_scheme = if args.insecure_dest { "http" } else { "https" };

    let source_uri = format!("{}://{}/{}", source_scheme, args.source_registry, source_path);
    let target_uri = format!("{}://{}/{}", target_scheme, args.target_registry, target_path);

    logging::info(&format!(
        "处理镜像: {} -> 目标: {}/{}",
        args.image, args.target_registry, target_path
    ));
    logging::debug(&format!(
        "解析结果: registry={}, namespace={}, name={}, tag={}",
        normalized.registry, normalized.namespace, normalized.name, normalized.tag
    ));
    logging::debug(&format!(
        "协议: source={}://, target={}://",
        source_scheme, target_scheme
    ));

    if args.dry_run {
        logging::info(&format!("[空跑模式] 源地址: {}", source_uri));
        logging::info(&format!("[空跑模式] 目标地址: {}", target_uri));
        return Ok(());
    }

    let source_auth = resolve_auth(
        args.source_creds.as_deref(),
        &args.authfile,
        &args.source_registry,
    )?;
    let target_auth = resolve_auth(args.creds.as_deref(), &args.authfile, &args.target_registry)?;
    logging::debug(&format!(
        "认证: source_auth={}, target_auth={}",
        if source_auth.is_some() { "有" } else { "无" },
        if target_auth.is_some() { "有" } else { "无" }
    ));

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

    let src_digest = source.digest(&source_auth).await?;
    let dest_digest = target.digest(&target_auth).await?;

    if let (Some(src), Some(dest)) = (&src_digest, &dest_digest) {
        if src == dest && !args.force {
            logging::info(&format!(
                "⏭️ 幂等跳过: 目标仓库已存在一致的版本 (Digest: {}...)",
                &src[..15.min(src.len())]
            ));
            return Ok(());
        }
    }
    logging::debug(&format!(
        "digest: source={:?}, target={:?}",
        src_digest, dest_digest
    ));

    logging::info("开始网络流式同步...");

    let copy_fut = try_copy(
        &target,
        &source,
        &source_auth,
        args.retries,
        args.retry_delay,
    );

    if args.timeout.is_zero() {
        copy_fut.await?;
    } else {
        timeout(args.timeout, copy_fut)
            .await
            .map_err(|_| BambooError::Sync(format!("同步超时（超过 {:?}）", args.timeout)))??;
    }

    Ok(())
}

async fn try_copy(
    target: &RegistryClient,
    source: &RegistryClient,
    source_auth: &Option<Auth>,
    retries: usize,
    retry_delay: Duration,
) -> Result<()> {
    let mut last_err = None;
    for attempt in 1..=retries {
        match target.copy_from(source, source_auth).await {
            Ok(()) => {
                logging::info("✅ 同步成功完成！");
                return Ok(());
            }
            Err(e) => {
                last_err = Some(e);
                if attempt < retries {
                    logging::warn(&format!(
                        "执行失败，等待 {:?} 秒后重试 ({}/{})...",
                        retry_delay, attempt, retries
                    ));
                    sleep(retry_delay).await;
                }
            }
        }
    }

    Err(BambooError::Sync(last_err.unwrap().to_string()))
}
