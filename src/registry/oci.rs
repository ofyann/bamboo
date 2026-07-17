use crate::auth::Auth;
use crate::progress::{BlobContext, Direction, ProgressSink};
use crate::registry::{Manifest, Reference, Registry, RegistryError, RepositoryRef};
use http::header::HeaderValue;
use oci_distribution::client::{Client, ClientConfig, ClientProtocol};
use oci_distribution::manifest::OciDescriptor;
use oci_distribution::secrets::RegistryAuth;
use oci_distribution::Reference as OciReference;

const ACCEPTED_MEDIA_TYPES: &[&str] = &[
    "application/vnd.docker.distribution.manifest.list.v2+json",
    "application/vnd.oci.image.index.v1+json",
    "application/vnd.docker.distribution.manifest.v2+json",
    "application/vnd.oci.image.manifest.v1+json",
];

/// 基于 `oci-distribution` 的 Registry adapter。
pub struct OciRegistry {
    client: Client,
}

impl OciRegistry {
    pub fn new(insecure: bool, skip_tls_verify: bool) -> Self {
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

        Self {
            client: Client::new(config),
        }
    }
}

fn to_oci_reference(repo: &RepositoryRef) -> Result<OciReference, RegistryError> {
    let registry = &repo.registry;
    let repository = &repo.repository;
    let s = match &repo.reference {
        Reference::Tag(tag) => format!("{registry}/{repository}:{tag}"),
        Reference::Digest(digest) => format!("{registry}/{repository}@{digest}"),
    };
    s.parse()
        .map_err(|e| RegistryError::InvalidReference(format!("{s}: {e}")))
}

fn auth_to_registry_auth(auth: &Option<Auth>) -> RegistryAuth {
    match auth {
        Some(a) => RegistryAuth::Basic(a.username.clone(), a.password.clone()),
        None => RegistryAuth::Anonymous,
    }
}

#[async_trait::async_trait]
impl Registry for OciRegistry {
    async fn digest(
        &self,
        repo: &RepositoryRef,
        auth: &Option<Auth>,
    ) -> Result<Option<String>, RegistryError> {
        let reference = to_oci_reference(repo)?;
        let registry_auth = auth_to_registry_auth(auth);

        match self
            .client
            .fetch_manifest_digest(&reference, &registry_auth)
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
            Err(e) => Err(RegistryError::PullManifest(e.to_string())),
        }
    }

    async fn pull_manifest(
        &self,
        repo: &RepositoryRef,
        auth: &Option<Auth>,
    ) -> Result<Manifest, RegistryError> {
        let reference = to_oci_reference(repo)?;
        let registry_auth = auth_to_registry_auth(auth);

        let (bytes, _digest) = self
            .client
            .pull_manifest_raw(&reference, &registry_auth, ACCEPTED_MEDIA_TYPES)
            .await
            .map_err(|e| RegistryError::PullManifest(e.to_string()))?;

        // oci-distribution 的 pull_manifest_raw 不直接返回 media type；
        // 我们在上层通过解析 OciManifest 或 index 的 media_type 字段来处理。
        // 这里先用 accept header 里允许的类型作为占位，Copier 解析后会重新使用真实 media type。
        let media_type = guess_media_type(&bytes);
        Ok(Manifest::new(bytes, media_type))
    }

    async fn push_manifest(
        &self,
        repo: &RepositoryRef,
        manifest: &Manifest,
        _auth: &Option<Auth>,
    ) -> Result<(), RegistryError> {
        let reference = to_oci_reference(repo)?;

        let content_type = HeaderValue::from_str(&manifest.media_type)
            .map_err(|e| RegistryError::InvalidContentType(e.to_string()))?;

        self.client
            .push_manifest_raw(&reference, manifest.bytes.clone(), content_type)
            .await
            .map_err(|e| RegistryError::PushManifest(e.to_string()))?;

        Ok(())
    }

    async fn pull_blob(
        &self,
        repo: &RepositoryRef,
        digest: &str,
        total: Option<u64>,
        _auth: &Option<Auth>,
        sink: &dyn ProgressSink,
    ) -> Result<Vec<u8>, RegistryError> {
        let reference = to_oci_reference(repo)?;
        let descriptor = descriptor_from_digest(digest, total.unwrap_or(0));
        let ctx = BlobContext {
            digest: digest.to_string(),
            size: total,
        };

        sink.on_start(&ctx, Direction::Pull);
        let mut writer = CountingWriter::new(sink, ctx.clone());
        self.client
            .pull_blob(&reference, &descriptor, &mut writer)
            .await
            .map_err(|e| RegistryError::PullBlob(e.to_string()))?;
        let data = writer.into_inner();
        sink.on_complete(&ctx, Direction::Pull, data.len() as u64);
        Ok(data)
    }

    async fn push_blob(
        &self,
        repo: &RepositoryRef,
        digest: &str,
        data: Vec<u8>,
        _auth: &Option<Auth>,
        sink: &dyn ProgressSink,
    ) -> Result<(), RegistryError> {
        let reference = to_oci_reference(repo)?;
        let ctx = BlobContext {
            digest: digest.to_string(),
            size: Some(data.len() as u64),
        };

        sink.on_start(&ctx, Direction::Push);
        self.client
            .push_blob(&reference, &data, digest)
            .await
            .map_err(|e| RegistryError::PushBlob(e.to_string()))?;
        sink.on_complete(&ctx, Direction::Push, data.len() as u64);
        Ok(())
    }

    async fn blob_exists(
        &self,
        _repo: &RepositoryRef,
        _digest: &str,
        _auth: &Option<Auth>,
    ) -> Result<bool, RegistryError> {
        // 暂时禁用基于 pull_blob_stream 的本地存在性探测：
        // 部分 Registry 对该探测返回假阳性，导致 manifest 推送时报 BLOB_UNKNOWN。
        // 直接返回 false，让 push_blob 自行处理去重（oci-distribution 内部会先 HEAD 检查）。
        Ok(false)
    }
}

fn descriptor_from_digest(digest: &str, size: u64) -> OciDescriptor {
    OciDescriptor {
        digest: digest.to_string(),
        media_type: "application/octet-stream".to_string(),
        size: size as i64,
        urls: None,
        annotations: None,
    }
}

/// 一个写入 Vec<u8> 并同时报告进度的 `tokio::io::AsyncWrite` 包装器。
struct CountingWriter<'a> {
    sink: &'a dyn ProgressSink,
    ctx: BlobContext,
    buffer: Vec<u8>,
    current: u64,
}

impl<'a> CountingWriter<'a> {
    fn new(sink: &'a dyn ProgressSink, ctx: BlobContext) -> Self {
        Self {
            sink,
            ctx,
            buffer: Vec::new(),
            current: 0,
        }
    }

    fn into_inner(self) -> Vec<u8> {
        self.buffer
    }
}

impl tokio::io::AsyncWrite for CountingWriter<'_> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        this.buffer.extend_from_slice(buf);
        this.current += buf.len() as u64;
        this.sink
            .on_progress(&this.ctx, Direction::Pull, this.current);
        std::task::Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

fn guess_media_type(bytes: &[u8]) -> String {
    // 快速启发式：根据 JSON 里的 mediaType 字段推断。
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) {
        if let Some(media_type) = value.get("mediaType").and_then(|v| v.as_str()) {
            return media_type.to_string();
        }
    }
    "application/vnd.oci.image.manifest.v1+json".to_string()
}
