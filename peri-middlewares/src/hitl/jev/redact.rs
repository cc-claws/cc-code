//! 密钥脱敏 —— 在状态离开本机之前替换明显凭据。
//!
//! 这是安全网，不是保证：非常规格式的密钥会漏过。

use std::sync::LazyLock;

use regex::Regex;

struct Rule {
    re: Regex,
    replacement: &'static str,
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let mk = |pat: &str, replacement: &'static str| Rule {
        re: Regex::new(pat).expect("redact regex"),
        replacement,
    };
    vec![
        // PEM 私钥块
        mk(
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
            "[REDACTED PRIVATE KEY]",
        ),
        // JWT
        mk(
            r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{5,}\b",
            "[REDACTED JWT]",
        ),
        // OpenAI / 兼容 sk- rk-
        mk(r"\b(?:sk|rk)-[A-Za-z0-9_-]{16,}\b", "[REDACTED KEY]"),
        // GitHub token
        mk(
            r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}\b",
            "[REDACTED TOKEN]",
        ),
        // AWS access key id
        mk(r"\b(?:AKIA|ASIA)[0-9A-Z]{12,}\b", "[REDACTED AWS KEY]"),
        // TypeSafe 风格 apikey_
        mk(r"(?i)\bapikey_[A-Za-z0-9_-]{8,}\b", "[REDACTED KEY]"),
        // Authorization: Bearer ...
        mk(r"\bBearer\s+[A-Za-z0-9._~+/-]{12,}=*", "Bearer [REDACTED]"),
        // 通用 key=value 形式
        mk(
            r#"(?i)((?:api[_-]?key|secret|token|password|passwd|access[_-]?key|client[_-]?secret|auth[_-]?token)\s*[:=]\s*)(["']?)([^\s"';|&]{6,})"#,
            "$1$2[REDACTED]",
        ),
    ]
});

/// 替换明显凭据，使其既不被发送也不被显示。
pub fn redact_secrets(text: &str) -> String {
    let mut result = text.to_string();
    for rule in RULES.iter() {
        result = rule.re.replace_all(&result, rule.replacement).into_owned();
    }
    result
}

/// 截断到最大长度（字符级，CJK 安全）。
pub fn truncate(value: &str, max_len: usize) -> String {
    if value.chars().count() > max_len {
        let mut s: String = value.chars().take(max_len).collect();
        s.push_str("...");
        s
    } else {
        value.to_string()
    }
}

#[cfg(test)]
#[path = "redact_test.rs"]
mod tests;
