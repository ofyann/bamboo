use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get},
    Router,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

#[derive(Clone)]
struct ManifestEntry {
    content_type: String,
    body: Vec<u8>,
    digest: String,
}

#[derive(Default)]
struct RegistryState {
    manifests: HashMap<(String, String), ManifestEntry>,
    blobs: HashMap<String, Vec<u8>>,
    uploads: HashMap<String, Vec<u8>>,
}

pub struct MockRegistry {
    addr: SocketAddr,
    state: Arc<Mutex<RegistryState>>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl MockRegistry {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(RegistryState::default()));
        let app = router(state.clone());

        let (tx, rx) = tokio::sync::oneshot::channel();
        let server = axum::serve(listener, app).with_graceful_shutdown(async {
            rx.await.ok();
        });
        tokio::spawn(async move { server.await });

        Self {
            addr,
            state,
            shutdown: Some(tx),
        }
    }

    pub fn base_url(&self) -> String {
        self.addr.to_string()
    }

    pub fn add_image(
        &self,
        repo: &str,
        reference: &str,
        content_type: &str,
        manifest: Vec<u8>,
        blobs: HashMap<String, Vec<u8>>,
    ) {
        let digest = sha256_hex(&manifest);
        let entry = ManifestEntry {
            content_type: content_type.to_string(),
            body: manifest,
            digest,
        };

        let mut state = self.state.lock().unwrap();
        state
            .manifests
            .insert((repo.to_string(), reference.to_string()), entry.clone());
        state
            .manifests
            .insert((repo.to_string(), entry.digest.clone()), entry);

        for (digest, bytes) in blobs {
            state.blobs.insert(digest, bytes);
        }
    }

    pub fn manifest(&self, repo: &str, reference: &str) -> Option<(String, Vec<u8>)> {
        let state = self.state.lock().unwrap();
        state
            .manifests
            .get(&(repo.to_string(), reference.to_string()))
            .map(|e| (e.digest.clone(), e.body.clone()))
    }
}

impl Drop for MockRegistry {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

fn router(state: Arc<Mutex<RegistryState>>) -> Router {
    Router::new()
        .route("/v2/", get(api_version_check))
        .route("/v2/*path", any(handle_v2))
        .with_state(state)
}

async fn api_version_check() -> StatusCode {
    StatusCode::OK
}

async fn handle_v2(
    State(state): State<Arc<Mutex<RegistryState>>>,
    method: Method,
    Path(path): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let marker = segments
        .iter()
        .position(|&s| s == "manifests" || s == "blobs");

    match marker {
        Some(idx) if segments[idx] == "manifests" && idx + 1 < segments.len() => {
            let repo = segments[..idx].join("/");
            let reference = segments[idx + 1..].join("/");
            match method {
                Method::HEAD => head_manifest(state, &repo, &reference).await,
                Method::GET => get_manifest(state, &repo, &reference).await,
                Method::PUT => put_manifest(state, &repo, &reference, headers, body).await,
                _ => oci_error(
                    StatusCode::METHOD_NOT_ALLOWED,
                    "UNSUPPORTED",
                    "method not allowed",
                ),
            }
        }
        Some(idx) if segments[idx] == "blobs" && idx + 1 < segments.len() => {
            let repo = segments[..idx].join("/");
            let next = segments[idx + 1];
            if next == "uploads" {
                if idx + 2 < segments.len() {
                    let uuid = segments[idx + 2].to_string();
                    match method {
                        Method::PATCH => push_chunk(state, &repo, &uuid, body).await,
                        Method::PUT => finish_upload(state, &repo, &uuid, params, body).await,
                        _ => oci_error(
                            StatusCode::METHOD_NOT_ALLOWED,
                            "UNSUPPORTED",
                            "method not allowed",
                        ),
                    }
                } else {
                    match method {
                        Method::POST => start_upload(state, &repo).await,
                        _ => oci_error(
                            StatusCode::METHOD_NOT_ALLOWED,
                            "UNSUPPORTED",
                            "method not allowed",
                        ),
                    }
                }
            } else {
                let digest = next.to_string();
                match method {
                    Method::HEAD => head_blob(state, &digest).await,
                    Method::GET => get_blob(state, &digest).await,
                    _ => oci_error(
                        StatusCode::METHOD_NOT_ALLOWED,
                        "UNSUPPORTED",
                        "method not allowed",
                    ),
                }
            }
        }
        _ => oci_error(
            StatusCode::NOT_FOUND,
            "NAME_UNKNOWN",
            "repository name not known",
        ),
    }
}

async fn head_manifest(state: Arc<Mutex<RegistryState>>, repo: &str, reference: &str) -> Response {
    let state = state.lock().unwrap();
    match state
        .manifests
        .get(&(repo.to_string(), reference.to_string()))
    {
        Some(entry) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                "Docker-Content-Digest",
                HeaderValue::from_str(&entry.digest).unwrap(),
            );
            headers.insert(
                "Content-Type",
                HeaderValue::from_str(&entry.content_type).unwrap(),
            );
            (StatusCode::OK, headers).into_response()
        }
        None => oci_error(
            StatusCode::NOT_FOUND,
            "MANIFEST_UNKNOWN",
            "manifest unknown",
        ),
    }
}

