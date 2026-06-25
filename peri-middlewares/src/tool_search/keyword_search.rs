//! 关键词搜索逻辑 — CamelCase 分词、MCP 前缀拆解、查询解析、关键词评分

/// CamelCase 分词
///
/// `CronCreate` → `["cron", "create"]`
/// `SearchExtraTools` → `["search", "extra", "tools"]`
pub fn split_camel_case(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for ch in name.chars() {
        if ch.is_uppercase() {
            if !current.is_empty() {
                words.push(current.to_lowercase());
            }
            current = ch.to_string();
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        words.push(current.to_lowercase());
    }

    words
}

/// MCP 前缀拆解
///
/// `mcp__slack__send_message` → `["slack", "send_message"]`
/// `mcp__read_resource` → `["read_resource"]`
/// `Read` → `["read"]` (非 MCP 工具，按原样返回)
pub fn split_mcp_prefix(name: &str) -> Vec<String> {
    if !name.starts_with("mcp__") {
        return split_camel_case(name);
    }

    // mcp__server_name__tool_name → 跳过 "mcp"，取 server_name 和 tool_name
    let parts: Vec<&str> = name.split("__").collect();
    if parts.len() >= 3 {
        // parts[0] = "mcp", parts[1] = server_name, parts[2..] = tool_name parts
        let mut result = vec![parts[1].to_lowercase()];
        result.push(parts[2..].join("_").to_lowercase());
        result
    } else if parts.len() == 2 {
        // mcp__tool_name (无 server name)
        vec![parts[1].to_lowercase()]
    } else {
        split_camel_case(name)
    }
}

/// 解析查询词，返回 `(required_words, optional_words)`
///
/// `+` 前缀词归入 required，其余归入 optional
/// `"+slack message"` → `(["slack"], ["message"])`
pub fn parse_query(query: &str) -> (Vec<String>, Vec<String>) {
    let mut required = Vec::new();
    let mut optional = Vec::new();

    for token in query.split_whitespace() {
        if let Some(word) = token.strip_prefix('+') {
            if !word.is_empty() {
                required.push(word.to_lowercase());
            }
        } else if !token.is_empty() {
            optional.push(token.to_lowercase());
        }
    }

    (required, optional)
}

/// 计算关键词分数
///
/// - 必选词缺失 → 0.0
/// - 必选词全部匹配 → 基础分 1.0
/// - 可选词匹配 → 每个加 0.3
/// - 工具名精确匹配加 0.5
/// - 描述精确匹配加 0.2
pub fn keyword_score(
    tool_name: &str,
    tool_desc: &str,
    required: &[String],
    optional: &[String],
) -> f64 {
    let name_lower = tool_name.to_lowercase();
    let desc_lower = tool_desc.to_lowercase();

    // 提取工具名的所有分词
    let name_words: Vec<String> = split_mcp_prefix(tool_name)
        .into_iter()
        .chain(split_camel_case(tool_name))
        .collect();
    let desc_words: Vec<String> = desc_lower.split_whitespace().map(String::from).collect();
    let all_words: Vec<&String> = name_words.iter().chain(desc_words.iter()).collect();

    /// 检查两个词是否匹配（子串匹配，但要求匹配长度 >= 2 或完全相等）
    fn words_match(a: &str, b: &str) -> bool {
        a == b || (a.len() >= 2 && b.len() >= 2 && (a.contains(b) || b.contains(a)))
    }

    // 必选词检查
    for req in required {
        let found = all_words.iter().any(|w| words_match(req, w));
        if !found {
            return 0.0;
        }
    }

    let mut score = 1.0;

    // 可选词匹配
    for opt in optional {
        let found = all_words.iter().any(|w| words_match(opt, w));
        if found {
            score += 0.3;
        }
    }

    // 工具名精确匹配
    for opt in optional.iter().chain(required.iter()) {
        if name_lower == *opt || name_words.contains(opt) {
            score += 0.5;
            break;
        }
    }

    // 描述精确匹配
    for opt in optional.iter().chain(required.iter()) {
        if desc_lower.contains(opt.as_str()) && opt.len() >= 3 {
            score += 0.2;
            break;
        }
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("keyword_search_test.rs");
}
