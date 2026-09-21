use peri_agent::tools::BaseTool;
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::Path;

use super::resolve_path;
use crate::tools::output_persist::persist_truncated_output;

/// Glob tool - 与 TypeScript glob_tool 对齐
pub struct GlobFilesTool {
    pub cwd: String,
}

impl GlobFilesTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

/// 最多返回的文件数，防止撑爆 LLM context window
const MAX_RESULTS: usize = 1_000;

const GLOB_FILES_DESCRIPTION: &str = r#"Fast file pattern matching tool that works with any codebase size. Supports glob patterns like "**/*.js" or "src/**/*.ts". Returns matching file paths sorted by modification time.

Usage:
- Use this tool when you need to find files by name patterns
- Returns file paths sorted by modification time (most recently modified first)
- Maximum 1000 results returned; results are truncated beyond this limit with a notice
- Common directories like node_modules, .git, target, dist, build are automatically excluded from results
- The path parameter is optional; defaults to the current working directory
- For searching file contents, use Grep instead

When to use:
- Use Glob when searching for files by name pattern (e.g., find all TypeScript files, find a specific config file)
- Use Grep when searching for content within files (e.g., find where a function is defined)
- For open-ended searches requiring multiple rounds, consider using a sub-agent via Agent"#;

fn glob_match(pattern: &str, path: &str) -> bool {
    glob::Pattern::new(pattern)
        .map(|p| p.matches(path))
        .unwrap_or(false)
}

/// 使用 ignore::WalkBuilder 收集匹配文件，原生支持 .gitignore + 隐藏文件过滤。
///
/// 使用 Top-K 最小堆就地维护最新的 MAX_RESULTS 个文件，避免全量收集后二次 metadata 调用。
/// 返回 (results, total_matched)：results 为最新的 MAX_RESULTS 个文件，total_matched 为总匹配数。
fn collect_files(base: &Path, pattern: &str) -> (Vec<String>, usize) {
    let mut builder = ignore::WalkBuilder::new(base);
    builder
        .hidden(true) // 搜索隐藏文件
        .git_ignore(true)
        .git_exclude(true)
        .ignore(true)
        .parents(true)
        .follow_links(true);

    // 排除 .claude/.worktrees 等衍生目录（不在 .gitignore 中的也需要排除）
    builder.filter_entry(|entry| {
        if entry.file_type().is_some_and(|ft| ft.is_dir()) {
            let name = entry.file_name().to_string_lossy();
            if name == ".claude" || name == ".worktrees" {
                return false;
            }
        }
        true
    });

    let walker = builder.build();

    // Top-K 最小堆：保留最新的 MAX_RESULTS 个文件
    // (Reverse(mtime), abs_path) — Reverse 使堆顶为最旧文件，方便弹出
    let mut heap: BinaryHeap<(Reverse<std::time::SystemTime>, String)> =
        BinaryHeap::with_capacity(MAX_RESULTS + 1);

    let mut total_matched: usize = 0;

    for entry in walker {
        let e = match entry {
            Ok(e) => e,
            Err(err) => {
                tracing::debug!(error = %err, "glob walk error (skipped)");
                continue;
            }
        };

        if !e.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }

        let abs_path = e.path().to_string_lossy().to_string();
        if let Ok(rel) = e.path().strip_prefix(base) {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if glob_match(pattern, &rel_str) {
                total_matched += 1;
                let mtime = e
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(std::time::UNIX_EPOCH);
                heap.push((Reverse(mtime), abs_path));
                if heap.len() > MAX_RESULTS {
                    heap.pop(); // 弹出最旧的
                }
            }
        }
    }

    // 从堆中提取并按 mtime 降序排列
    let mut results: Vec<_> = heap.into_iter().collect();
    results.sort_by(|a, b| b.0 .0.cmp(&a.0 .0));
    (results.into_iter().map(|(_, path)| path).collect(), total_matched)
}

#[async_trait::async_trait]
impl BaseTool for GlobFilesTool {
    fn name(&self) -> &str {
        "Glob"
    }

    fn description(&self) -> &str {
        GLOB_FILES_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against (e.g. \"**/*.js\", \"src/**/*.rs\", \"*.config.json\"). Use ** for recursive matching"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. Absolute path or relative to cwd. If not specified, the current working directory is used"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let pattern = input["pattern"]
            .as_str()
            .ok_or("The 'pattern' parameter is required for the Glob tool.")?;

        let search_root = if let Some(p) = input["path"].as_str() {
            resolve_path(&self.cwd, p)
        } else {
            Path::new(&self.cwd).to_path_buf()
        };

        if !search_root.exists() {
            return Err(format!("Error: Directory not found: {}", search_root.display()).into());
        }

        // 优先尝试 rg CLI 引擎
        if let Some(rg_path) = super::rg_engine::resolve_rg() {
            if let Some(output) = super::rg_engine::execute_rg_glob(
                rg_path, pattern, &search_root, &self.cwd, MAX_RESULTS,
            )
            .await
            {
                return Ok(crate::tools::output_persist::truncate_tool_output(&output));
            }
            tracing::debug!("rg glob returned None, falling back to Rust engine");
        }

        // Fallback: 纯 Rust 引擎（ignore::WalkBuilder + Top-K 堆排序）
        let (results, total_matched) = collect_files(&search_root, pattern);

        if results.is_empty() {
            Ok("No files found.".to_string())
        } else if total_matched > MAX_RESULTS {
            let persist_hint = persist_truncated_output(&results.join("\n"));
            Ok(crate::tools::output_persist::truncate_tool_output(
                &format!(
                    "{}\n\n[Output truncated: {} files total, showing first {}]{}",
                    results.join("\n"),
                    total_matched,
                    MAX_RESULTS,
                    persist_hint
                ),
            ))
        } else {
            Ok(crate::tools::output_persist::truncate_tool_output(
                &results.join("\n"),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("glob_test.rs");
}
