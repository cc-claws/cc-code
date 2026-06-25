/// 将绝对路径剥离 cwd 前缀，返回相对路径；失败则取末段文件名
fn strip_cwd(path: &str, cwd: Option<&str>) -> String {
    if let Some(cwd) = cwd {
        let base = if cwd.ends_with('/') {
            cwd.to_string()
        } else {
            format!("{}/", cwd)
        };
        if let Some(rel) = path.strip_prefix(&base) {
            return rel.to_string();
        }
    }
    // fallback：取最后一段文件名
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// 返回简短 display name，控制在 3-6 字符以保持 UI 对齐
pub fn format_tool_name(tool: &str) -> String {
    match tool {
        "Bash" => "Bash",
        "Read" => "Read",
        "Write" => "Write",
        "Edit" => "Edit",
        "Glob" => "Glob",
        "Grep" => "Grep",
        "FolderOperations" => "Folder",
        "TodoWrite" => "Todo",
        "AskUserQuestion" => "Ask",
        "Agent" => "Agent",
        "LSP" => "LSP",
        other => return to_pascal(other),
    }
    .to_string()
}

/// 返回参数摘要（含路径缩短逻辑）
pub fn format_tool_args(
    tool: &str,
    input: &serde_json::Value,
    cwd: Option<&str>,
) -> Option<String> {
    match tool {
        "Bash" => input["command"].as_str().map(|s| truncate(s, 400)),
        "Read" | "Write" | "Edit" => {
            let path = input["file_path"]
                .as_str()
                .or_else(|| input["path"].as_str());
            path.map(|p| strip_cwd(p, cwd))
        }
        "Glob" => input["pattern"].as_str().map(|p| truncate(p, 200)),
        "Grep" => input["pattern"].as_str().map(|s| truncate(s, 200)),
        "FolderOperations" => {
            let op = input["operation"].as_str().unwrap_or("?");
            let path = input["folder_path"].as_str().unwrap_or("?");
            Some(format!("{} {}", op, strip_cwd(path, cwd)))
        }
        "WebSearch" => input["query"].as_str().map(|s| truncate(s, 60)),
        "WebFetch" => input["url"].as_str().map(|s| truncate(s, 60)),
        "ExecuteExtraTool" => input["tool_name"].as_str().map(|s| truncate(s, 40)),
        "SearchExtraTools" => input["query"].as_str().map(|s| truncate(s, 40)),
        "LSP" => input["operation"].as_str().map(|s| truncate(s, 40)),
        _ => None,
    }
}

pub fn to_pascal(s: &str) -> String {
    s.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("tool_display_test.rs");
}
