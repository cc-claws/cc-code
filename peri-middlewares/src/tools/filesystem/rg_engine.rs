//! ripgrep (rg) CLI 执行引擎 — 优先使用 rg 二进制，Fallback 到纯 Rust 库实现。
//!
//! `RgResolver` 探测 rg 可用性并缓存结果。`execute_rg_grep` / `execute_rg_glob`
//! 将工具参数映射为 rg 命令行参数并执行，输出格式与 Rust 引擎对齐。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tokio::process::Command;
use tokio::time::{timeout, Duration};

use super::grep_args::{OutputMode, ParsedArgs};

/// rg 命令超时时间（与 Rust 引擎一致）
const RG_TIMEOUT_SECS: u64 = 15;

/// rg 探测结果缓存（进程级，只探测一次）
static RG_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// 探测 rg 二进制的可用路径。
///
/// 优先级：`PERI_RG_PATH` 环境变量 → exe 同级 `bin/rg(.exe)` → 系统 `PATH`。
/// 结果缓存到 `OnceLock`，进程生命周期内只探测一次。
pub fn resolve_rg() -> Option<&'static PathBuf> {
    RG_PATH
        .get_or_init(|| {
            // 1. 环境变量
            if let Ok(p) = std::env::var("PERI_RG_PATH") {
                let path = PathBuf::from(&p);
                if path.exists() {
                    tracing::debug!(path = %p, "rg resolved from PERI_RG_PATH");
                    return Some(path);
                }
            }

            // 2. exe 同级 bin/ 目录
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    for name in ["rg", "rg.exe"] {
                        let candidate = dir.join(name);
                        if candidate.exists() {
                            tracing::debug!(path = %candidate.display(), "rg resolved from exe dir");
                            return Some(candidate);
                        }
                    }
                    // bin/ 子目录
                    let bin_dir = dir.join("bin");
                    for name in ["rg", "rg.exe"] {
                        let candidate = bin_dir.join(name);
                        if candidate.exists() {
                            tracing::debug!(path = %candidate.display(), "rg resolved from exe bin/");
                            return Some(candidate);
                        }
                    }
                }
            }

            // 3. 系统 PATH：直接执行 rg --version 探测，避免依赖 which/where
            //    （极简容器如 Alpine/Debian-slim 可能无 which 命令）
            if let Ok(output) = std::process::Command::new("rg")
                .arg("--version")
                .output()
            {
                if output.status.success() {
                    tracing::debug!("rg resolved from PATH (via --version probe)");
                    // 返回 "rg" 让 OS 在 PATH 中查找，无需绝对路径
                    return Some(PathBuf::from("rg"));
                }
            }

            tracing::debug!("rg not found, will use fallback Rust engine");
            None
        })
        .as_ref()
}

/// 将 ParsedArgs 映射为 rg 命令行参数
fn build_grep_args(parsed: &ParsedArgs, search_path: &Path, _cwd: &str) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    // 基础参数
    args.push("--no-heading".to_string());
    // 强制始终输出文件名，防止单文件搜索时 rg 省略文件名导致行号被误认为路径
    args.push("-H".to_string());
    if parsed.line_number {
        args.push("--line-number".to_string());
    } else {
        args.push("--no-line-number".to_string());
    }
    args.push("--hidden".to_string());
    args.push("--max-columns".to_string());
    args.push("500".to_string());

    // worktree 排除
    args.push("-g".to_string());
    args.push("!.claude".to_string());
    args.push("-g".to_string());
    args.push("!.worktrees".to_string());

    // smart-case：全小写模式自动忽略大小写
    let smart_case = !parsed.case_insensitive && parsed.pattern.chars().all(|c| !c.is_uppercase());
    if parsed.case_insensitive || smart_case {
        args.push("-i".to_string());
    }

    // output_mode
    match parsed.output_mode {
        OutputMode::Default => {} // 默认输出 content
        OutputMode::FilesOnly => args.push("-l".to_string()),
        OutputMode::CountOnly => args.push("-c".to_string()),
        OutputMode::FilesWithoutMatch => args.push("--files-without-match".to_string()),
    }

    // glob 过滤
    for g in &parsed.glob_filters {
        args.push("-g".to_string());
        args.push(g.clone());
    }

    // 语言类型过滤（rg 原生支持 100+ 种语言）
    if let Some(ref type_name) = parsed.type_filter {
        args.push("-t".to_string());
        args.push(type_name.clone());
    }

    // 上下文
    if parsed.before_context > 0 && parsed.after_context > 0 {
        args.push("-C".to_string());
        args.push(parsed.before_context.max(parsed.after_context).to_string());
    } else {
        if parsed.before_context > 0 {
            args.push("-B".to_string());
            args.push(parsed.before_context.to_string());
        }
        if parsed.after_context > 0 {
            args.push("-A".to_string());
            args.push(parsed.after_context.to_string());
        }
    }

    // 其他 flags
    if parsed.whole_word {
        args.push("-w".to_string());
    }
    if parsed.fixed_strings {
        args.push("-F".to_string());
    }
    if parsed.invert_match {
        args.push("-v".to_string());
    }
    if parsed.multiline {
        args.push("-U".to_string());
        args.push("--multiline-dotall".to_string());
    }
    if let Some(depth) = parsed.max_depth {
        args.push("--max-depth".to_string());
        args.push(depth.to_string());
    }

    // pattern 和搜索路径
    args.push("--".to_string());
    args.push(parsed.pattern.clone());
    args.push(search_path.display().to_string());

    args
}

