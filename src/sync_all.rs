use crate::config_resolver::BatchOptions;
use crate::error::Result;
use crate::sync_engine::SyncEngine;
use crate::sync_spec::SyncSpec;

/// 批量同步：委托给 `SyncEngine`。
pub async fn run(specs: Vec<SyncSpec>, options: BatchOptions) -> Result<()> {
    let engine = SyncEngine::new(options.jobs);
    engine.run_many(specs, options.continue_on_error).await
}
