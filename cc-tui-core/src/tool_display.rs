use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 工具参数里的路径 → 展示用路径，逐条对齐 CC `utils/file.ts::getDisplayPath`：
///
/// 1. 在 cwd 内 → **相对路径**（`cc-tui/src/ui/render_thread.rs`）；
/// 2. 在 home 内 → **`~/...`** 记法；
/// 3. 其余 → **绝对路径原样**。
///
/// 旧实现的 fallback 是「只取末段文件名」，于是 cwd 外的文件（以及未传 cwd 的
/// 情况）一律退化成 `render_thread.rs`，用户看不出它到底在哪、也无法与其他
/// 同名文件区分。CC 从不丢掉目录信息。
fn strip_cwd(path: &str, cwd: Option<&str>) -> String {
    // Windows 路径统一用 `/` 参与展示与匹配（CC 在 Windows 上同样输出相对路径）
    let normalized = path.replace('\\', "/");
    let path = normalized.as_str();

    if let Some(cwd) = cwd {
        let cwd = cwd.replace('\\', "/");
        let base = if cwd.ends_with('/') {
            cwd
        } else {
            format!("{cwd}/")
        };
        if let Some(rel) = path.strip_prefix(base.as_str()) {
            return sanitize_display_text(rel);
        }
    }

    // home 目录 → `~/...`
    if let Some(home) = home_dir_string() {
        let base = if home.ends_with('/') {
            home
        } else {
            format!("{home}/")
        };
        if let Some(rel) = path.strip_prefix(base.as_str()) {
            return sanitize_display_text(&format!("~/{rel}"));
        }
    }

    sanitize_display_text(path)
}

/// 当前用户 home 目录的展示形式（已归一化为 `/` 分隔）；取不到时返回 None。
fn home_dir_string() -> Option<String> {
    dirs_next::home_dir().map(|home| home.to_string_lossy().replace('\\', "/"))
}

/// 返回简短 display name，控制在 3-6 字符以保持 UI 对齐
///
/// 对齐 CC 的 userFacingName：`Edit` 工具显示为 `Update`
/// （CC 中 `old_string === ''` 时显示 `Create`，见 [`format_tool_name_with_input`]）。
pub fn format_tool_name(tool: &str) -> String {
    match tool {
        "Bash" => "Bash",
        "Read" => "Read",
        "Skill" => "Skill",
        "Write" => "Write",
        "Edit" => "Update",
        "Glob" => "Glob",
        "Grep" => "Grep",
        "TodoWrite" => "Todo",
        "AskUserQuestion" => "Ask",
        "Agent" => "Agent",
        "LSP" => "LSP",
        other => return to_pascal(&sanitize_display_text(other)),
    }
    .to_string()
}

/// 会话工具统计（状态栏 `✓ Name ×N`）专用的显示名。
///
/// 与 [`format_tool_name`] 的唯一差别：`Edit` 统一显示为 **`Edit`**，不转成
/// `Update`。统计的 key 只有工具名、拿不到入参，无法像 CC `userFacingName(input)`
/// 那样区分 `Create`（`old_string == ""`）与 `Update`；若仍显示 `Update`，用户
/// 会把 `Create` 的次数也算进 `Update`（真机疑问：「这个地方哪有 Update」）。
/// 消息区的工具头仍按入参显示 `Create` / `Update`，不受影响。
pub fn format_tool_name_for_stats(tool: &str) -> String {
    if tool == "Edit" {
        return "Edit".to_string();
    }
    format_tool_name(tool)
}

