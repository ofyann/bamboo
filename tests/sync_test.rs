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
