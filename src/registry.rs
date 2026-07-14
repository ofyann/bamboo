use crate::auth::Auth;
use crate::error::{BambooError, Result};
use oci_distribution::client::{Client, ClientConfig, ClientProtocol};
use oci_distribution::manifest::{OciImageIndex, OciImageManifest, OciManifest};
use oci_distribution::secrets::RegistryAuth;
use oci_distribution::Reference;
use http::header::HeaderValue;

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
    pub fn new(registry: &str, image_path: &str, tag: &str, insecure: bool) -> Result<Self> {
        let reference_str = format!("{}/{image_path}:{tag}", registry);
        let reference: Reference = reference_str
            .parse()
            .map_err(|e| BambooError::Registry(format!("invalid reference: {e}")))?;

        let protocol = if insecure {
            ClientProtocol::Http
        } else {
            ClientProtocol::Https
        };

        let config = ClientConfig {
            protocol,
            ..Default::default()
        };

        let client = Client::new(config);

        Ok(Self {
            client,
            reference,
        })
    }

    pub async fn digest(&self, auth: &Option<Auth>) -> Result<Option<String>> {
        let registry_auth = auth_to_registry_auth(auth);
        match self
            .client
            .fetch_manifest_digest(&self.reference, &registry_auth)
            .await
        {
            Ok(digest) => Ok(Some(digest)),
            Err(oci_distribution::errors::OciDistributionError::RegistryError { envelope, .. })
                if envelope
                    .errors
                    .iter()
                    .any(|e| e.code == oci_distribution::errors::OciErrorCode::ManifestUnknown) =>
            {
                Ok(None)
            }
            Err(e) => Err(BambooError::Registry(format!("inspect failed: {e}"))),
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
            .pull_manifest_raw(&source.reference, &source_registry_auth, ACCEPTED_MEDIA_TYPES)
            .await
            .map_err(|e| BambooError::Registry(format!("pull manifest failed: {e}")))?;

        let manifest: OciManifest = serde_json::from_slice(&manifest_body)
            .map_err(|e| BambooError::Registry(format!("parse manifest failed: {e}")))?;

        match manifest {
            OciManifest::Image(_) => {
                // Single-arch image: copy blobs by digest and push the manifest raw
                // so the destination digest matches the source.
                let manifest: OciImageManifest = serde_json::from_slice(&manifest_body)
                    .map_err(|e| BambooError::Registry(format!("parse manifest failed: {e}")))?;

                let mut config = Vec::new();
                source
                    .client
                    .pull_blob(&source.reference, &manifest.config, &mut config)
                    .await
                    .map_err(|e| {
                        BambooError::Registry(format!(
                            "pull config {} failed: {}",
                            manifest.config.digest, e
                        ))
                    })?;
                self.client
                    .push_blob(&self.reference, &config, &manifest.config.digest)
                    .await
                    .map_err(|e| {
                        BambooError::Registry(format!(
                            "push config {} failed: {}",
                            manifest.config.digest, e
                        ))
                    })?;

                for layer in &manifest.layers {
                    let mut data = Vec::new();
                    source
                        .client
                        .pull_blob(&source.reference, layer, &mut data)
                        .await
                        .map_err(|e| {
                            BambooError::Registry(format!(
                                "pull layer {} failed: {}",
                                layer.digest, e
                            ))
                        })?;
                    self.client
                        .push_blob(&self.reference, &data, &layer.digest)
                        .await
                        .map_err(|e| {
                            BambooError::Registry(format!(
                                "push layer {} failed: {}",
                                layer.digest, e
                            ))
                        })?;
                }

                let content_type = HeaderValue::from_str(
                    manifest
                        .media_type
                        .as_deref()
                        .unwrap_or("application/vnd.oci.image.manifest.v1+json"),
                )
                .map_err(|e| BambooError::Registry(format!("invalid content type: {e}")))?;

                self.client
                    .push_manifest_raw(&self.reference, manifest_body, content_type)
                    .await
                    .map_err(|e| BambooError::Registry(format!("push manifest failed: {e}")))?;
            }
            OciManifest::ImageIndex(index) => {
                // Multi-arch image: copy each platform manifest by digest, then push the index.
                self.copy_image_index(source, &index, &source_registry_auth)
                    .await?;

                let content_type = HeaderValue::from_str(
                    index
                        .media_type
                        .as_deref()
                        .unwrap_or("application/vnd.oci.image.index.v1+json"),
                )
                .map_err(|e| BambooError::Registry(format!("invalid content type: {e}")))?;

                self.client
                    .push_manifest_raw(&self.reference, manifest_body, content_type)
                    .await
                    .map_err(|e| BambooError::Registry(format!("push manifest index failed: {e}")))?;
            }
        }

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
                    BambooError::Registry(format!(
                        "pull child manifest {} failed: {}",
                        digest, e
                    ))
                })?;

            let manifest: OciImageManifest = serde_json::from_slice(&manifest_body)
                .map_err(|e| {
                    BambooError::Registry(format!(
                        "parse child manifest {} failed: {}",
                        digest, e
                    ))
                })?;

            let mut config = Vec::new();
            source
                .client
                .pull_blob(&child_ref, &manifest.config, &mut config)
                .await
                .map_err(|e| {
                    BambooError::Registry(format!(
                        "pull child config {} failed: {}",
                        manifest.config.digest, e
                    ))
                })?;
            self.client
                .push_blob(&target_child_ref, &config, &manifest.config.digest)
                .await
                .map_err(|e| {
                    BambooError::Registry(format!(
                        "push child config {} failed: {}",
                        manifest.config.digest, e
                    ))
                })?;

            for layer in &manifest.layers {
                let mut data = Vec::new();
                source
                    .client
                    .pull_blob(&child_ref, layer, &mut data)
                    .await
                    .map_err(|e| {
                        BambooError::Registry(format!(
                            "pull child layer {} failed: {}",
                            layer.digest, e
                        ))
                    })?;
                self.client
                    .push_blob(&target_child_ref, &data, &layer.digest)
                    .await
                    .map_err(|e| {
                        BambooError::Registry(format!(
                            "push child layer {} failed: {}",
                            layer.digest, e
                        ))
                    })?;
            }

            let content_type = HeaderValue::from_str(
                manifest
                    .media_type
                    .as_deref()
                    .unwrap_or("application/vnd.oci.image.manifest.v1+json"),
            )
            .map_err(|e| {
                BambooError::Registry(format!(
                    "invalid child manifest content type: {}",
                    e
                ))
            })?;

            self.client
                .push_manifest_raw(&target_child_ref, manifest_body, content_type)
                .await
                .map_err(|e| {
                    BambooError::Registry(format!(
                        "push child manifest {} failed: {}",
                        digest, e
                    ))
                })?;
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
