use crate::auth::Auth;
use crate::progress::{BlobContext, Direction, ProgressSink};
use crate::registry::{Manifest, Registry, RegistryError, RepositoryRef};
use oci_distribution::manifest::{ImageIndexEntry, OciImageIndex, OciImageManifest, OciManifest};

/// 负责把 manifest（单架构或多架构 index）从 source Registry 拷贝到 target Registry。
///
/// 它只依赖 `Registry` trait，不依赖任何具体 Registry 实现。
pub struct ManifestCopier<'a> {
    source: &'a dyn Registry,
    target: &'a dyn Registry,
    source_auth: &'a Option<Auth>,
    target_auth: &'a Option<Auth>,
    progress: &'a dyn ProgressSink,
    platform: Option<String>,
}

impl<'a> ManifestCopier<'a> {
    pub fn new(
        source: &'a dyn Registry,
        target: &'a dyn Registry,
        source_auth: &'a Option<Auth>,
        target_auth: &'a Option<Auth>,
        progress: &'a dyn ProgressSink,
        platform: Option<String>,
    ) -> Self {
        Self {
            source,
            target,
            source_auth,
            target_auth,
            progress,
            platform,
        }
    }

    /// 从 source 拷贝 manifest 到 target，保留 digest。
    pub async fn copy(
        &self,
        source_ref: &RepositoryRef,
        target_ref: &RepositoryRef,
    ) -> Result<(), RegistryError> {
        let manifest = self
            .source
            .pull_manifest(source_ref, self.source_auth)
            .await?;

        let oci_manifest: OciManifest = serde_json::from_slice(&manifest.bytes)
            .map_err(|e| RegistryError::ParseManifest(e.to_string()))?;

        match oci_manifest {
            OciManifest::Image(_) => {
                tracing::debug!("同步单架构镜像");
                self.copy_single_manifest(source_ref, target_ref, &manifest.bytes)
                    .await?;
            }
            OciManifest::ImageIndex(index) => {
                tracing::debug!(
                    "同步多架构镜像 index，包含 {} 个子 manifest",
                    index.manifests.len()
                );
                self.copy_image_index(source_ref, target_ref, &index)
                    .await?;

                self.target
                    .push_manifest(target_ref, &manifest, self.target_auth)
                    .await?;
            }
        }

        Ok(())
    }

    async fn copy_single_manifest(
        &self,
        source_ref: &RepositoryRef,
        target_ref: &RepositoryRef,
        manifest_body: &[u8],
    ) -> Result<(), RegistryError> {
        let manifest: OciImageManifest = serde_json::from_slice(manifest_body)
            .map_err(|e| RegistryError::ParseManifest(e.to_string()))?;

        let total_blobs = 1 + manifest.layers.len();
        let total_bytes = manifest.config.size.max(0) as u64
            + manifest
                .layers
                .iter()
                .map(|l| l.size.max(0) as u64)
                .sum::<u64>();
        self.progress.init_manifest(total_blobs, total_bytes);

        self.copy_blob(
            source_ref,
            target_ref,
            &manifest.config.digest,
            Some(manifest.config.size.max(0) as u64),
        )
        .await?;

        for layer in &manifest.layers {
            self.copy_blob(
                source_ref,
                target_ref,
                &layer.digest,
                Some(layer.size.max(0) as u64),
            )
            .await?;
        }

        let media_type = manifest
            .media_type
            .clone()
            .unwrap_or_else(|| "application/vnd.oci.image.manifest.v1+json".to_string());

        self.target
            .push_manifest(
                target_ref,
                &Manifest::new(manifest_body.to_vec(), media_type),
                self.target_auth,
            )
            .await?;

        Ok(())
    }

    async fn copy_blob(
        &self,
        source_ref: &RepositoryRef,
        target_ref: &RepositoryRef,
        digest: &str,
        size: Option<u64>,
    ) -> Result<(), RegistryError> {
        let ctx = BlobContext {
            digest: digest.to_string(),
            size,
        };

        // 目标仓库如果已经有这个 blob，直接跳过，避免重复拉取和推送。
        if self
            .target
            .blob_exists(target_ref, digest, self.target_auth)
            .await?
        {
            self.progress.on_skip(&ctx, Direction::Push);
            return Ok(());
        }

        tracing::debug!("拉取 blob {} ...", digest);
        let data = self
            .source
            .pull_blob(
                source_ref,
                digest,
                ctx.size,
                self.source_auth,
                self.progress,
            )
            .await?;
        tracing::debug!("拉取 blob {} 完成", digest);

        tracing::debug!("推送 blob {} ({} bytes)...", digest, data.len());
        self.target
            .push_blob(target_ref, digest, data, self.target_auth, self.progress)
            .await?;
        tracing::debug!("推送 blob {} 完成", digest);

        Ok(())
    }

