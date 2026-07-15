use crate::auth::Auth;
use crate::registry::{Manifest, Registry, RegistryError, RepositoryRef};
use std::collections::HashMap;
use std::sync::Mutex;

/// 内存 Registry，用于单元测试。
///
/// 不验证认证信息，只根据 manifest/blob 是否存在来响应。
#[derive(Debug, Default)]
pub struct InMemoryRegistry {
    manifests: Mutex<HashMap<RepositoryRef, (Manifest, String)>>,
    blobs: Mutex<HashMap<String, Vec<u8>>>,
}

impl InMemoryRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加一个 manifest，并指定其 digest（测试代码负责计算）。
    pub fn add_manifest(&self, repo: RepositoryRef, manifest: Manifest, digest: impl Into<String>) {
        self.manifests
            .lock()
            .unwrap()
            .insert(repo, (manifest, digest.into()));
    }

    pub fn add_blob(&self, digest: impl Into<String>, data: Vec<u8>) {
        self.blobs.lock().unwrap().insert(digest.into(), data);
    }

    pub fn get_manifest(&self, repo: &RepositoryRef) -> Option<Manifest> {
        self.manifests
            .lock()
            .unwrap()
            .get(repo)
            .map(|(m, _)| m.clone())
    }

    pub fn get_blob(&self, digest: &str) -> Option<Vec<u8>> {
        self.blobs.lock().unwrap().get(digest).cloned()
    }
}

#[async_trait::async_trait]
impl Registry for InMemoryRegistry {
    async fn digest(
        &self,
        repo: &RepositoryRef,
        _auth: &Option<Auth>,
    ) -> Result<Option<String>, RegistryError> {
        let manifests = self.manifests.lock().unwrap();
        match manifests.get(repo) {
            Some((_, digest)) => Ok(Some(digest.clone())),
            None => Ok(None),
        }
    }

    async fn pull_manifest(
        &self,
        repo: &RepositoryRef,
        _auth: &Option<Auth>,
    ) -> Result<Manifest, RegistryError> {
        self.manifests
            .lock()
            .unwrap()
            .get(repo)
            .map(|(m, _)| m.clone())
            .ok_or(RegistryError::ManifestUnknown)
    }

    async fn push_manifest(
        &self,
        repo: &RepositoryRef,
        manifest: &Manifest,
        _auth: &Option<Auth>,
    ) -> Result<(), RegistryError> {
        let digest = sha256_hex(&manifest.bytes);
        self.manifests
            .lock()
            .unwrap()
            .insert(repo.clone(), (manifest.clone(), digest));
        Ok(())
    }

    async fn pull_blob(
        &self,
        _repo: &RepositoryRef,
        digest: &str,
        _auth: &Option<Auth>,
    ) -> Result<Vec<u8>, RegistryError> {
        self.blobs
            .lock()
            .unwrap()
            .get(digest)
            .cloned()
            .ok_or_else(|| RegistryError::PullBlob(format!("blob {digest} 不存在")))
    }

    async fn push_blob(
        &self,
        _repo: &RepositoryRef,
        digest: &str,
        data: Vec<u8>,
        _auth: &Option<Auth>,
    ) -> Result<(), RegistryError> {
        self.blobs.lock().unwrap().insert(digest.to_string(), data);
        Ok(())
    }

    async fn blob_exists(
        &self,
        _repo: &RepositoryRef,
        digest: &str,
        _auth: &Option<Auth>,
    ) -> Result<bool, RegistryError> {
        Ok(self.blobs.lock().unwrap().contains_key(digest))
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}
