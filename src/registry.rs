use crate::auth::Auth;
use crate::error::{BambooError, Result};
use crate::logging;
use http::header::HeaderValue;
use oci_distribution::client::{Client, ClientConfig, ClientProtocol};
use oci_distribution::manifest::{OciImageIndex, OciImageManifest, OciManifest};
use oci_distribution::secrets::RegistryAuth;
use oci_distribution::Reference;

const ACCEPTED_MEDIA_TYPES: &[&str] = &[
    "application/vnd.docker.distribution.manifest.list.v2+json",
    "application/vnd.oci.image.index.v1+json",
    "application/vnd.docker.distribution.manifest.v2+json",
    "application/vnd.oci.image.manifest.v1+json",
];

pub struct RegistryClient {
    client: Client,
    reference: Reference,
}

impl RegistryClient {
    pub fn new(
        registry: &str,
        image_path: &str,
        tag: &str,
        insecure: bool,
        skip_tls_verify: bool,
    ) -> Result<Self> {
        let reference_str = format!("{}/{image_path}:{tag}", registry);
        let reference: Reference = reference_str
            .parse()
            .map_err(|e| BambooError::Registry(format!("镜像引用无效: {e}")))?;

        let protocol = if insecure {
            ClientProtocol::Http
        } else {
            ClientProtocol::Https
        };

        let config = ClientConfig {
            protocol,
            accept_invalid_hostnames: skip_tls_verify,
            accept_invalid_certificates: skip_tls_verify,
            ..Default::default()
        };

        let client = Client::new(config);

        Ok(Self { client, reference })
    }

    pub async fn digest(&self, auth: &Option<Auth>) -> Result<Option<String>> {
        let registry_auth = auth_to_registry_auth(auth);
        match self
            .client
            .fetch_manifest_digest(&self.reference, &registry_auth)
            .await
        {
            Ok(digest) => Ok(Some(digest)),
            Err(oci_distribution::errors::OciDistributionError::RegistryError {
                envelope, ..
            }) if envelope
                .errors
                .iter()
                .any(|e| e.code == oci_distribution::errors::OciErrorCode::ManifestUnknown) =>
            {
                Ok(None)
            }
            Err(e) => Err(BambooError::Registry(format!("获取 digest 失败: {e}"))),
        }
    }

    pub async fn copy_from(
        &self,
        source: &RegistryClient,
        source_auth: &Option<Auth>,
    ) -> Result<()> {
        let source_registry_auth = auth_to_registry_auth(source_auth);

        // Pull the manifest raw so we can preserve multi-arch indexes verbatim.
        let (manifest_body, _digest) = source
            .client
            .pull_manifest_raw(
                &source.reference,
                &source_registry_auth,
                ACCEPTED_MEDIA_TYPES,
            )
            .await
            .map_err(|e| BambooError::Registry(format!("拉取 manifest 失败: {e}")))?;

        let manifest: OciManifest = serde_json::from_slice(&manifest_body)
            .map_err(|e| BambooError::Registry(format!("解析 manifest 失败: {e}")))?;

        match &manifest {
            OciManifest::Image(_) => {
                logging::debug("同步单架构镜像");
                self.copy_single_manifest(
                    source,
                    &source.reference,
                    &self.reference,
                    &manifest_body,
                    "",
                )
                .await?;
            }
            OciManifest::ImageIndex(index) => {
                logging::debug(&format!(
                    "同步多架构镜像 index，包含 {} 个子 manifest",
                    index.manifests.len()
                ));
                // Multi-arch image: copy each platform manifest by digest, then push the index.
                self.copy_image_index(source, index, &source_registry_auth)
                    .await?;

                let content_type = HeaderValue::from_str(
                    index
                        .media_type
                        .as_deref()
                        .unwrap_or("application/vnd.oci.image.index.v1+json"),
                )
                .map_err(|e| BambooError::Registry(format!("Content-Type 无效: {e}")))?;

                self.client
                    .push_manifest_raw(&self.reference, manifest_body, content_type)
                    .await
                    .map_err(|e| BambooError::Registry(format!("推送 manifest index 失败: {e}")))?;
            }
        }

        Ok(())
    }

