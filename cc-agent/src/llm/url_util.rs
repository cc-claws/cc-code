//! URL 工具与规范化逻辑
//!
//! 提供针对 LLM provider 的 base_url 自动容错清洗、补齐协议及友好的 reqwest 错误信息转换。

/// 判断 host 是否属于本机或局域网私有地址
fn is_local_or_private_host(host_and_port: &str) -> bool {
    let host = host_and_port
        .split(':')
        .next()
        .unwrap_or(host_and_port)
        .trim();
    let host_lower = host.to_lowercase();

    if host_lower == "localhost"
        || host_lower == "127.0.0.1"
        || host_lower == "0.0.0.0"
        || host_lower == "::1"
        || host_lower == "[::1]"
    {
        return true;
    }

    if host_lower.starts_with("192.168.") || host_lower.starts_with("10.") {
        return true;
    }

    if let Some(rest) = host_lower.strip_prefix("172.") {
        if let Some(second_octet) = rest.split('.').next().and_then(|s| s.parse::<u8>().ok()) {
            if (16..=31).contains(&second_octet) {
                return true;
            }
        }
    }

    false
}

/// 规范化与修复 LLM base_url
///
/// 核心修复行为：
/// 1. 去除首尾空白与末尾多余斜杠。
/// 2. 缺失 scheme 时根据 host 自动补齐：本地/内网默认补 `http://`，公网域名补 `https://`。
/// 3. Anthropic：
///    - 适配器内部拼接固定为 `{base_url}/v1/messages`。
///    - 若用户填写了 `/v1/messages` 或 `/messages`，自动剥除。
///    - 若用户填写了末尾 `/v1`（如 `http://127.0.0.1:20128/v1`），自动剥除末尾 `/v1`，防止重复拼成 `/v1/v1/messages`。
/// 4. OpenAI 及其兼容层：
///    - 适配器内部拼接固定为 `{base_url}/chat/completions`。
///    - 若用户填写了末尾 `/chat/completions`，自动剥除。
///    - 若用户仅填写了根域名且未指定路径（如 `http://127.0.0.1:20128`），自动追加 `/v1`。
pub fn normalize_base_url(raw: &str, provider_type: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // 1. 确保带有 http:// 或 https:// 协议头
    let with_scheme = if trimmed.to_lowercase().starts_with("http://")
        || trimmed.to_lowercase().starts_with("https://")
    {
        trimmed.to_string()
    } else {
        let default_scheme = if is_local_or_private_host(trimmed) {
            "http://"
        } else {
            "https://"
        };
        format!("{}{}", default_scheme, trimmed)
    };

    let mut clean = with_scheme.trim_end_matches('/').to_string();

    let ptype = provider_type.trim().to_lowercase();
    if ptype == "anthropic" {
        // 针对 Anthropic 剥离重复路径
        if let Some(stripped) = clean.strip_suffix("/v1/messages") {
            clean = stripped.trim_end_matches('/').to_string();
        } else if let Some(stripped) = clean.strip_suffix("/messages") {
            clean = stripped.trim_end_matches('/').to_string();
        }

        if let Some(stripped) = clean.strip_suffix("/v1") {
            clean = stripped.trim_end_matches('/').to_string();
        }
    } else {
        // OpenAI 及兼容层处理
        if let Some(stripped) = clean.strip_suffix("/chat/completions") {
            clean = stripped.trim_end_matches('/').to_string();
        }

        // 如果只有协议和 host，没有 path，OpenAI 标准 endpoint 需要 /v1
        if let Ok(parsed) = reqwest::Url::parse(&clean) {
            let path = parsed.path().trim_matches('/');
            if path.is_empty() {
                clean = format!("{}/v1", clean.trim_end_matches('/'));
            }
        }
    }

    clean
}

/// 将 reqwest 错误转换为面向终端用户友好、定位明确的错误描述
pub fn format_reqwest_error(e: &reqwest::Error, url: &str) -> String {
    if e.is_builder() {
        format!("LLM HTTP 请求构建失败 (URL: {url}): {e}（请检查 baseUrl 配置格式是否有效）")
    } else if e.is_connect() {
        format!("LLM 连接失败 (URL: {url}): {e}（请检查网络连接、服务端口或代理设置）")
    } else if e.is_timeout() {
        format!("LLM 请求超时 (URL: {url}): {e}")
    } else {
        format!("LLM 网络请求失败 (URL: {url}): {e}")
    }
}