/// 将 Glob 参数映射为 rg --files 命令行参数
fn build_glob_args(pattern: &str, search_root: &Path) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    args.push("--files".to_string());
    args.push("--hidden".to_string());

    // 排除目录（与 should_skip_dir 对齐）
    let skip_dirs = [
        ".git",
        ".claude",
        ".worktrees",
        "node_modules",
        "dist",
        "build",
        ".next",
        ".turbo",
        "coverage",
        ".nyc_output",
        "temp",
        ".cache",
        "vendor",
        "venv",
        "__pycache__",
        "target",
        "out",
        ".output",
    ];
    for dir in &skip_dirs {
        args.push("-g".to_string());
        args.push(format!("!{dir}"));
    }

    // 文件模式过滤
    args.push("-g".to_string());
    args.push(pattern.to_string());

    args.push(search_root.display().to_string());

    args
}

/// 使用 rg 执行 Grep 搜索。
///
/// 成功返回格式化输出，失败（超时/进程错误）返回 None 触发 Fallback。
pub async fn execute_rg_grep(
    rg_path: &Path,
    parsed: &ParsedArgs,
    cwd: &str,
    head_limit: usize,
    offset: usize,
) -> Option<String> {
    let search_path = match &parsed.path {
        Some(p) => {
            let p = Path::new(p);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                // 清理相对路径中的 ./ 或 .\ 前缀，避免 rg 输出 ./xxx 格式
                let cleaned = p
                    .to_string_lossy()
                    .trim_start_matches("./")
                    .trim_start_matches(".\\")
                    .to_string();
                if cleaned.is_empty() {
                    PathBuf::from(cwd)
                } else {
                    Path::new(cwd).join(&cleaned)
                }
            }
        }
        None => PathBuf::from(cwd),
    };

    if !search_path.exists() {
        return Some(format!(
            "Search path does not exist: {}",
            search_path.display()
        ));
    }

    // 传给 rg 的路径：优先传相对路径（相对于 cwd），这样 rg 输出就是相对路径，无需转换
    let rg_search_arg = {
        let cwd_path = Path::new(cwd);
        search_path
            .strip_prefix(cwd_path)
            .map(|rel| {
                let s = rel.to_string_lossy().replace('\\', "/");
                if s.is_empty() {
                    ".".to_string()
                } else {
                    s
                }
            })
            .unwrap_or_else(|_| search_path.display().to_string())
    };

    let args = build_grep_args(parsed, Path::new(&rg_search_arg), cwd);

    tracing::debug!(args = ?args, "executing rg grep");

    let result = timeout(
        Duration::from_secs(RG_TIMEOUT_SECS),
        Command::new(rg_path)
            .args(&args)
            .current_dir(cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await;

    let output = match result {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "rg execution failed, falling back to Rust engine");
            return None;
        }
        Err(_) => {
            tracing::warn!("rg timed out, falling back to Rust engine");
            return None;
        }
    };

    // rg exit code: 0=matches found, 1=no matches, 2=error
    if output.status.code() == Some(2) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(stderr = %stderr, "rg returned error, falling back");
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    // 转换路径为相对于 cwd 的格式
    // rg 在 current_dir(cwd) 下运行时，输出路径可能是绝对或相对，统一用 Path 处理
    let cwd_path = Path::new(cwd);
    let converted: Vec<String> = lines
        .iter()
        .map(|line| convert_rg_line(line, cwd_path))
        .collect();

    // FilesOnly / FilesWithoutMatch 模式需要排序
    let converted = match parsed.output_mode {
        OutputMode::FilesOnly | OutputMode::FilesWithoutMatch => {
            let mut keyed: Vec<(String, std::time::SystemTime)> = converted
                .into_iter()
                .map(|p| {
                    let abs = Path::new(cwd).join(&p);
                    let mtime = std::fs::metadata(&abs)
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::UNIX_EPOCH);
                    (p, mtime)
                })
                .collect();
            #[cfg(test)]
            keyed.sort_by(|a, b| a.0.cmp(&b.0));
            #[cfg(not(test))]
            keyed.sort_by(|a, b| match b.1.cmp(&a.1) {
                std::cmp::Ordering::Equal => a.0.cmp(&b.0),
                other => other,
            });
            keyed.into_iter().map(|(p, _)| p).collect()
        }
        _ => converted,
    };

    // 应用 offset + head_limit
    let total = converted.len();
    let was_truncated = head_limit > 0 && total > offset.saturating_add(head_limit);
    let slice_end = if head_limit > 0 {
        offset.saturating_add(head_limit).min(total)
    } else {
        total
    };
    let slice_start = offset.min(total);
    let items: Vec<String> = converted[slice_start..slice_end].to_vec();

    // 格式化输出（与 Rust 引擎对齐）
    let output_str = format_grep_output(
        parsed.output_mode,
        &items,
        total,
        head_limit,
        offset,
        was_truncated,
    );
    Some(output_str)
}

