pub mod edit;
pub mod glob;
pub mod grep;
pub(crate) mod grep_args;
pub(crate) mod grep_format;
pub mod read;
pub(crate) mod rg_engine;
pub mod write;

pub use edit::EditFileTool;
pub use glob::GlobFilesTool;
pub use grep::GrepTool;
pub use read::ReadFileTool;
pub use write::WriteFileTool;

use std::path::{Path, PathBuf};

/// 统一路径解析：相对路径基于 cwd，绝对路径直接使用。
///
/// 通过 `canonicalize` 解析 `..`、`.`、symlinks，使 HITL 审批时显示真实路径，
/// 防止路径遍历攻击被用户忽略。对于尚不存在的文件，规范化父目录 + 文件名。
pub fn resolve_path(cwd: &str, file_path: &str) -> PathBuf {
    // 自动安全展开波浪号家目录（~ 或 ~/ 或 ~\）
    let expanded = if file_path == "~" {
        dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."))
    } else if let Some(rest) = file_path
        .strip_prefix("~/")
        .or_else(|| file_path.strip_prefix("~\\"))
    {
        dirs_next::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(rest)
    } else {
        PathBuf::from(file_path)
    };

    let raw = if expanded.is_absolute() {
        expanded
    } else {
        Path::new(cwd).join(expanded)
    };
    // 路径已存在：完整 canonicalize（解析 symlinks + ..）
    if raw.exists() {
        return raw.canonicalize().unwrap_or(raw);
    }
    // 路径不存在（新文件）：canonicalize 父目录 + 保留文件名
    if let (Some(parent), Some(file_name)) = (raw.parent(), raw.file_name()) {
        if let Ok(canon_parent) = parent.canonicalize() {
            return canon_parent.join(file_name);
        }
    }
    raw
}

/// 生成用于 UI 显示的路径：剥离 Windows `canonicalize` 产生的 `\\?\` verbatim 前缀，
/// 再尽量转为相对 cwd 的路径（对齐 CC 的相对路径展示）。
pub fn display_rel_path(cwd: &str, path: &Path) -> String {
    let raw = path.to_string_lossy();
    let clean: String = if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        raw.to_string()
    };
    let cwd_clean = cwd.strip_prefix(r"\\?\").unwrap_or(cwd);
    let clean_path = Path::new(&clean);
    clean_path
        .strip_prefix(cwd_clean)
        .unwrap_or(clean_path)
        .display()
        .to_string()
}

/// 将输入字符串解析为 JSON Value，失败时原样返回为字符串
pub async fn parse_json_input(input: &str) -> serde_json::Value {
    serde_json::from_str(input).unwrap_or(serde_json::Value::String(input.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    include!("mod_test.rs");
}