async fn get_manifest(state: Arc<Mutex<RegistryState>>, repo: &str, reference: &str) -> Response {
    let state = state.lock().unwrap();
    match state
        .manifests
        .get(&(repo.to_string(), reference.to_string()))
    {
        Some(entry) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                "Docker-Content-Digest",
                HeaderValue::from_str(&entry.digest).unwrap(),
            );
            headers.insert(
                "Content-Type",
                HeaderValue::from_str(&entry.content_type).unwrap(),
            );
            (StatusCode::OK, headers, entry.body.clone()).into_response()
        }
        None => oci_error(
            StatusCode::NOT_FOUND,
            "MANIFEST_UNKNOWN",
            "manifest unknown",
        ),
    }
}

async fn head_blob(state: Arc<Mutex<RegistryState>>, digest: &str) -> Response {
    let state = state.lock().unwrap();
    if state.blobs.contains_key(digest) {
        StatusCode::OK.into_response()
    } else {
        oci_error(StatusCode::NOT_FOUND, "BLOB_UNKNOWN", "blob unknown")
    }
}

async fn get_blob(state: Arc<Mutex<RegistryState>>, digest: &str) -> Response {
    let state = state.lock().unwrap();
    match state.blobs.get(digest) {
        Some(bytes) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                "Content-Type",
                HeaderValue::from_static("application/octet-stream"),
            );
            headers.insert(
                "Docker-Content-Digest",
                HeaderValue::from_str(digest).unwrap(),
            );
            (StatusCode::OK, headers, bytes.clone()).into_response()
        }
        None => oci_error(StatusCode::NOT_FOUND, "BLOB_UNKNOWN", "blob unknown"),
    }
}

async fn push_chunk(
    state: Arc<Mutex<RegistryState>>,
    repo: &str,
    uuid: &str,
    body: Bytes,
) -> Response {
    let mut state = state.lock().unwrap();
    state
        .uploads
        .entry(uuid.to_string())
        .or_default()
        .extend_from_slice(&body);

    let mut headers = HeaderMap::new();
    let location = format!("/v2/{}/blobs/uploads/{}", repo, uuid);
    headers.insert("Location", HeaderValue::from_str(&location).unwrap());

    let uploaded_len = state.uploads[uuid].len();
    headers.insert(
        "Range",
        HeaderValue::from_str(&format!("0-{}", uploaded_len.saturating_sub(1))).unwrap(),
    );
    (StatusCode::ACCEPTED, headers).into_response()
}

async fn start_upload(state: Arc<Mutex<RegistryState>>, repo: &str) -> Response {
    let uuid = format!(
        "upload-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    state
        .lock()
        .unwrap()
        .uploads
        .insert(uuid.clone(), Vec::new());

    let mut headers = HeaderMap::new();
    headers.insert(
        "Location",
        HeaderValue::from_str(&format!("/v2/{}/blobs/uploads/{}", repo, uuid)).unwrap(),
    );
    headers.insert("Range", HeaderValue::from_static("0-0"));
    (StatusCode::ACCEPTED, headers).into_response()
}

async fn finish_upload(
    state: Arc<Mutex<RegistryState>>,
    repo: &str,
    uuid: &str,
    params: HashMap<String, String>,
    body: Bytes,
) -> Response {
    let digest = match params.get("digest") {
        Some(d) => d.clone(),
        None => return oci_error(StatusCode::BAD_REQUEST, "DIGEST_INVALID", "missing digest"),
    };

    let mut state = state.lock().unwrap();
    let bytes = if body.is_empty() {
        state.uploads.remove(uuid).unwrap_or_default()
    } else {
        body.to_vec()
    };
    if bytes.is_empty() {
        return oci_error(StatusCode::BAD_REQUEST, "SIZE_INVALID", "empty blob");
    }
    state.blobs.insert(digest.clone(), bytes);
    state.uploads.remove(uuid);

    let mut headers = HeaderMap::new();
    headers.insert(
        "Location",
        HeaderValue::from_str(&format!("/v2/{}/blobs/{}", repo, digest)).unwrap(),
    );
    headers.insert(
        "Docker-Content-Digest",
        HeaderValue::from_str(&digest).unwrap(),
    );
    (StatusCode::CREATED, headers).into_response()
}

async fn put_manifest(
    state: Arc<Mutex<RegistryState>>,
    repo: &str,
    reference: &str,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let content_type = headers
        .get("Content-Type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/vnd.docker.distribution.manifest.v2+json")
        .to_string();

    let digest = sha256_hex(&body);
    let entry = ManifestEntry {
        content_type,
        body: body.to_vec(),
        digest: digest.clone(),
    };

    let mut state = state.lock().unwrap();
    state
        .manifests
        .insert((repo.to_string(), reference.to_string()), entry.clone());
    state
        .manifests
        .insert((repo.to_string(), digest.clone()), entry);

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        "Location",
        HeaderValue::from_str(&format!("/v2/{}/manifests/{}", repo, digest)).unwrap(),
    );
    resp_headers.insert(
        "Docker-Content-Digest",
        HeaderValue::from_str(&digest).unwrap(),
    );
    (StatusCode::CREATED, resp_headers).into_response()
}

fn oci_error(status: StatusCode, code: &str, message: &str) -> Response {
    let body = format!(
        r#"{{"errors":[{{"code":"{}","message":"{}"}}]}}"#,
        code, message
    );
    (
        status,
        [("Content-Type", HeaderValue::from_static("application/json"))],
        body,
    )
        .into_response()
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let hex = result
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    format!("sha256:{}", hex)
}