/// 使用 rg --files 执行 Glob 搜索。
///
/// 成功返回格式化输出，失败返回 None 触发 Fallback。
pub async fn execute_rg_glob(
    rg_path: &Path,
    pattern: &str,
    search_root: &Path,
    cwd: &str,
    max_results: usize,
) -> Option<String> {
    let args = build_glob_args(pattern, search_root);

    tracing::debug!(args = ?args, "executing rg glob");

    let result = timeout(
        Duration::from_secs(RG_TIMEOUT_SECS),
        Command::new(rg_path)
            .args(&args)
            .current_dir(cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await;

    let output = match result {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "rg glob failed, falling back");
            return None;
        }
        Err(_) => {
            tracing::warn!("rg glob timed out, falling back");
            return None;
        }
    };

    if output.status.code() == Some(2) {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Glob 返回绝对路径（与 Rust 引擎对齐：collect_files 返回绝对路径）
    let mut results: Vec<String> = stdout
        .lines()
        .map(|line| {
            let p = Path::new(line);
            if p.is_absolute() {
                line.to_string()
            } else {
                Path::new(cwd).join(line).display().to_string()
            }
        })
        .collect();

    // 按 mtime 排序（与 Rust 引擎对齐）
    results.sort_by(|a, b| {
        let ta = std::fs::metadata(a).and_then(|m| m.modified()).ok();
        let tb = std::fs::metadata(b).and_then(|m| m.modified()).ok();
        tb.cmp(&ta)
    });

    // 截断
    if results.len() > max_results {
        let full = results.join("\n");
        let truncated = &results[..max_results];
        let persist_hint = super::super::output_persist::persist_truncated_output(&full);
        return Some(format!(
            "{}\n\n[Output truncated: {} files total, showing first {}]{}",
            truncated.join("\n"),
            results.len(),
            max_results,
            persist_hint
        ));
    }

    if results.is_empty() {
        Some("No files found.".to_string())
    } else {
        Some(results.join("\n"))
    }
}

/// 转换 rg 输出行的路径为相对于 cwd 的格式，并统一输出格式。
///
/// rg 输出格式: `path/to/file.rs:123: content` 或 `path/to/file.rs:123- context`
/// 路径可能是绝对/相对/带 `./` 前缀，统一转为不含 `./` 的相对路径。
/// rg 输出的 `:line:` 后可能无空格（`path:123:content`），Rust 引擎有空格（`path:123: content`），
/// 需要在行号后的 `:` 后补充空格以对齐格式。
fn convert_rg_line(line: &str, cwd: &Path) -> String {
    let path_end = find_path_end(line);
    if path_end == 0 {
        return line.to_string();
    }

    let path_part = &line[..path_end];
    let rest = &line[path_end..];
    let path = Path::new(path_part);

    // strip_prefix(cwd) 转相对路径
    let mut display = path
        .strip_prefix(cwd)
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path_part.replace('\\', "/"));

    // 清理 ./ 前缀
    if display.starts_with("./") {
        display = display[2..].to_string();
    }

    // 统一格式：rg 输出 `:123:content`，Rust 引擎 `:123: content`，补空格
    let rest_with_space = fix_line_number_spacing(rest);

    format!("{}{}", display, rest_with_space)
}

