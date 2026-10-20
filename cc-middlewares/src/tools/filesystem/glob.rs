use cc_agent::tools::BaseTool;
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::Path;
use std::time::{Duration, Instant};

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

/// collect_files 最大遍历条目数（含目录），超过即中止返回已收集结果
const MAX_WALK_ENTRIES: usize = 500_000;

/// collect_files 最大遍历时间
const MAX_WALK_DURATION: Duration = Duration::from_secs(10);

/// Glob fallback 遍历的整体超时上限
const GLOB_TIMEOUT_SECS: u64 = 30;

/// 判断是否为 Linux 虚拟文件系统根目录（/proc、/sys、/dev、/run），
/// 这些目录含有大量符号链接和伪文件，遍历会导致指数级膨胀。
#[cfg(target_os = "linux")]
fn is_virtual_fs_root(path: &Path) -> bool {
    matches!(
        path.to_str(),
        Some("/proc") | Some("/sys") | Some("/dev") | Some("/run")
    )
}

#[cfg(not(target_os = "linux"))]
fn is_virtual_fs_root(_path: &Path) -> bool {
    false
}

const GLOB_FILES_DESCRIPTION: &str = r#"Find local files by name using glob patterns such as "**/*.js" or "src/**/*.rs".

Usage:
- Provide pattern; path defaults to the working directory and may be absolute or relative to it.
- Results are sorted by modification time, newest first, with at most 1000 paths returned.
- Search honors applicable ignore rules; hidden and generated directories may be excluded. A missing result does not establish that a file is absent.
- Narrow broad patterns when results are truncated. Use Grep to search file contents."#;

fn glob_match(pattern: &str, path: &str) -> bool {
    glob::Pattern::new(pattern)
        .map(|p| p.matches(path))
        .unwrap_or(false)
}

/// 使用 ignore::WalkBuilder 收集匹配文件，原生支持 .gitignore + 隐藏文件过滤。
///
/// 使用 Top-K 最小堆就地维护最新的 MAX_RESULTS 个文件，避免全量收集后二次 metadata 调用。
/// 返回 (results, total_matched, truncated)：results 为最新的 MAX_RESULTS 个文件，
/// total_matched 为总匹配数，truncated 为是否因安全上限提前中止。
fn collect_files(base: &Path, pattern: &str) -> (Vec<String>, usize, bool) {
    let mut builder = ignore::WalkBuilder::new(base);
    builder
        .hidden(true) // 搜索隐藏文件
        .git_ignore(true)
        .git_exclude(true)
        .ignore(true)
        .parents(true)
        .follow_links(false); // 禁止跟随符号链接，防止 /proc、/sys 等虚拟文件系统引发指数级膨胀

    // 排除 .claude/.worktrees 等衍生目录 + 虚拟文件系统根目录
    builder.filter_entry(|entry| {
        if entry.file_type().is_some_and(|ft| ft.is_dir()) {
            let name = entry.file_name().to_string_lossy();
            if name == ".claude" || name == ".worktrees" {
                return false;
            }
            // 跳过 Linux 虚拟文件系统根目录
            if is_virtual_fs_root(entry.path()) {
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
    let mut entries_visited: usize = 0;
    let mut truncated = false;
    let start = Instant::now();

    for entry in walker {
        // 安全上限：条目数或时间超限时中止，防止无限遍历
        entries_visited += 1;
        if entries_visited > MAX_WALK_ENTRIES || start.elapsed() > MAX_WALK_DURATION {
            tracing::warn!(
                entries_visited,
                elapsed_ms = start.elapsed().as_millis() as u64,
                "glob walk truncated: safety limit reached"
            );
            truncated = true;
            break;
        }

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
    results.sort_unstable_by_key(|b| Reverse(b.0 .0));
    (
        results.into_iter().map(|(_, path)| path).collect(),
        total_matched,
        truncated,
    )
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
            match super::rg_engine::execute_rg_glob(
                rg_path,
                pattern,
                &search_root,
                &self.cwd,
                MAX_RESULTS,
            )
            .await
            {
                Some(output) => {
                    return Ok(crate::tools::output_persist::truncate_tool_output(&output));
                }
                None => {
                    // rg 失败/超时时记录日志，继续走 Rust fallback（带超时保护）
                    tracing::debug!("rg glob returned None, falling back to Rust engine");
                }
            }
        }

        // Fallback: 纯 Rust 引擎（ignore::WalkBuilder + Top-K 堆排序）
        // 移到 spawn_blocking 避免阻塞 tokio 工作线程，外层加超时兜底
        let pattern_owned = pattern.to_string();
        let search_root_owned = search_root.clone();

        let result = tokio::time::timeout(
            Duration::from_secs(GLOB_TIMEOUT_SECS),
            tokio::task::spawn_blocking(move || {
                collect_files(&search_root_owned, &pattern_owned)
            }),
        )
        .await;

        let (results, total_matched, truncated) = match result {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                return Err(format!("Glob task failed: {e}").into());
            }
            Err(_) => {
                return Err(format!(
                    "Glob timed out after {}s searching '{}' in '{}'.\n\
                     Hint: narrow the search pattern or specify a more specific path.",
                    GLOB_TIMEOUT_SECS,
                    pattern,
                    search_root.display()
                )
                .into());
            }
        };

        if results.is_empty() {
            Ok("No files found.".to_string())
        } else {
            let mut output = results.join("\n");
            if total_matched > MAX_RESULTS {
                let persist_hint = persist_truncated_output(&output);
                output = format!(
                    "{}\n\n[Output truncated: {} files total, showing first {}]{}",
                    output, total_matched, MAX_RESULTS, persist_hint
                );
            }
            if truncated {
                output.push_str(
                    "\n\n[Warning: search was truncated due to safety limits. \
                     Narrow your search pattern or specify a more specific path.]",
                );
            }
            Ok(crate::tools::output_persist::truncate_tool_output(&output))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("glob_test.rs");
}
