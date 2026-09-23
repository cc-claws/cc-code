use regex::Regex;
use std::sync::LazyLock;

static ANSI_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\x1b(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])").expect("valid ANSI regex")
});

/// 剥离终端 ANSI 转义码（颜色、光标控制等），并规范化回车符。
pub fn strip_ansi(input: &str) -> String {
    let stripped = ANSI_REGEX.replace_all(input, "");
    // 处理带 \r 的终端覆盖输出（如进度条）：保留最后一段
    let mut normalized_lines = Vec::new();
    for line in stripped.lines() {
        if line.contains('\r') {
            if let Some(last) = line.split('\r').rfind(|s| !s.is_empty()) {
                normalized_lines.push(last);
            }
        } else {
            normalized_lines.push(line);
        }
    }
    normalized_lines.join("\n")
}

/// 检查一行文本是否属于 git status 的交互提示噪音。
fn is_git_status_noise(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("(use \"git ")
        || trimmed.starts_with("no changes added to commit")
        || trimmed.starts_with("nothing added to commit")
        || trimmed == "nothing to commit, working tree clean"
}

/// 压缩 `git status` 输出：剥离所有无用的命令指引，只保留分支信息和变更文件。
pub fn filter_git_status(output: &str) -> String {
    let mut filtered_lines = Vec::new();
    let mut prev_empty = false;

    for line in output.lines() {
        if is_git_status_noise(line) {
            continue;
        }
        let is_empty = line.trim().is_empty();
        if is_empty && prev_empty {
            continue;
        }
        filtered_lines.push(line);
        prev_empty = is_empty;
    }

    filtered_lines.join("\n").trim().to_string()
}

/// 压缩 `cargo test` / `cargo nextest` 输出：
/// - 测试全部通过时：剥离大量 `test ... ok`，仅保留摘要结果
/// - 测试有失败时：剥离通过的测试，仅保留失败用例、panic 栈和汇总
pub fn filter_cargo_test(output: &str, exit_code: i32) -> String {
    let mut filtered_lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        // 过滤通过的测试用例
        if trimmed.starts_with("test ") && trimmed.ends_with("... ok") {
            continue;
        }
        // 如果全部成功，过滤 "running X tests" 冗余行
        if exit_code == 0 && trimmed.starts_with("running ") && trimmed.ends_with("tests") {
            continue;
        }
        filtered_lines.push(line);
    }

    let result = filtered_lines.join("\n").trim().to_string();
    if result.is_empty() {
        if exit_code == 0 {
            "test result: ok (all tests passed)".to_string()
        } else {
            output.to_string()
        }
    } else {
        result
    }
}

/// 压缩 `cargo build` / `cargo check` / `cargo clippy` 输出：
/// 剥离 `Compiling ...`、`Checking ...`、`Downloading ...` 刷屏日志，
/// 仅保留 `warning:`、`error:` 和最终 `Finished` 状态。
pub fn filter_cargo_build_or_check(output: &str, exit_code: i32) -> String {
    let mut filtered_lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Compiling ")
            || trimmed.starts_with("Checking ")
            || trimmed.starts_with("Downloaded ")
            || trimmed.starts_with("Downloading ")
            || trimmed.starts_with("Fetch ")
            || trimmed.starts_with("Updating ")
        {
            continue;
        }
        filtered_lines.push(line);
    }

    let result = filtered_lines.join("\n").trim().to_string();
    if result.is_empty() {
        if exit_code == 0 {
            "Finished (no warnings or errors)".to_string()
        } else {
            output.to_string()
        }
    } else {
        result
    }
}

/// 过滤 RTK 自身向 stderr 输出的外部宿主提示噪音（如未安装 Hook 或 Hook 过期的提示），
/// 避免污染 stderr 并被 `format_command_output` 误标为 `[stderr]` 从而误导 Agent。
pub fn clean_rtk_stderr_noise(stderr: &str) -> String {
    if !stderr.contains("[rtk] /!\\") {
        return stderr.to_string();
    }
    let cleaned = stderr
        .lines()
        .filter(|line| !line.trim_start().starts_with("[rtk] /!\\"))
        .collect::<Vec<_>>()
        .join("\n");
    cleaned.trim().to_string()
}