/// rg 输出的 `:line:content` 在行号后的 `:` 后无空格，Rust 引擎有空格。
/// 对 `:NUM:` 格式补充空格，对 `:NUM-` 上下文格式保持不变。
fn fix_line_number_spacing(rest: &str) -> String {
    // rest 格式: ":123: content" 或 ":123-content" 或 ":123+content"
    // 找到第二个 ':'（行号结束后的冒号），检查后面是否有空格
    let bytes = rest.as_bytes();
    if bytes.len() < 2 || bytes[0] != b':' {
        return rest.to_string();
    }
    // 跳过行号数字
    let mut i = 1;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b':' {
        // 匹配行分隔符
        if i + 1 < bytes.len() && bytes[i + 1] != b' ' {
            return format!("{} {}", &rest[..=i], &rest[i + 1..]);
        }
    }
    rest.to_string()
}

/// 找到 rg 输出行中路径部分的结束位置（行号前的 `:` 索引）。
/// Windows 绝对路径含盘符 `:`（如 `C:\`），需要跳过。
fn find_path_end(line: &str) -> usize {
    let bytes = line.as_bytes();
    if bytes.len() < 2 {
        return 0;
    }
    // Windows 盘符：如 "C:/..." 或 "C:\..."，跳过前两个字符后的 ':'
    let start = if bytes.len() > 2 && bytes[1] == b':' && (bytes[2] == b'/' || bytes[2] == b'\\') {
        2
    } else {
        0
    };
    // 从 start 之后找第一个 ':'
    line[start..].find(':').map(|i| start + i).unwrap_or(0)
}

/// 格式化 Grep 输出（与 Rust 引擎的输出格式对齐）
fn format_grep_output(
    mode: OutputMode,
    items: &[String],
    _total: usize,
    head_limit: usize,
    offset: usize,
    was_truncated: bool,
) -> String {
    let pagination_info = || -> String {
        let mut parts: Vec<String> = Vec::new();
        if was_truncated {
            parts.push(format!("limit: {}", head_limit));
        }
        if offset > 0 {
            parts.push(format!("offset: {}", offset));
        }
        parts.join(", ")
    };

    match mode {
        OutputMode::Default => {
            if items.is_empty() {
                "No matches found".to_string()
            } else {
                let mut s = items.join("\n");
                if was_truncated {
                    s.push_str(&format!(
                        "\n\n[Showing results with pagination = {}]",
                        pagination_info()
                    ));
                }
                if was_truncated && head_limit > 0 {
                    let persist_hint = super::super::output_persist::persist_truncated_output(&s);
                    s.push_str(&persist_hint);
                }
                s
            }
        }
        OutputMode::FilesOnly | OutputMode::FilesWithoutMatch => {
            if items.is_empty() {
                "No files found".to_string()
            } else {
                let n = items.len();
                let plural = if n == 1 { "file" } else { "files" };
                let limit_info = pagination_info();
                let header = if limit_info.is_empty() {
                    format!("Found {} {}", n, plural)
                } else {
                    format!("Found {} {} {}", n, plural, limit_info)
                };
                let mut s = format!("{}\n{}", header, items.join("\n"));
                if was_truncated && head_limit > 0 {
                    let persist_hint = super::super::output_persist::persist_truncated_output(&s);
                    s.push_str(&persist_hint);
                }
                s
            }
        }
        OutputMode::CountOnly => {
            let mut total_matches: usize = 0;
            let mut file_count: usize = 0;
            for item in items {
                if let Some(idx) = item.rfind(':') {
                    if let Ok(c) = item[idx + 1..].parse::<usize>() {
                        total_matches += c;
                        file_count += 1;
                    }
                }
            }
            let raw = if items.is_empty() {
                "No matches found".to_string()
            } else {
                items.join("\n")
            };
            let occ_plural = if total_matches == 1 {
                "occurrence"
            } else {
                "occurrences"
            };
            let file_plural = if file_count == 1 { "file" } else { "files" };
            let limit_info = pagination_info();
            let mut s = format!(
                "{}\n\nFound {} total {} across {} {}.",
                raw, total_matches, occ_plural, file_count, file_plural
            );
            if !limit_info.is_empty() {
                s.push_str(&format!(" with pagination = {}", limit_info));
            }
            if was_truncated && head_limit > 0 {
                let persist_hint = super::super::output_persist::persist_truncated_output(&s);
                s.push_str(&persist_hint);
            }
            s
        }
    }
}

#[cfg(test)]
mod tests {
    include!("rg_engine_test.rs");
}
