use crate::error::BambooError;

/// 把内部错误转换成给终端用户看的中文提示。
///
/// `--verbose` 模式下应直接打印原始错误，不走本函数。
pub fn format(err: &BambooError) -> String {
    match err {
        BambooError::ImageParse(msg) => format!(
            "无法解析镜像引用：{}\n请检查输入格式，例如 nginx:1.25、redis:7 或 quay.io/coreos/etcd:v3.5",
            msg
        ),
        BambooError::Auth(msg) => format!(
            "认证失败：{}\n请检查 --dest-creds/--source-creds、authfile 配置或对应的环境变量（BAMBOO_DEST_CREDS / BAMBOO_SOURCE_CREDS / BAMBOO_AUTHFILE）。",
            msg
        ),
        BambooError::Config(msg) => format!(
            "配置错误：{}\n请检查 TOML 文件格式、字段名以及配置文件路径。",
            msg
        ),
        BambooError::Io(e) => format!(
            "IO 错误：{}\n请检查文件路径、磁盘空间和权限。",
            e
        ),
        BambooError::Registry(msg) | BambooError::Sync(msg) => translate_registry_or_sync(msg),
    }
}

fn translate_registry_or_sync(msg: &str) -> String {
    // 先保留最外层可能已经带上的镜像上下文。
    let prefix = extract_image_prefix(msg);
    let body = msg.strip_prefix(&prefix).unwrap_or(msg).trim_start();

    let translated = if body.contains("同步超时") || body.contains("timed out") {
        format!(
            "同步超时：{}\n请检查网络质量，或增大 --timeout / 配置文件中的 timeout。",
            body
        )
    } else if body.contains("没有匹配平台") || body.contains("平台格式错误") {
        format!(
            "平台过滤失败：{}\n请检查 --platform / platform 配置格式是否为 linux/amd64 或 linux/arm64/v8。",
            body
        )
    } else if body.contains("tls")
        || body.contains("TLS")
        || body.contains("certificate")
        || body.contains("Certificate")
    {
        format!(
            "TLS 证书校验失败：{}\n如果 Registry 使用自签名证书，可在配置中设置 skip_tls_verify_dest = true 或 insecure_dest = true。",
            body
        )
    } else if body.contains(" ManifestUnknown")
        || body.contains("manifest 不存在")
        || body.contains("manifest unknown")
    {
        format!(
            "镜像或标签不存在：{}\n请确认源 Registry 中存在该镜像和标签。",
            body
        )
    } else if body.contains("error sending request")
        || body.contains("error trying to connect")
        || body.contains("dns error")
        || body.contains("Connection refused")
    {
        format!(
            "无法连接到 Registry：{}\n请检查 source_registry / dest_registry 地址、网络连通性，以及 insecure / skip_tls_verify 配置。",
            body
        )
    } else {
        format!(
            "同步失败：{}\n如需要原始错误信息排查，可加上 --verbose 重试。",
            body
        )
    };

    if prefix.is_empty() {
        translated
    } else {
        format!("{} {}", prefix.trim(), translated)
    }
}

/// 如果错误字符串以 `[image:tag] ...` 开头，提取并保留该前缀。
fn extract_image_prefix(msg: &str) -> String {
    if !msg.starts_with('[') {
        return String::new();
    }
    if let Some(end) = msg.find("] ") {
        format!("{}]", &msg[..=end])
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::BambooError;

    #[test]
    fn translates_image_parse() {
        let err = BambooError::ImageParse("empty image".to_string());
        let s = format(&err);
        assert!(s.contains("无法解析镜像引用"));
        assert!(s.contains("nginx:1.25"));
    }

    #[test]
    fn keeps_image_prefix_for_sync_errors() {
        let err =
            BambooError::Sync("[redis:8] 同步失败: error sending request for url".to_string());
        let s = format(&err);
        assert!(s.contains("[redis:8]"));
        assert!(s.contains("无法连接到 Registry"));
    }

    #[test]
    fn translates_tls_error() {
        let err =
            BambooError::Registry("error sending request: certificate verify failed".to_string());
        let s = format(&err);
        assert!(s.contains("TLS 证书校验失败"));
        assert!(s.contains("skip_tls_verify_dest"));
    }

    #[test]
    fn translates_manifest_unknown() {
        let err = BambooError::Registry(
            "拉取 manifest 失败: Registry error: ..., envelope: ... ManifestUnknown ..."
                .to_string(),
        );
        let s = format(&err);
        assert!(s.contains("镜像或标签不存在"));
    }
}
