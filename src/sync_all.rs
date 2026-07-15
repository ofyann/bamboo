use crate::config_resolver::BatchOptions;
use crate::error::{BambooError, Result};
use crate::logging;
use crate::sync;
use crate::sync_spec::SyncSpec;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub async fn run(specs: Vec<SyncSpec>, options: BatchOptions) -> Result<()> {
    if specs.is_empty() {
        logging::warn("没有需要同步的镜像");
        return Ok(());
    }

    let total = specs.len();
    let jobs = options.jobs.max(1).min(total);
    logging::info(&format!(
        "开始批量同步，共 {} 个镜像，并发数 {}",
        total, jobs
    ));

    let semaphore = Arc::new(Semaphore::new(jobs));
    let mut join_set = JoinSet::new();

    for spec in specs {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| BambooError::Sync(format!("无法获取并发许可: {}", e)))?;
        let image = spec.image.image_path_with_tag();

        join_set.spawn(async move {
            let _permit = permit;
            (image, sync::run(spec).await)
        });
    }

    let mut errors: Vec<(String, String)> = Vec::new();

    while let Some(result) = join_set.join_next().await {
        let (image, sync_result) =
            result.map_err(|e| BambooError::Sync(format!("任务异常: {}", e)))?;
        match sync_result {
            Ok(()) => {}
            Err(e) => {
                let msg = e.to_string();
                logging::error(&format!("同步 {} 失败: {}", image, msg));
                if options.continue_on_error {
                    errors.push((image, msg));
                } else {
                    return Err(e);
                }
            }
        }
    }

    if errors.is_empty() {
        logging::info("✅ 批量同步全部完成");
        Ok(())
    } else {
        let mut summary = format!(
            "批量同步完成，但 {} / {} 个镜像失败：\n",
            errors.len(),
            total
        );
        for (image, msg) in &errors {
            summary.push_str(&format!("  - {}: {}\n", image, msg));
        }
        Err(BambooError::Sync(summary))
    }
}
