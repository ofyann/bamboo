use crate::error::{BambooError, Result};
use crate::progress::TerminalProgressSink;
use crate::registry::{ManifestCopier, OciRegistry, Registry, RepositoryRef};
use crate::sync_spec::SyncSpec;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};

/// 负责执行单个或批量镜像同步的引擎。
///
/// 它集中了 retry、timeout、并发控制和失败聚合策略，
/// 让 `sync` 与 `sync_all` 子命令变成纯粹的 adapter。
#[derive(Debug, Clone)]
pub struct SyncEngine {
    jobs: usize,
}

impl SyncEngine {
    pub fn new(jobs: usize) -> Self {
        Self { jobs: jobs.max(1) }
    }

    /// 执行单个同步任务。
    pub async fn run_one(&self, spec: &SyncSpec) -> Result<()> {
        let image = spec.image.image_path_with_tag();
        let result: Result<()> = async {
            let normalized = spec.image.normalize();
            let source_path = normalized.hubproxy_path();
            let dest_path = normalized.dest_path();

            let source_scheme = if spec.source.insecure {
                "http"
            } else {
                "https"
            };
            let dest_scheme = if spec.dest.insecure { "http" } else { "https" };

            let source_uri = format!(
                "{}://{}/{}",
                source_scheme, spec.source.registry, source_path
            );
            let dest_uri = format!("{}://{}/{}", dest_scheme, spec.dest.registry, dest_path);

            tracing::info!(
                "[{}] 处理镜像: {} -> 目标: {}/{}",
                image,
                spec.image.image_path_with_tag(),
                spec.dest.registry,
                dest_path
            );
            tracing::debug!(
                "解析结果: registry={}, namespace={}, name={}, tag={}",
                normalized.registry,
                normalized.namespace,
                normalized.name,
                normalized.tag
            );
            tracing::debug!("协议: source={}://, dest={}://", source_scheme, dest_scheme);

            if spec.dry_run {
                tracing::info!("[{}] [空跑模式] 源地址: {}", image, source_uri);
                tracing::info!("[{}] [空跑模式] 目标地址: {}", image, dest_uri);
                return Ok(());
            }

            tracing::debug!(
                "认证: source_auth={}, dest_auth={}",
                if spec.auth.source.is_some() {
                    "有"
                } else {
                    "无"
                },
                if spec.auth.dest.is_some() {
                    "有"
                } else {
                    "无"
                }
            );

            let source_registry =
                OciRegistry::new(spec.source.insecure, spec.source.skip_tls_verify);
            let dest_registry = OciRegistry::new(spec.dest.insecure, spec.dest.skip_tls_verify);

            let source_ref =
                RepositoryRef::with_tag(&spec.source.registry, &source_path, &normalized.tag);
            let dest_ref =
                RepositoryRef::with_tag(&spec.dest.registry, &dest_path, &normalized.tag);

            let src_digest = source_registry
                .digest(&source_ref, &spec.auth.source)
                .await
                .map_err(|e| BambooError::Registry(e.to_string()))?;
            let dest_digest = dest_registry
                .digest(&dest_ref, &spec.auth.dest)
                .await
                .map_err(|e| BambooError::Registry(e.to_string()))?;

            if let (Some(src), Some(dest)) = (&src_digest, &dest_digest) {
                if src == dest && !spec.force {
                    tracing::info!(
                        "[{}] ⏭️ 幂等跳过: 目标仓库已存在一致的版本 (Digest: {}...)",
                        image,
                        &src[..15.min(src.len())]
                    );
                    return Ok(());
                }
            }
            tracing::debug!("digest: source={:?}, dest={:?}", src_digest, dest_digest);

            tracing::info!("[{}] 开始网络流式同步...", image);

            let progress = TerminalProgressSink::new(&image);
            let copier = ManifestCopier::new(
                &source_registry,
                &dest_registry,
                &spec.auth.source,
                &spec.auth.dest,
                &progress,
                spec.platform.clone(),
            );
            let copy_fut = try_with_retry(
                || copier.copy(&source_ref, &dest_ref),
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

            tracing::info!("[{}] ✅ 同步成功完成！", image);
            Ok(())
        }
        .await;

        match result {
            Ok(()) => Ok(()),
            Err(e) => Err(BambooError::Sync(format!("[{}] {}", image, e))),
        }
    }

    /// 执行批量同步任务。
    pub async fn run_many(&self, specs: Vec<SyncSpec>, continue_on_error: bool) -> Result<()> {
        if specs.is_empty() {
            tracing::warn!("没有需要同步的镜像");
            return Ok(());
        }

        let total = specs.len();
        let jobs = self.jobs.min(total);
        tracing::info!("开始批量同步，共 {} 个镜像，并发数 {}", total, jobs);

        let semaphore = Arc::new(Semaphore::new(jobs));
        let mut join_set = JoinSet::new();

        for spec in specs {
            let permit = semaphore
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| BambooError::Sync(format!("无法获取并发许可: {}", e)))?;
            let image = spec.image.image_path_with_tag();
            let engine = self.clone();

            join_set.spawn(async move {
                let _permit = permit;
                (image, engine.run_one(&spec).await)
            });
        }

        let mut success = 0usize;
        let mut errors: Vec<(String, String)> = Vec::new();

        while let Some(result) = join_set.join_next().await {
            let (image, sync_result) =
                result.map_err(|e| BambooError::Sync(format!("任务异常: {}", e)))?;
            match sync_result {
                Ok(()) => {
                    success += 1;
                }
                Err(e) => {
                    let msg = e.to_string();
                    tracing::error!("同步 {} 失败: {}", image, msg);
                    if continue_on_error {
                        errors.push((image, msg));
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        if errors.is_empty() {
            tracing::info!("✅ 批量同步全部完成，共 {} 个镜像", success);
            Ok(())
        } else {
            let mut summary = format!(
                "批量同步完成：成功 {} / {}，失败 {}：\n",
                success,
                total,
                errors.len()
            );
            for (image, msg) in &errors {
                summary.push_str(&format!("  - {}: {}\n", image, msg));
            }
            Err(BambooError::Sync(summary))
        }
    }
}

async fn try_with_retry<F, Fut>(mut f: F, max_attempts: usize, retry_delay: Duration) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(), crate::registry::RegistryError>>,
{
    // retries 语义为最大尝试次数；为 0 时仍执行一次，避免 panic。
    let attempts = max_attempts.max(1);
    let mut last_err = None;

    for attempt in 1..=attempts {
        match f().await {
            Ok(()) => {
                return Ok(());
            }
            Err(e) => {
                last_err = Some(e);
                if attempt < attempts {
                    tracing::warn!(
                        "执行失败: {}，等待 {:?} 秒后重试 ({}/{})...",
                        last_err.as_ref().unwrap(),
                        retry_delay,
                        attempt,
                        attempts
                    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::ImageRef;
    use crate::sync_spec::{AuthPair, RegistryEndpoint, SyncPolicy};
    use std::str::FromStr;

    fn spec_for(image: &str, source_registry: &str, dest_registry: &str) -> SyncSpec {
        SyncSpec {
            image: ImageRef::from_str(image).unwrap(),
            source: RegistryEndpoint {
                registry: source_registry.to_string(),
                insecure: true,
                skip_tls_verify: false,
            },
            dest: RegistryEndpoint {
                registry: dest_registry.to_string(),
                insecure: true,
                skip_tls_verify: false,
            },
            auth: AuthPair::default(),
            authfile: "~/.docker/config.json".to_string(),
            policy: SyncPolicy {
                max_attempts: 1,
                retry_delay: Duration::from_millis(10),
                timeout: Duration::from_millis(100),
            },
            platform: None,
            dry_run: false,
            force: false,
        }
    }

    #[tokio::test]
    async fn run_many_empty_returns_ok() {
        let engine = SyncEngine::new(3);
        let result = engine.run_many(vec![], false).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn run_many_continues_on_error_and_aggregates() {
        let engine = SyncEngine::new(2);
        let specs = vec![
            spec_for("nginx:1.25", "127.0.0.1:1", "127.0.0.1:2"),
            spec_for("redis:7", "127.0.0.1:3", "127.0.0.1:4"),
        ];

        let result = engine.run_many(specs, true).await;
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nginx:1.25"), "{}", err);
        assert!(err.contains("redis:7"), "{}", err);
    }

    #[tokio::test]
    async fn run_many_fail_fast_stops_on_first_error() {
        let engine = SyncEngine::new(2);
        let specs = vec![
            spec_for("nginx:1.25", "127.0.0.1:1", "127.0.0.1:2"),
            spec_for("redis:7", "127.0.0.1:3", "127.0.0.1:4"),
        ];

        let result = engine.run_many(specs, false).await;
        let err = result.unwrap_err().to_string();
        // fail-fast 下只应包含一个镜像
        assert!(
            !(err.contains("nginx:1.25") && err.contains("redis:7")),
            "不应同时包含两个镜像错误: {}",
            err
        );
    }
}
