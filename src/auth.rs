use crate::error::{BambooError, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Auth {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
struct DockerConfig {
    #[serde(rename = "auths")]
    auths: Option<HashMap<String, DockerAuth>>,
}

#[derive(Debug, Deserialize)]
struct DockerAuth {
    auth: Option<String>,
}

pub async fn resolve_auth(
    creds: Option<&str>,
    authfile: &str,
    registry: &str,
) -> Result<Option<Auth>> {
    // 1. 显式传入的 creds 优先
    if let Some(creds) = creds {
        return Some(parse_creds(creds)).transpose();
    }

    // 2. Try docker config authfile
    let expanded = shellexpand::tilde(authfile).to_string();
    let path = PathBuf::from(expanded);
    let exists = tokio::fs::try_exists(&path)
        .await
        .map_err(|e| BambooError::Auth(format!("无法检查认证文件 {}: {}", path.display(), e)))?;
    if exists {
        if let Some(auth) = read_docker_config(&path, registry).await? {
            return Ok(Some(auth));
        }
    }

    Ok(None)
}

fn parse_creds(creds: &str) -> Result<Auth> {
    let (user, pass) = creds
        .split_once(':')
        .ok_or_else(|| BambooError::Auth("凭据格式必须为 user:pass".to_string()))?;
    Ok(Auth {
        username: user.to_string(),
        password: pass.to_string(),
    })
}

async fn read_docker_config(path: &std::path::Path, registry: &str) -> Result<Option<Auth>> {
    let contents = tokio::fs::read_to_string(path).await?;
    let config: DockerConfig = serde_json::from_str(&contents)
        .map_err(|e| BambooError::Auth(format!("Docker 配置文件格式错误: {e}")))?;

    let auths = match config.auths {
        Some(a) => a,
        None => return Ok(None),
    };

    // Try exact match first, then https:// prefix, then http:// prefix
    let entry = auths
        .get(registry)
        .or_else(|| auths.get(&format!("https://{}", registry)))
        .or_else(|| auths.get(&format!("http://{}", registry)));

    if let Some(auth_b64) = entry.and_then(|a| a.auth.as_ref()) {
        let decoded = String::from_utf8(
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, auth_b64)
                .map_err(|e| BambooError::Auth(format!("凭据 base64 解码失败: {e}")))?,
        )
        .map_err(|e| BambooError::Auth(format!("凭据包含非法 UTF-8: {e}")))?;

        let auth = parse_creds(&decoded)?;
        return Ok(Some(auth));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn base64_auth(user: &str, pass: &str) -> String {
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            format!("{}:{}", user, pass),
        )
    }

    fn write_config(contents: &str) -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bamboo-auth-test-{}-{}", counter, nanos));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        file.flush().unwrap();
        path
    }

    #[test]
    fn test_parse_creds() {
        let auth = parse_creds("admin:secret").unwrap();
        assert_eq!(auth.username, "admin");
        assert_eq!(auth.password, "secret");
    }

    #[test]
    fn test_parse_creds_missing_colon() {
        let err = parse_creds("admin").unwrap_err();
        assert!(err.to_string().contains("user:pass"));
    }

    #[tokio::test]
    async fn test_resolve_auth_creds_takes_precedence() {
        let auth = resolve_auth(Some("user:pass"), "/nonexistent", "registry.example.com")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(auth.username, "user");
        assert_eq!(auth.password, "pass");
    }

    #[tokio::test]
    async fn test_read_docker_config_exact_match() {
        let path = write_config(&format!(
            r#"{{"auths": {{"registry.example.com": {{"auth": "{}"}}}}}}"#,
            base64_auth("docker", "hub")
        ));
        let auth = read_docker_config(&path, "registry.example.com")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(auth.username, "docker");
        assert_eq!(auth.password, "hub");
    }

    #[tokio::test]
    async fn test_read_docker_config_https_prefix() {
        let path = write_config(&format!(
            r#"{{"auths": {{"https://registry.example.com": {{"auth": "{}"}}}}}}"#,
            base64_auth("user", "pass")
        ));
        let auth = read_docker_config(&path, "registry.example.com")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(auth.username, "user");
        assert_eq!(auth.password, "pass");
    }

    #[tokio::test]
    async fn test_read_docker_config_no_match() {
        let path = write_config(r#"{"auths": {"other.registry.com": {"auth": "abc"}}}"#);
        let auth = read_docker_config(&path, "registry.example.com")
            .await
            .unwrap();
        assert!(auth.is_none());
    }

    #[tokio::test]
    async fn test_read_docker_config_missing_auths() {
        let path = write_config(r#"{}"#);
        let auth = read_docker_config(&path, "registry.example.com")
            .await
            .unwrap();
        assert!(auth.is_none());
    }

    #[tokio::test]
    async fn test_read_docker_config_malformed_auth() {
        let token = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            "not_a_valid_credential",
        );
        let path = write_config(&format!(
            r#"{{"auths": {{"registry.example.com": {{"auth": "{}"}}}}}}"#,
            token
        ));
        let result = read_docker_config(&path, "registry.example.com").await;
        assert!(result.is_err());
    }
}