    /// Copy a single image manifest (config + layers + manifest raw) from source to target.
    async fn copy_single_manifest(
        &self,
        source: &RegistryClient,
        source_ref: &Reference,
        target_ref: &Reference,
        manifest_body: &[u8],
        prefix: &str,
    ) -> Result<()> {
        let manifest: OciImageManifest = serde_json::from_slice(manifest_body)
            .map_err(|e| BambooError::Registry(format!("{prefix}解析 manifest 失败: {e}")))?;

        let mut config = Vec::new();
        logging::info(&format!(
            "{prefix}拉取 config {} ({} bytes)...",
            manifest.config.digest, manifest.config.size
        ));
        source
            .client
            .pull_blob(source_ref, &manifest.config, &mut config)
            .await
            .map_err(|e| {
                BambooError::Registry(format!(
                    "{prefix}拉取 config {} 失败: {}",
                    manifest.config.digest, e
                ))
            })?;
        logging::info(&format!(
            "{prefix}拉取 config {} 完成",
            manifest.config.digest
        ));

        logging::info(&format!(
            "{prefix}推送 config {} ({} bytes)...",
            manifest.config.digest,
            config.len()
        ));
        self.client
            .push_blob(target_ref, &config, &manifest.config.digest)
            .await
            .map_err(|e| {
                BambooError::Registry(format!(
                    "{prefix}推送 config {} 失败: {}",
                    manifest.config.digest, e
                ))
            })?;
        logging::info(&format!(
            "{prefix}推送 config {} 完成",
            manifest.config.digest
        ));

        for layer in &manifest.layers {
            let mut data = Vec::new();
            logging::info(&format!(
                "{prefix}拉取 layer {} ({} bytes)...",
                layer.digest, layer.size
            ));
            source
                .client
                .pull_blob(source_ref, layer, &mut data)
                .await
                .map_err(|e| {
                    BambooError::Registry(format!(
                        "{prefix}拉取 layer {} 失败: {}",
                        layer.digest, e
                    ))
                })?;
            logging::info(&format!("{prefix}拉取 layer {} 完成", layer.digest));

            logging::info(&format!(
                "{prefix}推送 layer {} ({} bytes)...",
                layer.digest,
                data.len()
            ));
            self.client
                .push_blob(target_ref, &data, &layer.digest)
                .await
                .map_err(|e| {
                    BambooError::Registry(format!(
                        "{prefix}推送 layer {} 失败: {}",
                        layer.digest, e
                    ))
                })?;
            logging::info(&format!("{prefix}推送 layer {} 完成", layer.digest));
        }

        let content_type = HeaderValue::from_str(
            manifest
                .media_type
                .as_deref()
                .unwrap_or("application/vnd.oci.image.manifest.v1+json"),
        )
        .map_err(|e| BambooError::Registry(format!("{prefix}Content-Type 无效: {e}")))?;

        self.client
            .push_manifest_raw(target_ref, manifest_body.to_vec(), content_type)
            .await
            .map_err(|e| BambooError::Registry(format!("{prefix}推送 manifest 失败: {e}")))?;

        Ok(())
    }

    async fn copy_image_index(
        &self,
        source: &RegistryClient,
        index: &OciImageIndex,
        source_auth: &RegistryAuth,
    ) -> Result<()> {
        for entry in &index.manifests {
            let digest = &entry.digest;
            let child_ref = Reference::with_digest(
                source.reference.registry().to_string(),
                source.reference.repository().to_string(),
                digest.clone(),
            );

            let target_child_ref = Reference::with_digest(
                self.reference.registry().to_string(),
                self.reference.repository().to_string(),
                digest.clone(),
            );

            // Pull the child manifest raw so its digest is preserved in the target.
            let (manifest_body, _) = source
                .client
                .pull_manifest_raw(&child_ref, source_auth, ACCEPTED_MEDIA_TYPES)
                .await
                .map_err(|e| {
                    BambooError::Registry(format!("拉取子 manifest {} 失败: {}", digest, e))
                })?;

            self.copy_single_manifest(source, &child_ref, &target_child_ref, &manifest_body, "子 ")
                .await?;
        }

        Ok(())
    }
}

fn auth_to_registry_auth(auth: &Option<Auth>) -> RegistryAuth {
    match auth {
        Some(a) => RegistryAuth::Basic(a.username.clone(), a.password.clone()),
        None => RegistryAuth::Anonymous,
    }
}
