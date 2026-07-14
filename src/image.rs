use std::str::FromStr;

use crate::error::{BambooError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    pub registry: String,
    pub namespace: String,
    pub name: String,
    pub tag: String,
}

impl ImageRef {
    pub fn normalize(&self) -> Self {
        let mut normalized = self.clone();
        if normalized.registry == "docker.io" && normalized.namespace.is_empty() {
            normalized.namespace = "library".to_string();
        }
        normalized
    }

    pub fn image_path(&self) -> String {
        if self.namespace.is_empty() {
            format!("{}", self.name)
        } else {
            format!("{}/{}", self.namespace, self.name)
        }
    }

    pub fn hubproxy_path(&self) -> String {
        let normalized = self.normalize();
        if normalized.registry == "docker.io" {
            normalized.image_path()
        } else {
            format!("{}/{}", normalized.registry, normalized.image_path())
        }
    }

    pub fn target_path(&self) -> String {
        self.normalize().image_path()
    }
}

impl FromStr for ImageRef {
    type Err = BambooError;

    fn from_str(s: &str) -> Result<Self> {
        let input = s.trim();
        if input.is_empty() {
            return Err(BambooError::ImageParse("empty image reference".to_string()));
        }

        // Split tag from the right. A tag cannot contain '/', so any ':' followed by '/'
        // belongs to a registry port, not a tag separator.
        let (image_part, tag) = if let Some((img, maybe_tag)) = input.rsplit_once(':') {
            if !maybe_tag.contains('/') {
                (img, maybe_tag)
            } else {
                (input, "latest")
            }
        } else {
            (input, "latest")
        };

        // Determine whether the path starts with an explicit registry.
        let (registry, path_part) = if let Some((first, rest)) = image_part.split_once('/') {
            if first.contains('.') || first.contains(':') || first == "localhost" {
                (first.to_string(), rest)
            } else {
                ("docker.io".to_string(), image_part)
            }
        } else {
            ("docker.io".to_string(), image_part)
        };

        let parts: Vec<&str> = path_part.split('/').collect();
        let name = parts.last().unwrap().to_string();
        let namespace = if parts.len() > 1 {
            parts[..parts.len() - 1].join("/")
        } else {
            String::new()
        };

        Ok(ImageRef {
            registry,
            namespace,
            name,
            tag: tag.to_string(),
        })
    }
}
