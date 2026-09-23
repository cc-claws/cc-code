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
        // RTK 格式化后的干净工作区提示（`rtk git status` 输出 `clean — nothing to commit`）
        // RTK 源码三个版本（v0.44.2 / v0.49.0 / develop）均为 em dash，无 ASCII 变体
        || trimmed == "clean — nothing to commit"
}

/// 压缩 `git status` 输出：剥离所有无用的命令指引，只保留分支信息和变更文件。
///
/// 也用于 RTK 重写后的 `rtk git status` 输出——RTK 的 `clean — nothing to commit`
/// 会被识别为噪音行剔除（issue #207）。
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
        // 通用兜底：不做折叠，直接返回 strip_ansi 后的原始输出。
        // 原因（Issue #214 反馈）：通用折叠对代码类输出副作用过强，
        // 代码天然含大量重复行（`});`、`.unwrap();` 等），折叠会
        // 整体吞掉函数体，agent 被迫多次小窗口读取才能拼出完整实现。
        // RTK 已做了 token 压缩，peri 再加一层折叠弊大于利。
        clean_output
    }
}

// 注意：fold_repeated_lines / fold_repeated_blocks 已移除（Issue #214 反馈）。
// 通用折叠对代码类输出毒性太强——代码天然含大量重复行（`});`、`.unwrap();` 等），
// 折叠会整体吞掉函数体，agent 被迫多次小窗口读取才能拼出完整实现。
// 折叠标记形态又容易被误判为「内容结束」，产生二次误导。
// RTK 已做了 token 压缩，peri 再加一层折叠弊大于利。

#[cfg(test)]
#[path = "output_filter_test.rs"]
mod output_filter_test;