/// 对命令输出进行语义级轻量压缩。
///
/// 1. 首先剥离 ANSI 转义字符和回车覆盖。
/// 2. 根据执行的命令类型匹配最合适的过滤器（git status, cargo test, cargo check/build 等）。
/// 3. 若无匹配规则，返回剥离 ANSI 后的原始文本。
pub fn filter_command_output(command: &str, output: &str, exit_code: i32) -> String {
    let clean_output = strip_ansi(output);
    let cmd_lower = command.to_lowercase();

    // 命令匹配逻辑（支持带路径或前缀参数的情况）
    if cmd_lower.contains("git status") {
        filter_git_status(&clean_output)
    } else if cmd_lower.contains("cargo test") || cmd_lower.contains("cargo nextest") {
        filter_cargo_test(&clean_output, exit_code)
    } else if cmd_lower.contains("cargo check")
        || cmd_lower.contains("cargo build")
        || cmd_lower.contains("cargo clippy")
    {
        filter_cargo_build_or_check(&clean_output, exit_code)
    } else {
        // 通用折叠：先尝试块级折叠（多行 warning 块），再做行级折叠（连续相同行）
        let folded = fold_repeated_blocks(&clean_output);
        fold_repeated_lines(&folded)
    }
}

/// 通用重复行折叠：将连续出现的相同行折叠为前 2 个完整样例 + `... (+N more identical lines)`。
///
/// 这是 `filter_command_output` 兜底分支的通用压缩策略（Issue #214），
/// 适用于任何未被特定命令过滤器覆盖的输出。典型场景：构建工具的同类 warning 刷屏。
fn fold_repeated_lines(output: &str) -> String {
    /// 连续相同行超过此阈值时触发折叠（保留前 KEEP_FIRST 个）
    const FOLD_THRESHOLD: usize = 3;
    /// 折叠时保留的完整样例行数
    const KEEP_FIRST: usize = 2;

    let lines: Vec<&str> = output.lines().collect();
    if lines.len() < FOLD_THRESHOLD {
        return output.to_string();
    }

    let mut result: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        let current = lines[i];
        // 计算从 i 开始连续相同行的数量
        let mut run_len = 1;
        while i + run_len < lines.len() && lines[i + run_len] == current {
            run_len += 1;
        }

        if run_len >= FOLD_THRESHOLD {
            // 保留前 KEEP_FIRST 行，折叠其余
            for _ in 0..KEEP_FIRST.min(run_len) {
                result.push(current.to_string());
            }
            let folded = run_len - KEEP_FIRST.min(run_len);
            if folded > 0 {
                result.push(format!("... (+{folded} more identical lines)"));
            }
        } else {
            for _ in 0..run_len {
                result.push(current.to_string());
            }
        }
        i += run_len;
    }

    result.join("\n")
}

/// 通用重复块折叠：将多次出现的相同多行块（首行相同的连续行组）折叠。
///
/// "块"定义：以非空行开头、后续以缩进行（空格/tab 开头）续接的连续行组。
/// 当同一个块（首行完全相同）出现 ≥3 次时，保留前 2 个完整块 + 一行摘要。
pub fn fold_repeated_blocks(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return output.to_string();
    }

    // 拆分为块：每个块以非空、非缩进行开头，后续缩进行归入同块
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    let mut current_block: Vec<&str> = Vec::new();

    for line in &lines {
        let is_continuation = !line.is_empty() && (line.starts_with(' ') || line.starts_with('\t'));
        if is_continuation && !current_block.is_empty() {
            current_block.push(line);
        } else {
            if !current_block.is_empty() {
                blocks.push(std::mem::take(&mut current_block));
            }
            current_block.push(line);
        }
    }
    if !current_block.is_empty() {
        blocks.push(current_block);
    }

    // 如果块太少，不做块级折叠
    if blocks.len() < 3 {
        return output.to_string();
    }

    // 按首行分组计数，记录每个首行出现的次数
    let mut first_line_counts: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for block in &blocks {
        if let Some(first) = block.first() {
            *first_line_counts.entry(first).or_insert(0) += 1;
        }
    }

    // 只有至少一种块出现 ≥3 次才做折叠
    if !first_line_counts.values().any(|&c| c >= 3) {
        return output.to_string();
    }

    let mut result_lines: Vec<String> = Vec::new();
    let mut seen_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();

    for block in &blocks {
        let first = block.first().copied().unwrap_or("");
        let total = first_line_counts.get(first).copied().unwrap_or(1);

        if total >= 3 {
            let seen = seen_counts.entry(first).or_insert(0);
            *seen += 1;
            if *seen <= 2 {
                // 保留前 2 个完整块
                for line in block {
                    result_lines.push(line.to_string());
                }
            } else if *seen == 3 {
                // 第 3 次出现时插入折叠摘要
                let remaining = total - 2;
                result_lines.push(format!("... (+{remaining} more similar blocks: {first})"));
            }
            // 第 4+ 次静默跳过
        } else {
            for line in block {
                result_lines.push(line.to_string());
            }
        }
    }

    result_lines.join("\n")
}

#[cfg(test)]
#[path = "output_filter_test.rs"]
mod output_filter_test;
