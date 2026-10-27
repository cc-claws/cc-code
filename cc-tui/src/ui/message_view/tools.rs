use crate::ui::theme;
use ratatui::style::Color;

/// 用户侧技能元消息的展示信息；只投影界面，不制造 Assistant/Tool 历史。
pub(crate) struct SkillInvocationDisplay<'a> {
    pub command_name: String,
    pub success: bool,
    pub display: bool,
    pub payload: &'a str,
}

pub(crate) fn skill_invocation_display(content: &str) -> Option<SkillInvocationDisplay<'_>> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Metadata {
        command_name: String,
        success: bool,
        status: String,
        #[serde(default)]
        display: bool,
    }
    let payload = cc_agent::agent::compact::re_inject::skill_invocation_payload(content)?;
    let metadata = serde_json::from_str::<Metadata>(payload).ok()?;
    if metadata.command_name.trim().is_empty()
        || !matches!(metadata.status.as_str(), "inline" | "forked")
    {
        return None;
    }
    Some(SkillInvocationDisplay {
        command_name: metadata.command_name,
        success: metadata.success,
        display: metadata.display,
        payload,
    })
}

/// 旧版 Bash 后台句柄的展示投影；原始 tool_result 保留给模型和历史。
/// 只识别完整 envelope，不删除普通输出里提到的标签或错误文本。
pub(crate) struct BackgroundTaskStarted<'a> {
    pub task_id: &'a str,
    pub output: &'a str,
}

impl<'a> BackgroundTaskStarted<'a> {
    pub fn parse(content: &'a str) -> Option<Self> {
        let body = content
            .trim()
            .strip_prefix("<background-task-started>")?
            .strip_suffix("</background-task-started>")?;
        let (task_id, rest) = body
            .trim_start()
            .strip_prefix("<task-id>")?
            .split_once("</task-id>")?;
        if task_id.is_empty()
            || task_id.len() > 128
            || !task_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            return None;
        }
        let (command, rest) = rest
            .trim_start()
            .strip_prefix("<command>")?
            .split_once("</command>")?;
        let output = rest
            .trim()
            .strip_prefix("<output>")?
            .strip_suffix("</output>")?;
        if command.is_empty() || output.is_empty() {
            return None;
        }
        Some(Self { task_id, output })
    }
}

/// 只读工具分类，用于折叠聚合
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    /// 搜索类（Glob/Grep）
    Search,
    /// Glob 搜索
    Glob,
    /// Grep 搜索
    Grep,
    /// Read 读取
    Read,
    /// Bash 文件查看（模型显式声明）
    Inspect,
    /// Write/Edit 写入
    Write,
    /// AskUserQuestion 提问
    AskUser,
}

impl ToolCategory {
    pub fn from_tool_name(name: &str) -> Option<Self> {
        match name {
            "Glob" => Some(Self::Glob),
            "Grep" => Some(Self::Search),
            "Read" => Some(Self::Read),
            "AskUserQuestion" => Some(Self::AskUser),
            _ => None,
        }
    }

    /// 按工具名和 Bash 的文件查看标记分配聚合分类。
    pub fn from_tool_block(name: &str, file_inspect: bool) -> Option<Self> {
        if name == "Bash" && file_inspect {
            return Some(Self::Inspect);
        }
        Self::from_tool_name(name)
    }
}

/// 文件查看类 Bash（模型显式声明 `file_inspect=true` 才成立）。
pub fn is_file_inspect(tool_name: &str, input: &serde_json::Value) -> bool {
    // 判定权 100% 在前端的命令词法上：模型不再需要、也不应该输出展示专用参数。
    // （历史会话里残留的 `file_inspect` 字段一律忽略。）
    if tool_name != "Bash" {
        return false;
    }
    input
        .get("command")
        .and_then(serde_json::Value::as_str)
        .is_some_and(is_readonly_shell_command)
}

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
            "sed" => args.iter().any(|a| *a == "-i" || a.starts_with("-i") || *a == "--in-place"),
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

/// 工具条目（聚合组内）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolEntry {
    pub tool_name: String,
    pub display_name: String,
    pub args_display: Option<String>,
    pub content: String,
    pub is_error: bool,
    pub file_inspect: bool,
}

/// SubAgent 批次聚合的摘要信息
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentSummary {
    pub agent_id: String,
    pub task_preview: String,
    /// 工具调用数
    pub tool_count: usize,
    /// 是否以错误结束
    pub is_error: bool,
    /// 最终结果（仅第一行）
    pub final_result: Option<String>,
}

/// 从 SubAgent 结果内容中解析工具调用次数。
///
/// 支持英文格式 `[Sub-agent executed N tool calls: ...]`
/// 或中文版 `[子 agent 执行了 N 个工具调用: ...]`。
/// 解析失败时返回 0（优雅降级）。
pub(crate) fn parse_subagent_tool_count(content: &str) -> usize {
    if let Some(rest) = content.strip_prefix("[Sub-agent executed ") {
        if let Some(n_str) = rest.split(' ').next() {
            if let Ok(n) = n_str.parse::<usize>() {
                return n;
            }
        }
    }
    if let Some(rest) = content
        .strip_prefix("[子 agent 执行了 ")
        .or_else(|| content.strip_prefix("[子agent 执行了 "))
    {
        if let Some(n_str) = rest.split(' ').next() {
            if let Ok(n) = n_str.parse::<usize>() {
                return n;
            }
        }
    }
    0
}

/// 按工具名分配颜色（按操作类型分色）
pub fn tool_color(name: &str) -> Color {
    match name {
        "Read" | "Glob" | "Grep" | "Skill" => theme::cur().success,
        "Write" | "Edit" | "delete_file" | "delete_folder" | "rm" | "rm_rf" => theme::cur().warning,
        "Bash" => theme::cur().bash_border,
        "Agent" | "AskUserQuestion" | "TodoWrite" => theme::cur().thinking,
        _ if name.contains("error") => theme::cur().error,
        _ => theme::cur().inactive,
    }
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
