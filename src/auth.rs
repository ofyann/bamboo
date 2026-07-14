use crate::error::{BambooError, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
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

pub fn resolve_auth(creds: Option<&str>, authfile: &str, registry: &str) -> Result<Option<Auth>> {
    // 1. --creds takes precedence
    if let Some(creds) = creds {
        return Some(parse_creds(creds)).transpose();
    }

    // 2. Try docker config authfile
    let expanded = shellexpand::tilde(authfile).to_string();
    let path = PathBuf::from(expanded);
    if path.exists() {
        if let Some(auth) = read_docker_config(&path, registry)? {
            return Ok(Some(auth));
        }
    }

    Ok(None)
}

fn parse_creds(creds: &str) -> Result<Auth> {
    let (user, pass) = creds
        .split_once(':')
        .ok_or_else(|| BambooError::Auth("credentials must be in user:pass format".to_string()))?;
    Ok(Auth {
        username: user.to_string(),
        password: pass.to_string(),
    })
}

fn read_docker_config(path: &std::path::Path, registry: &str) -> Result<Option<Auth>> {
    let contents = fs::read_to_string(path)?;
    let config: DockerConfig = serde_json::from_str(&contents)
        .map_err(|e| BambooError::Auth(format!("invalid docker config: {e}")))?;

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
                .map_err(|e| BambooError::Auth(format!("base64 decode failed: {e}")))?,
        )
        .map_err(|e| BambooError::Auth(format!("invalid utf8 in auth: {e}")))?;

        let (user, pass) = decoded.split_once(':').unwrap_or((&decoded, ""));
        return Ok(Some(Auth {
            username: user.to_string(),
            password: pass.to_string(),
        }));
    }

    Ok(None)
}
