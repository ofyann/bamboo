use crate::auth::Auth;
use std::fmt;
use thiserror::Error;

pub mod copier;
pub mod memory;
pub mod oci;

pub use copier::ManifestCopier;
pub use memory::InMemoryRegistry;
pub use oci::OciRegistry;

/// Registry 操作的底层错误。
///
/// 这个错误类型故意与 `BambooError` 分离，让 `Registry` trait 的 seam
/// 不依赖上层错误类型，便于测试 fake 和 future adapter 接入。
#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("镜像引用无效: {0}")]
    InvalidReference(String),

    #[error("manifest 不存在")]
    ManifestUnknown,

    #[error("拉取 manifest 失败: {0}")]
    PullManifest(String),

    #[error("推送 manifest 失败: {0}")]
    PushManifest(String),

    #[error("拉取 blob 失败: {0}")]
    PullBlob(String),

    #[error("推送 blob 失败: {0}")]
    PushBlob(String),

    #[error("解析 manifest 失败: {0}")]
    ParseManifest(String),

    #[error("Content-Type 无效: {0}")]
    InvalidContentType(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

/// 一个仓库引用：registry + repository + tag 或 digest。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepositoryRef {
    pub registry: String,
    pub repository: String,
    pub reference: Reference,
}

impl RepositoryRef {
    pub fn with_tag(
        registry: impl Into<String>,
        repository: impl Into<String>,
        tag: impl Into<String>,
    ) -> Self {
        Self {
            registry: registry.into(),
            repository: repository.into(),
            reference: Reference::Tag(tag.into()),
        }
    }

    pub fn with_digest(
        registry: impl Into<String>,
        repository: impl Into<String>,
        digest: impl Into<String>,
    ) -> Self {
        Self {
            registry: registry.into(),
            repository: repository.into(),
            reference: Reference::Digest(digest.into()),
        }
    }
}

impl fmt::Display for RepositoryRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.registry, self.repository)?;
        match &self.reference {
            Reference::Tag(tag) => write!(f, ":{tag}"),
            Reference::Digest(digest) => write!(f, "@{digest}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Reference {
    Tag(String),
    Digest(String),
}

/// 从 Registry 拉取或推送到 Registry 的 manifest 负载。
#[derive(Debug, Clone)]
pub struct Manifest {
    pub bytes: Vec<u8>,
    pub media_type: String,
}

impl Manifest {
    pub fn new(bytes: Vec<u8>, media_type: impl Into<String>) -> Self {
        Self {
            bytes,
            media_type: media_type.into(),
        }
    }
}

/// 一次 Registry 连接可以执行的最小操作集合。
#[async_trait::async_trait]
pub trait Registry: Send + Sync {
    /// 获取指定引用的 digest；不存在时返回 `None`。
    async fn digest(
        &self,
        repo: &RepositoryRef,
        auth: &Option<Auth>,
    ) -> Result<Option<String>, RegistryError>;

    /// 拉取 manifest 原始字节与 media type。
    async fn pull_manifest(
        &self,
        repo: &RepositoryRef,
        auth: &Option<Auth>,
    ) -> Result<Manifest, RegistryError>;

    /// 推送 manifest 原始字节与 media type。
    async fn push_manifest(
        &self,
        repo: &RepositoryRef,
        manifest: &Manifest,
        auth: &Option<Auth>,
    ) -> Result<(), RegistryError>;

    /// 拉取 blob 原始字节。
    async fn pull_blob(
        &self,
        repo: &RepositoryRef,
        digest: &str,
        auth: &Option<Auth>,
    ) -> Result<Vec<u8>, RegistryError>;

    /// 推送 blob 原始字节。
    async fn push_blob(
        &self,
        repo: &RepositoryRef,
        digest: &str,
        data: Vec<u8>,
        auth: &Option<Auth>,
    ) -> Result<(), RegistryError>;

    /// 检查 blob 是否已存在。
    async fn blob_exists(
        &self,
        repo: &RepositoryRef,
        digest: &str,
        auth: &Option<Auth>,
    ) -> Result<bool, RegistryError>;
}
