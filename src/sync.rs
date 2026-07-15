use crate::error::{BambooError, Result};
use crate::logging;
use crate::registry::RegistryClient;
use crate::sync_spec::SyncSpec;
use std::time::Duration;
use tokio::time::{sleep, timeout};

pub async fn run(spec: SyncSpec) -> Result<()> {
    let normalized = spec.image.normalize();

    let source_path = normalized.hubproxy_path();
    let target_path = normalized.target_path();

    let source_scheme = if spec.source.insecure {
        "http"
    } else {
        "https"
    };
    let target_scheme = if spec.target.insecure {
        "http"
    } else {
        "https"
    };

    let source_uri = format!(
        "{}://{}/{}",
        source_scheme, spec.source.registry, source_path
    );
    let target_uri = format!(
        "{}://{}/{}",
        target_scheme, spec.target.registry, target_path
    );

    logging::info(&format!(
        "处理镜像: {} -> 目标: {}/{}",
        spec.image.image_path_with_tag(),
        spec.target.registry,
        target_path
    ));
    logging::debug(&format!(
        "解析结果: registry={}, namespace={}, name={}, tag={}",
        normalized.registry, normalized.namespace, normalized.name, normalized.tag
    ));
    logging::debug(&format!(
        "协议: source={}://, target={}://",
        source_scheme, target_scheme
    ));

    if spec.dry_run {
        logging::info(&format!("[空跑模式] 源地址: {}", source_uri));
        logging::info(&format!("[空跑模式] 目标地址: {}", target_uri));
        return Ok(());
    }

    logging::debug(&format!(
        "认证: source_auth={}, target_auth={}",
        if spec.auth.source.is_some() {
            "有"
        } else {
            "无"
        },
        if spec.auth.target.is_some() {
            "有"
        } else {
            "无"
        }
    ));

    let source = RegistryClient::new(
        &spec.source.registry,
        &source_path,
        &normalized.tag,
        spec.source.insecure,
        spec.source.skip_tls_verify,
    )?;
    let target = RegistryClient::new(
        &spec.target.registry,
        &target_path,
        &normalized.tag,
        spec.target.insecure,
        spec.target.skip_tls_verify,
    )?;

    let src_digest = source.digest(&spec.auth.source).await?;
    let dest_digest = target.digest(&spec.auth.target).await?;

    if let (Some(src), Some(dest)) = (&src_digest, &dest_digest) {
        if src == dest && !spec.force {
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
        &spec.auth.source,
        spec.policy.max_attempts,
        spec.policy.retry_delay,
    );

    if spec.policy.timeout.is_zero() {
        copy_fut.await?;
    } else {
        timeout(spec.policy.timeout, copy_fut).await.map_err(|_| {
            BambooError::Sync(format!("同步超时（超过 {:?}）", spec.policy.timeout))
        })??;
    }

    Ok(())
}

async fn try_copy(
    target: &RegistryClient,
    source: &RegistryClient,
    source_auth: &Option<crate::auth::Auth>,
    retries: usize,
    retry_delay: Duration,
) -> Result<()> {
    // retries 语义为最大尝试次数；为 0 时仍执行一次，避免 panic。
    let max_attempts = retries.max(1);
    let mut last_err = None;
    for attempt in 1..=max_attempts {
        match target.copy_from(source, source_auth).await {
            Ok(()) => {
                logging::info("✅ 同步成功完成！");
                return Ok(());
            }
            Err(e) => {
                last_err = Some(e);
                if attempt < max_attempts {
                    logging::warn(&format!(
                        "执行失败，等待 {:?} 秒后重试 ({}/{})...",
                        retry_delay, attempt, max_attempts
                    ));
                    sleep(retry_delay).await;
                }
            }
        }
    }

    Err(BambooError::Sync(
        last_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "未知错误".to_string()),
    ))
}