/// 带工具入参的 display name，对齐 CC 的 `userFacingName(input)`。
///
/// CC 的 Edit 工具按 `old_string` 区分：空字符串表示新建 → `Create`，
/// 其余情况 → `Update`。无入参时回退到 [`format_tool_name`]（CC 的 `!input` 分支也是 `Update`）。
/// 是否 `Agent(action="stop")` 调用——展示层特判（动作名、默认展开）共用，
/// 避免以 `display_name == "Stop Task"` 这类展示产物作为判据（改文案即静默失效）。
pub fn is_agent_stop(tool: &str, input: Option<&serde_json::Value>) -> bool {
    tool == "Agent"
        && input
            .and_then(|v| v.get("action"))
            .and_then(|v| v.as_str())
            .is_some_and(|a| a == "stop")
}

pub fn format_tool_name_with_input(tool: &str, input: Option<&serde_json::Value>) -> String {
    if is_agent_stop(tool, input) {
        return "Stop Task".to_string();
    }
    if tool == "Edit" {
        let is_create = input
            .and_then(|v| v.get("old_string"))
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.is_empty());
        if is_create {
            return "Create".to_string();
        }
    }
    format_tool_name(tool)
}

// ── DiffInput 构造 ──────────────────────────────────────────────────────────

/// 按条目数淘汰的进程内缓存：满了整体清空。
///
/// 服务于「流式渲染每帧重建 VM」这类高频短暂缓存——命中率不是关键，
/// 避免无上限增长才是。两个使用方（Edit 全文 diff、Write 旧内容）共用同一套淘汰策略。
struct CapacityCache<K, V> {
    inner: Mutex<HashMap<K, V>>,
    capacity: usize,
}

impl<K: std::hash::Hash + Eq, V: Clone> CapacityCache<K, V> {
    fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            capacity,
        }
    }

    fn get<Q>(&self, key: &Q) -> Option<V>
    where
        K: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq + ?Sized,
    {
        self.inner.lock().ok()?.get(key).cloned()
    }

    fn insert(&self, key: K, value: V) {
        if let Ok(mut map) = self.inner.lock() {
            if map.len() >= self.capacity {
                map.clear();
            }
            map.insert(key, value);
        }
    }
}

/// Edit diff 缓存：key 含文件 mtime + 长度，磁盘内容再变化时自动失效。
/// 流式渲染每 16ms 重建一次 VM，避免重复读盘 + 重建全文。
static EDIT_DIFF_CACHE: std::sync::LazyLock<CapacityCache<String, cc_widgets::DiffInput>> =
    std::sync::LazyLock::new(|| CapacityCache::new(128));

/// 单个文件超过此大小不做全文 diff：cc-widgets 以 `old.len() + new.len()` 对比
/// `MAX_DIFF_SIZE_BYTES`（1 MB），全文 diff 会同时携带新旧两份内容，故取一半。
/// 超限回退片段 diff，避免触发「diff too large」截断提示。
const FULL_FILE_DIFF_MAX_BYTES: u64 = 500_000;

/// Write 工具写入前的文件旧内容（按 `tool_call_id` 记录）。
///
/// Write 是整文件覆盖，执行后磁盘上只剩新内容，无法像 Edit 那样反推，
/// 因此必须由执行侧在发出 ToolStart 前抓取，经 ACP 元数据传入。值 `None` 表示文件不存在 / 不可读 /
/// 超过 [`FULL_FILE_DIFF_MAX_BYTES`]，此时按 create 视图渲染。
static WRITE_OLD_CONTENT: std::sync::LazyLock<CapacityCache<String, Option<String>>> =
    std::sync::LazyLock::new(|| CapacityCache::new(128));

/// 缓存 ACP 传来的写入前快照。此处不能读盘：消费 ToolStart 时写入可能已完成。
pub fn cache_write_old_content(tool_call_id: &str, old_content: Option<String>) {
    if tool_call_id.is_empty() {
        return;
    }
    WRITE_OLD_CONTENT.insert(tool_call_id.to_string(), old_content);
}

/// 取出 Write 执行前的旧内容。外层 `None` = 未记录过（历史回放等），按 create 视图处理。
fn write_old_content(tool_call_id: &str) -> Option<Option<String>> {
    WRITE_OLD_CONTENT.get(tool_call_id)
}

