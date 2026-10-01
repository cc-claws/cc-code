//! 规则来源采集。
//!
//! **不假设用户把规则写在某个固定文件里。** 不同的人、不同的项目，规则散落在不同地方：
//!
//! - `CLAUDE.md` / `AGENTS.md` 的知识库段落（业务规则、数据安全红线）
//! - `.claude/hooks/*.sh` 里的 guard 条件与 `echo "... 原因：..."` 消息（流程规则）
//! - `~/.claude/CLAUDE.md`（跨项目的个人偏好）
//!
//! hook 脚本尤其重要：很多人的"铁律"根本没写进 CLAUDE.md，而是直接写成了可执行脚本。
//! 那些脚本里的 `grep -qE '...'` + `echo "[BLOCKED] ... 原因：..."` 本身就是
//! 自然语言规则，采集进来提炼器才看得见。
//!
//! 采集**只读文件、不调用模型**；提炼是惰性的（见 [`super::rules::JevRuleLoader`]）。

use std::path::Path;

/// 单个来源的字符上限，防止某个文件吃掉整个预算。
const MAX_CHARS_PER_FILE: usize = 12_000;
/// 最多采集多少个 hook 脚本。
const MAX_HOOK_FILES: usize = 8;

/// 采集 `.claude/hooks/` 下的脚本内容（按文件名排序）。
///
/// 返回 `None` = 目录不存在或没有可用脚本。
pub fn collect_hook_rules(cwd: &str) -> Option<String> {
    let dir = Path::new(cwd).join(".claude").join("hooks");
    if !dir.is_dir() {
        return None;
    }

    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .map(|x| x.eq_ignore_ascii_case("sh"))
                    .unwrap_or(false)
        })
        .collect();
    files.sort();

    let mut out = String::new();
    for path in files.into_iter().take(MAX_HOOK_FILES) {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        if content.trim().is_empty() {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let body: String = content.chars().take(MAX_CHARS_PER_FILE).collect();
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str("### hook: ");
        out.push_str(&name);
        out.push('\n');
        out.push_str(&body);
    }

    if out.trim().is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
#[path = "sources_test.rs"]
mod tests;
