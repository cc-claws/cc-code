//! 只读 shell 命令的确定性判定。
//!
//! 供两处复用：
//! - `cc-tui-core` 的展示分类（Bash 是否折叠为文件查看行）；
//! - `cc-agent` 的工具错配自动改道（判断误发到只读工具的 Bash 参数是否安全可执行）。
//!
//! # 为什么下沉到 cc-agent
//!
//! 判定原先只在 `cc-tui-core/src/message_view/tools.rs`，而工具改道逻辑位于本
//! crate（`agent/executor/tool_dispatch.rs`）。`cc-tui-core` 依赖 `cc-agent`，
//! 反向调用不成立，故把判定下沉到此处，由 `cc-tui-core` 转出后继续供展示层使用。
//!
//! # 判定是白名单式的，收紧/放宽都只改这里
//!
//! 当前实现按命令词法判定：命令链逐段检查首个可执行词是否属于查看器白名单，
//! 并排除写盘重定向与该命令的写盘参数。**这是一层启发式，不是沙箱**：例如
//! `awk`/`less` 虽在查看器白名单内，其参数中的 `system(...)`、`!cmd` 仍可能间接
//! 执行命令。调用方若把它用作执行门控，必须自行叠加审批环节，不能仅依赖本判定。
//!
//! 后续接入本地小模型判定时，只需替换 [`is_readonly_shell_command`] 的实现，
//! 调用方与测试稳定契约（`command: &str -> bool`）保持不动。

/// 只读文件查看命令的确定性判定（与参考实现的 `isSearchOrReadCommand` 同源）。
///
/// - 按 `|`、`||`、`&&`、`;`、换行拆段（**引号内与转义后的分隔符不算**）；
/// - 每段首个可执行须在白名单内，或属于中性命令（cd/pwd/echo/printf/true/false/:、`VAR=x` 前缀）；
/// - 任一段出现**写文件**的重定向（`>`/`>>`）→ 判定为非只读；
///   `2>/dev/null`、`2>&1`、`>&2` 只丢弃/合并 stderr，不算写盘；
/// - 至少有一段是真正的文件查看器（纯 `cd`/`echo` 不算）。
pub fn is_readonly_shell_command(command: &str) -> bool {
    const READERS: [&str; 21] = [
        "ls", "cat", "head", "tail", "find", "wc", "rg", "grep", "tree", "du", "stat", "file",
        "less", "more", "strings", "jq", "awk", "cut", "sort", "uniq", "tr",
    ];
    const NEUTRAL: [&str; 10] = [
        "cd", "pwd", "echo", "printf", "true", "false", ":", "export", "set", "which",
    ];

    let mut saw_reader = false;
    // 命令替换/子 shell 无法静态判定，保守判为非只读
    if command.contains("$(") || command.contains('`') {
        return false;
    }
    for raw in split_shell_segments(command) {
        let seg = raw.trim();
        if seg.is_empty() {
            continue;
        }
        if has_file_redirect(seg) {
            return false;
        }
        let mut tokens = seg.split_whitespace();
        let Some(first) = tokens.next() else {
            continue;
        };
        let mut exe = first;
        // 跳过 `VAR=value` 前缀
        while exe.contains('=') {
            match tokens.next() {
                Some(next) => exe = next,
                None => return false,
            }
        }
        let exe = exe.trim_matches(|c| c == '(' || c == ')');
        if NEUTRAL.contains(&exe) {
            continue;
        }
        let args: Vec<&str> = tokens.collect();
        // 各命令的"会写盘"参数：命中即判为非只读
        let writes = match exe {
            "sed" => args
                .iter()
                .any(|a| *a == "-i" || a.starts_with("-i") || *a == "--in-place"),
            "find" => args
                .iter()
                .any(|a| *a == "-delete" || *a == "-exec" || a.starts_with("-fprint")),
            "sort" => args.iter().any(|a| *a == "-o" || a.starts_with("-o")),
            _ => false,
        };
        if writes {
            return false;
        }
        if exe == "sed" {
            if args.iter().any(|t| t.starts_with("-n")) {
                saw_reader = true;
                continue;
            }
            return false;
        }
        if READERS.contains(&exe) {
            saw_reader = true;
            continue;
        }
        return false;
    }
    saw_reader
}

/// 按 `|`、`||`、`&&`、`;`、换行切分命令，但**跳过引号内与转义后**的分隔符。
fn split_shell_segments(command: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    cur.push(c);
                }
                '\\' => {
                    cur.push(c);
                    if let Some(next) = chars.next() {
                        cur.push(next);
                    }
                }
                '|' => {
                    // `||` 与 `|` 等价：都是命令分隔
                    if chars.peek() == Some(&'|') {
                        chars.next();
                    }
                    out.push(std::mem::take(&mut cur));
                }
                '&' if chars.peek() == Some(&'&') => {
                    chars.next();
                    out.push(std::mem::take(&mut cur));
                }
                ';' | '\n' => out.push(std::mem::take(&mut cur)),
                _ => cur.push(c),
            },
        }
    }
    out.push(cur);
    out.retain(|s| !s.trim().is_empty());
    out
}

/// 段内是否存在**写文件**的重定向。
///
/// `2>/dev/null`、`2>&1`、`>&2` 只是丢弃/合并错误输出，不算写盘，先剥离再判断；
/// 引号内的 `>`（如 `awk 'NR>=2900'`）是参数内容，同样不算。
fn has_file_redirect(seg: &str) -> bool {
    let stripped = mask_quoted(seg)
        .replace("2>/dev/null", "")
        .replace("2> /dev/null", "")
        .replace("2>&1", "")
        .replace(">&2", "");
    stripped.contains('>')
}

/// 把引号内与转义后的字符替换成空格（保持字符位置），只留命令结构。
fn mask_quoted(seg: &str) -> String {
    let mut out = String::with_capacity(seg.len());
    let mut quote: Option<char> = None;
    let mut chars = seg.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                    out.push(c);
                } else {
                    out.push(' ');
                }
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    out.push(c);
                }
                '\\' => {
                    out.push(' ');
                    if chars.next().is_some() {
                        out.push(' ');
                    }
                }
                _ => out.push(c),
            },
        }
    }
    out
}

#[cfg(test)]
#[path = "readonly_test.rs"]
mod tests;