/// 从工具名与入参构造 DiffInput（仅成功的 Write/Edit）。
///
/// Edit：优先读取磁盘上的**完整文件**，用 `new_string → old_string` 反推编辑前全文，
/// 让 diff 显示真实行号与前后各 3 行上下文（对齐 CC 基于 `structuredPatch` 的展示）。
/// 文件不可读 / 编辑内容已被后续修改 / 文件过大时回退为 `old_string` vs `new_string` 片段 diff。
///
/// Write：用 [`cache_write_old_content`] 保存的执行侧旧内容做真 diff；
/// 文件原本不存在（或缓存未命中）时按 create 的全绿视图渲染。
pub fn build_diff_input(
    name: &str,
    input: &serde_json::Value,
    cwd: Option<&str>,
    tool_call_id: Option<&str>,
) -> Option<cc_widgets::DiffInput> {
    match name {
        "Edit" => {
            let old_string = input
                .get("old_string")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let new_string = input
                .get("new_string")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let file_path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if old_string.is_empty() || file_path.is_empty() {
                return None;
            }
            let replace_all = input
                .get("replace_all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if let Some(di) =
                edit_full_file_diff(file_path, old_string, new_string, replace_all, cwd)
            {
                return Some(di);
            }
            // 回退：片段 diff（磁盘不可读或内容已再次变化）
            Some(cc_widgets::DiffInput {
                file_path: file_path.to_string(),
                old_content: old_string.to_string(),
                new_content: new_string.to_string(),
                is_new_file: false,
                is_deleted_file: false,
                is_binary: false,
            })
        }
        "Write" => {
            let content = input.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let file_path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if content.is_empty() || file_path.is_empty() {
                return None;
            }
            let append = input
                .get("append")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            // 对齐 CC FileWriteTool：文件已存在时用旧内容做真 diff（类型 update），
            // 只有文件原本不存在才按 create 的全绿视图渲染。
            if let Some(Some(old_content)) = tool_call_id.and_then(write_old_content) {
                let new_content = if append {
                    format!("{old_content}{content}")
                } else {
                    content.to_string()
                };
                // 写入内容与旧内容一致：无变化可展示，保留工具结果行
                if new_content == old_content {
                    return None;
                }
                return Some(cc_widgets::DiffInput {
                    file_path: file_path.to_string(),
                    old_content,
                    new_content,
                    is_new_file: false,
                    is_deleted_file: false,
                    is_binary: false,
                });
            }
            Some(cc_widgets::DiffInput {
                file_path: file_path.to_string(),
                old_content: String::new(),
                new_content: content.to_string(),
                is_new_file: true,
                is_deleted_file: false,
                is_binary: false,
            })
        }
        _ => None,
    }
}

/// 基于磁盘上的完整文件重建编辑前后全文，返回全文 diff。
fn edit_full_file_diff(
    file_path: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
    cwd: Option<&str>,
) -> Option<cc_widgets::DiffInput> {
    // 纯删除无法从结果反推插入位置，回退片段 diff
    if new_string.is_empty() {
        return None;
    }
    // replace_all 反推存在固有歧义：编辑后文件中的 new_string 无法区分
    // "原本就存在"与"本次由 old_string 替换而来"（例：`bar foo` 全量替换
    // `foo→bar` 后为 `bar bar`，反推会错误得到 `foo foo`），正向校验也会通过。
    // 无法可靠重建，回退片段 diff。
    if replace_all {
        return None;
    }
    let path: PathBuf = match cwd {
        Some(cwd) => Path::new(cwd).join(file_path),
        None => PathBuf::from(file_path),
    };
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() > FULL_FILE_DIFF_MAX_BYTES {
        return None;
    }
    let stamp = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos().to_string())
        .unwrap_or_default();
    let key = format!(
        "{}\u{0}{}\u{0}{}\u{0}{}:{}",
        file_path,
        old_string,
        new_string,
        meta.len(),
        stamp
    );
    if let Some(hit) = EDIT_DIFF_CACHE.get(&key) {
        return Some(hit);
    }

    let new_full = std::fs::read_to_string(&path).ok()?;
    let old_full = replace_first(&new_full, new_string, old_string)?;
    // 正向校验：对旧全文应用同一编辑必须还原出当前文件，
    // 避免 new_string 在文件中另有出现时反推错位置。
    let verified = replace_first(&old_full, old_string, new_string)?;
    if verified != new_full {
        return None;
    }

    let di = cc_widgets::DiffInput {
        file_path: file_path.to_string(),
        old_content: old_full,
        new_content: new_full,
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    EDIT_DIFF_CACHE.insert(key, di.clone());
    Some(di)
}

/// 替换第一次出现的 `needle`
fn replace_first(haystack: &str, needle: &str, replacement: &str) -> Option<String> {
    let idx = haystack.find(needle)?;
    let mut out = String::with_capacity(haystack.len());
    out.push_str(&haystack[..idx]);
    out.push_str(replacement);
    out.push_str(&haystack[idx + needle.len()..]);
    Some(out)
}

/// 工具命令/参数摘要的字符硬上限，仅作为防御极端超长输入的兜底。
///
/// 不能用较小值：详细模式（Ctrl+O）下参数需要完整折行展示，若在此处
/// 提前硬截断，下游 `tool_args_header` 将无法还原完整内容（issue #258）。
pub const TOOL_ARGS_MAX_CHARS: usize = 4000;
pub const BASH_COMMAND_MAX_CHARS: usize = TOOL_ARGS_MAX_CHARS;

/// 返回参数摘要（含路径缩短逻辑）
pub fn format_tool_args(
    tool: &str,
    input: &serde_json::Value,
    cwd: Option<&str>,
) -> Option<String> {
    match tool {
        "Bash" => input["command"]
            .as_str()
            .map(|s| truncate(s, BASH_COMMAND_MAX_CHARS)),
        "Read" => {
            let path = input["file_path"]
                .as_str()
                .or_else(|| input["path"].as_str());
            path.map(|p| {
                let mut display = strip_cwd(p, cwd);
                let offset = input["offset"].as_u64();
                let limit = input["limit"].as_u64();
                // 对齐参考实现的 verbose 文案（` · lines 起-止` / ` · from line 起`）：
                // offset 或 limit 任一存在就显示，起始行缺省为 1。
                // 用固定文案而不是 `:起-止`，渲染层据此切分路径与范围，不必从
                // 数字位置反推，避免文件名自带 `:123` 时被误当成行号范围。
                if offset.is_some() || limit.is_some() {
                    let start = offset.unwrap_or(1);
                    if let Some(limit) = limit {
                        display.push_str(&format!(" · lines {}-{}", start, start + limit - 1));
                    } else {
                        display.push_str(&format!(" · from line {start}"));
                    }
                }
                display
            })
        }
        // 对齐 CC SkillTool：工具头仅显示技能名，args 仍完整传给执行侧。
        "Skill" => input["skill"]
            .as_str()
            .map(|skill| truncate(skill, TOOL_ARGS_MAX_CHARS)),
        "Write" | "Edit" => {
            let path = input["file_path"]
                .as_str()
                .or_else(|| input["path"].as_str());
            path.map(|p| strip_cwd(p, cwd))
        }
        "Glob" => input["pattern"].as_str().map(|p| truncate(p, 200)),
        "Grep" => input["pattern"].as_str().map(|s| truncate(s, 200)),
        "WebSearch" => input["query"]
            .as_str()
            .map(|s| truncate(s, TOOL_ARGS_MAX_CHARS)),
        "WebFetch" => input["url"]
            .as_str()
            .map(|s| truncate(s, TOOL_ARGS_MAX_CHARS)),
        "ExecuteExtraTool" => input["tool_name"].as_str().map(|s| truncate(s, 40)),
        "SearchExtraTools" => input["query"].as_str().map(|s| truncate(s, 40)),
        "LSP" => input["operation"].as_str().map(|s| truncate(s, 40)),
        "AskUserQuestion" => ask_user_summary(input),
        "Agent" => input["description"]
            .as_str()
            .or_else(|| input["prompt"].as_str())
            // stop 调用只有 action/task_id，回退展示 task_id 避免 header 为空（issue #831）
            .or_else(|| {
                if is_agent_stop("Agent", Some(input)) {
                    input["task_id"].as_str()
                } else {
                    None
                }
            })
            .map(|s| truncate(s, 40)),
        "TodoWrite" => input["todos"].as_array().map(|todos| {
            crate::i18n::global().tr_args(
                "todo-count",
                &[("count".into(), todos.len().to_string().into())],
            )
        }),
        // 兜底：状态栏要求所有工具都有摘要，取第一个非空字符串字段
        _ => first_string_field(input).map(|s| truncate(s, 40)),
    }
}

/// AskUserQuestion 摘要：优先取首个问题的正文，取不到再退回标题
fn ask_user_summary(input: &serde_json::Value) -> Option<String> {
    let first = input["questions"].as_array()?.first()?;
    first["question"]
        .as_str()
        .or_else(|| first["header"].as_str())
        .map(|s| truncate(s, 40))
}

/// 通用兜底摘要：按 key 顺序取 input 里第一个非空字符串值
fn first_string_field(input: &serde_json::Value) -> Option<&str> {
    let obj = input.as_object()?;
    obj.values()
        .filter_map(|value| value.as_str())
        .find(|value| !value.trim().is_empty())
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
    let s = sanitize_display_text(s);
    if s.chars().count() <= max {
        s
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

pub fn sanitize_display_text(s: &str) -> String {
    sanitize_display_text_inner(s, false)
}

/// 审批正文保留脚本换行和缩进，同时复用完整控制序列扫描器。
pub(crate) fn sanitize_display_text_preserving_newlines(s: &str) -> String {
    sanitize_display_text_inner(s, true)
}

fn sanitize_display_text_inner(s: &str, preserve_newlines: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => skip_escape_sequence(&mut chars),
            '\u{009b}' => skip_csi_sequence(&mut chars),
            '\n' if preserve_newlines => out.push('\n'),
            '\r' if preserve_newlines => {}
            '\t' if preserve_newlines => out.push_str("    "),
            '\n' | '\r' | '\t' => out.push(' '),
            ch if ch.is_control() => {}
            ch => out.push(ch),
        }
    }
    out
}

fn skip_escape_sequence<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    match chars.peek().copied() {
        Some('[') => {
            chars.next();
            skip_csi_sequence(chars);
        }
        Some(']') => {
            chars.next();
            skip_osc_sequence(chars);
        }
        Some('P' | '_' | '^') => {
            chars.next();
            skip_string_control_sequence(chars);
        }
        Some(_) => {
            chars.next();
        }
        None => {}
    }
}

fn skip_csi_sequence<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    for ch in chars.by_ref() {
        if matches!(ch, '\u{0040}'..='\u{007e}') {
            break;
        }
    }
}

fn skip_osc_sequence<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    let mut saw_esc = false;
    for ch in chars.by_ref() {
        if ch == '\u{0007}' {
            break;
        }
        if saw_esc && ch == '\\' {
            break;
        }
        saw_esc = ch == '\x1b';
    }
}

fn skip_string_control_sequence<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    let mut saw_esc = false;
    for ch in chars.by_ref() {
        if saw_esc && ch == '\\' {
            break;
        }
        saw_esc = ch == '\x1b';
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("tool_display_test.rs");
}