    fn filter_platforms<'b>(
        &self,
        manifests: &'b [ImageIndexEntry],
    ) -> Result<Vec<&'b ImageIndexEntry>, RegistryError> {
        let filter = match &self.platform {
            None => return Ok(manifests.iter().collect()),
            Some(f) => f,
        };

        let parts: Vec<&str> = filter.split('/').collect();
        if parts.len() != 2 && parts.len() != 3 {
            return Err(RegistryError::InvalidReference(format!(
                "平台格式错误: {}（应为 os/arch 或 os/arch/variant）",
                filter
            )));
        }

        let (want_os, want_arch, want_variant) = (parts[0], parts[1], parts.get(2).copied());

        let matched: Vec<_> = manifests
            .iter()
            .filter(|entry| {
                let platform = match &entry.platform {
                    Some(p) => p,
                    None => return false,
                };
                if platform.os != want_os || platform.architecture != want_arch {
                    return false;
                }
                if let Some(want) = want_variant {
                    platform
                        .variant
                        .as_deref()
                        .map(|v| v == want)
                        .unwrap_or(false)
                } else {
                    true
                }
            })
            .collect();

        if matched.is_empty() {
            return Err(RegistryError::InvalidReference(format!(
                "没有匹配平台 {} 的子 manifest",
                filter
            )));
        }

        Ok(matched)
    }

    async fn copy_image_index(
        &self,
        source_ref: &RepositoryRef,
        target_ref: &RepositoryRef,
        index: &OciImageIndex,
    ) -> Result<(), RegistryError> {
        let manifests = self.filter_platforms(&index.manifests)?;
        if manifests.is_empty() {
            return Err(RegistryError::ManifestUnknown);
        }

        for entry in manifests {
            let digest = &entry.digest;

            let child_source_ref =
                RepositoryRef::with_digest(&source_ref.registry, &source_ref.repository, digest);

            // 绕过 oci-distribution 0.11 的 bug：当目标 Registry 推送 manifest 后不返回
            // Location header 时，push_manifest_raw 对 digest reference 会 panic。
            // 使用一个临时 tag 推送子 manifest，manifest 实际仍按 digest 存储，index 也仍按 digest 引用它。
            let child_target_tag = format!("_bamboo_child_{}", digest.replace(':', "_"));
            let child_target_ref = RepositoryRef::with_tag(
                &target_ref.registry,
                &target_ref.repository,
                &child_target_tag,
            );

            let manifest = self
                .source
                .pull_manifest(&child_source_ref, self.source_auth)
                .await?;

            let platform = entry
                .platform
                .as_ref()
                .map(|p| {
                    if let Some(variant) = &p.variant {
                        format!("{}/{}/{}", p.os, p.architecture, variant)
                    } else {
                        format!("{}/{}", p.os, p.architecture)
                    }
                })
                .unwrap_or_else(|| "unknown".to_string());
            self.progress.set_platform(Some(platform.clone()));
            self.copy_single_manifest(&child_source_ref, &child_target_ref, &manifest.bytes)
                .await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::NoopProgressSink;
    use crate::registry::InMemoryRegistry;
    use std::collections::HashMap;

    const MANIFEST_MEDIA_TYPE: &str = "application/vnd.docker.distribution.manifest.v2+json";
    const CONFIG_MEDIA_TYPE: &str = "application/vnd.docker.container.image.v1+json";
    const LAYER_MEDIA_TYPE: &str = "application/vnd.docker.image.rootfs.diff.tar.gzip";

    fn sha256_hex(data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("sha256:{:x}", hasher.finalize())
    }

    fn sample_image() -> (Vec<u8>, HashMap<String, Vec<u8>>, String) {
        let config = br#"{"architecture":"amd64","config":{}}"#.to_vec();
        let layer = b"fake-layer-content".to_vec();

        let config_digest = sha256_hex(&config);
        let layer_digest = sha256_hex(&layer);

        let manifest = format!(
            r#"{{
  "schemaVersion": 2,
  "mediaType": "{}",
  "config": {{
    "mediaType": "{}",
    "size": {},
    "digest": "{}"
  }},
  "layers": [
    {{
      "mediaType": "{}",
      "size": {},
      "digest": "{}"
    }}
  ]
}}"#,
            MANIFEST_MEDIA_TYPE,
            CONFIG_MEDIA_TYPE,
            config.len(),
            config_digest,
            LAYER_MEDIA_TYPE,
            layer.len(),
            layer_digest
        )
        .into_bytes();

        let digest = sha256_hex(&manifest);

        let mut blobs = HashMap::new();
        blobs.insert(config_digest, config);
        blobs.insert(layer_digest, layer);

        (manifest, blobs, digest)
    }

    #[tokio::test]
    async fn copy_between_in_memory_registries() {
        let (manifest, blobs, digest) = sample_image();

        let source = InMemoryRegistry::new();
        let target = InMemoryRegistry::new();

        let source_ref = RepositoryRef::with_tag("localhost", "library/nginx", "1.25");
        let target_ref = RepositoryRef::with_tag("localhost", "library/nginx", "1.25");

        source.add_manifest(
            source_ref.clone(),
            Manifest::new(manifest.clone(), MANIFEST_MEDIA_TYPE),
            &digest,
        );
        for (d, b) in &blobs {
            source.add_blob(d, b.clone());
        }

        let progress = NoopProgressSink;
        let copier = ManifestCopier::new(&source, &target, &None, &None, &progress, None);
        copier.copy(&source_ref, &target_ref).await.unwrap();

        let dest_digest = target.digest(&target_ref, &None).await.unwrap();
        assert_eq!(dest_digest, Some(digest));

        let copied = target.get_manifest(&target_ref).unwrap();
        assert_eq!(copied.bytes, manifest);

        for (d, b) in &blobs {
            assert_eq!(target.get_blob(d).as_ref(), Some(b));
        }
    }
}
