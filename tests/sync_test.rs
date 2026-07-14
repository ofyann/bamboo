mod common;

use bamboo::registry::RegistryClient;
use common::mock_registry::MockRegistry;
use std::collections::HashMap;

const MANIFEST_MEDIA_TYPE: &str = "application/vnd.docker.distribution.manifest.v2+json";
const CONFIG_MEDIA_TYPE: &str = "application/vnd.docker.container.image.v1+json";
const LAYER_MEDIA_TYPE: &str = "application/vnd.docker.image.rootfs.diff.tar.gzip";

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let hex = result.iter().map(|b| format!("{:02x}", b)).collect::<String>();
    format!("sha256:{}", hex)
}

fn sample_image() -> (Vec<u8>, HashMap<String, Vec<u8>>) {
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

    let mut blobs = HashMap::new();
    blobs.insert(config_digest, config);
    blobs.insert(layer_digest, layer);

    (manifest, blobs)
}

fn sample_child_manifest(arch: &str, layer: Vec<u8>) -> (Vec<u8>, HashMap<String, Vec<u8>>) {
    let config = format!(r#"{{"architecture":"{}","config":{{}}}}"#, arch).into_bytes();
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

    let mut blobs = HashMap::new();
    blobs.insert(config_digest, config);
    blobs.insert(layer_digest, layer);

    (manifest, blobs)
}

const INDEX_MEDIA_TYPE: &str = "application/vnd.docker.distribution.manifest.list.v2+json";

#[tokio::test]
async fn test_digest_skip_when_manifests_match() {
    let src = MockRegistry::start().await;
    let dest = MockRegistry::start().await;

    let (manifest, blobs) = sample_image();
    src.add_image("library/nginx", "1.25", MANIFEST_MEDIA_TYPE, manifest.clone(), blobs.clone());
    dest.add_image("library/nginx", "1.25", MANIFEST_MEDIA_TYPE, manifest, blobs);

    let source = RegistryClient::new(&src.base_url(), "library/nginx", "1.25", true).unwrap();
    let target = RegistryClient::new(&dest.base_url(), "library/nginx", "1.25", true).unwrap();

    let src_digest = source.digest(&None).await.unwrap();
    let dest_digest = target.digest(&None).await.unwrap();

    assert!(src_digest.is_some());
    assert_eq!(src_digest, dest_digest);
}

#[tokio::test]
async fn test_copy_single_arch_image() {
    let src = MockRegistry::start().await;
    let dest = MockRegistry::start().await;

    let (manifest, blobs) = sample_image();
    src.add_image("library/nginx", "1.25", MANIFEST_MEDIA_TYPE, manifest.clone(), blobs);

    let source = RegistryClient::new(&src.base_url(), "library/nginx", "1.25", true).unwrap();
    let target = RegistryClient::new(&dest.base_url(), "library/nginx", "1.25", true).unwrap();

    let src_digest = source.digest(&None).await.unwrap();
    assert!(src_digest.is_some());
    assert!(target.digest(&None).await.unwrap().is_none());

    target.copy_from(&source, &None).await.unwrap();

    let dest_digest = target.digest(&None).await.unwrap();
    assert_eq!(src_digest, dest_digest);

    let (stored_digest, stored_manifest) = dest.manifest("library/nginx", "1.25").unwrap();
    assert_eq!(stored_digest, src_digest.unwrap());
    assert_eq!(stored_manifest, manifest);
}

#[tokio::test]
async fn test_copy_multi_arch_image_index() {
    let src = MockRegistry::start().await;
    let dest = MockRegistry::start().await;

    let (amd64_manifest, amd64_blobs) = sample_child_manifest("amd64", b"amd64-layer".to_vec());
    let (arm64_manifest, arm64_blobs) = sample_child_manifest("arm64", b"arm64-layer".to_vec());

    let amd64_digest = sha256_hex(&amd64_manifest);
    let arm64_digest = sha256_hex(&arm64_manifest);

    let index = format!(
        r#"{{
  "schemaVersion": 2,
  "mediaType": "{}",
  "manifests": [
    {{
      "mediaType": "{}",
      "size": {},
      "digest": "{}",
      "platform": {{"architecture": "amd64", "os": "linux"}}
    }},
    {{
      "mediaType": "{}",
      "size": {},
      "digest": "{}",
      "platform": {{"architecture": "arm64", "os": "linux"}}
    }}
  ]
}}"#,
        INDEX_MEDIA_TYPE,
        MANIFEST_MEDIA_TYPE,
        amd64_manifest.len(),
        amd64_digest,
        MANIFEST_MEDIA_TYPE,
        arm64_manifest.len(),
        arm64_digest
    )
    .into_bytes();

    let mut all_blobs = HashMap::new();
    all_blobs.extend(amd64_blobs);
    all_blobs.extend(arm64_blobs);

    src.add_image("library/nginx", "1.25", INDEX_MEDIA_TYPE, index.clone(), all_blobs);
    src.add_image(
        "library/nginx",
        &amd64_digest,
        MANIFEST_MEDIA_TYPE,
        amd64_manifest.clone(),
        HashMap::new(),
    );
    src.add_image(
        "library/nginx",
        &arm64_digest,
        MANIFEST_MEDIA_TYPE,
        arm64_manifest.clone(),
        HashMap::new(),
    );

    let source = RegistryClient::new(&src.base_url(), "library/nginx", "1.25", true).unwrap();
    let target = RegistryClient::new(&dest.base_url(), "library/nginx", "1.25", true).unwrap();

    let src_digest = source.digest(&None).await.unwrap();
    assert!(target.digest(&None).await.unwrap().is_none());

    target.copy_from(&source, &None).await.unwrap();

    let dest_digest = target.digest(&None).await.unwrap();
    assert_eq!(src_digest, dest_digest);

    let (_, stored_index) = dest.manifest("library/nginx", "1.25").unwrap();
    assert_eq!(stored_index, index);

    let (_, stored_amd64) = dest.manifest("library/nginx", &amd64_digest).unwrap();
    assert_eq!(stored_amd64, amd64_manifest);

    let (_, stored_arm64) = dest.manifest("library/nginx", &arm64_digest).unwrap();
    assert_eq!(stored_arm64, arm64_manifest);
}
