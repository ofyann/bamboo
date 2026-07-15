use crate::error::Result;
use crate::sync_engine::SyncEngine;
use crate::sync_spec::SyncSpec;

/// 单镜像同步：委托给 `SyncEngine`。
pub async fn run(spec: SyncSpec) -> Result<()> {
    let engine = SyncEngine::new(1);
    engine.run_one(&spec).await
}
