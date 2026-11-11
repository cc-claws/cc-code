use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use tokio_util::sync::CancellationToken;

use crate::{
    agent::{
        events::AgentEvent,
        react::{ReactLLM, Reasoning, ToolCall, ToolErrorKind, ToolResult},
        state::State,
    },
    error::{AgentError, AgentResult},
    kernel::Kernel,
    messages::{message::MessageId, BaseMessage, ContentBlock, MessageContent, ToolCallRequest},
    shell::is_readonly_shell_command,
    tools::{validate_against_schema, BaseTool},
};

use super::ReActAgent;

/// 工具名语义别名表：LLM 输出的名称 → 实际注册的工具名。
const TOOL_ALIASES: &[(&str, &str)] = &[("task", "Agent"), ("shell", "Bash"), ("reading", "Read")];

/// 工具参数名别名表：LLM 输出的参数名 → 实际参数名。
/// 主要解决 Read/Write/Edit（file_path）与 Glob/Grep（path）之间的 LLM 参数名混淆。
const PARAM_ALIASES: &[(&str, &str)] = &[("path", "file_path")];

/// 仅对这些工具执行 `path` → `file_path` 别名转换。
/// Glob/Grep 的参数名本来就是 `path`，不能被篡改。
const FILE_PATH_TOOLS: &[&str] = &["Read", "Write", "Edit"];

/// 日志预览用截断：按字符数截断注入正文，避免 CJK 字符边界 panic。
///
/// 仅用于 tracing 输出，不改变实际注入内容。
fn truncate_for_log(text: &str) -> String {
    const MAX_CHARS: usize = 200;
    if text.chars().count() > MAX_CHARS {
        let head: String = text.chars().take(MAX_CHARS).collect();
        format!("{}...(truncated)", head)
    } else {
        text.to_string()
    }
}

/// 将 LLM 有时会误用的参数名归一化为标准名。
/// 仅对有别名键且无目标键时才替换（不覆盖已有正确值）。
/// 仅对 `FILE_PATH_TOOLS` 列表中的工具生效，避免篡改 Grep/Glob 的 `path` 参数。
fn normalize_params(tool_name: &str, input: serde_json::Value) -> serde_json::Value {
    if !FILE_PATH_TOOLS.contains(&tool_name) {
        return input;
    }

    let mut obj = match input {
        serde_json::Value::Object(map) => map,
        _ => return input,
    };

    for (alias, real) in PARAM_ALIASES {
        if obj.contains_key(*alias) && !obj.contains_key(*real) {
            let value = obj.remove(*alias).unwrap();
            obj.insert(real.to_string(), value);
            tracing::warn!(
                alias = %alias,
                resolved = %real,
                tool = %tool_name,
                "参数名别名归一化：LLM 使用了非标准参数名"
            );
        }
    }

    serde_json::Value::Object(obj)
}

/// 连续失败检测阈值：相同工具相同错误类型连续失败 ≥ 此次数时注入纠正提示。
const CONSECUTIVE_FAILURE_THRESHOLD: usize = 5;

#[derive(Debug, Clone)]
struct ConsecutiveFailureRecord {
    count: usize,
    next_threshold: usize,
}

/// 连续失败追踪器：按 (tool_name, error_kind) 聚合统计连续错误，支持指数退避避免频繁刷屏。
#[derive(Debug, Default)]
pub(crate) struct ConsecutiveFailureTracker {
    records: HashMap<(String, ToolErrorKind), ConsecutiveFailureRecord>,
}

impl ConsecutiveFailureTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次错误，返回 (当前连续次数, 是否达到警告阈值)
    pub fn record_failure(&mut self, tool_name: &str, kind: ToolErrorKind) -> (usize, bool) {
        let key = (tool_name.to_string(), kind);
        let record = self.records.entry(key).or_insert(ConsecutiveFailureRecord {
            count: 0,
            next_threshold: CONSECUTIVE_FAILURE_THRESHOLD,
        });
        record.count += 1;
        let count = record.count;
        let should_warn = if record.count >= record.next_threshold {
            record.next_threshold *= 2;
            true
        } else {
            false
        };
        (count, should_warn)
    }

    /// 工具调用成功时重置该工具的所有失败记录
    pub fn reset_tool(&mut self, tool_name: &str) {
        self.records.retain(|(t, _), _| t != tool_name);
    }

    /// 获取特定工具和错误类型的连续失败次数
    #[allow(dead_code)]
    pub fn failure_count(&self, tool_name: &str, kind: ToolErrorKind) -> usize {
        self.records
            .get(&(tool_name.to_string(), kind))
            .map(|r| r.count)
            .unwrap_or(0)
    }
}

/// 连续相同动作签名检测阈值
const CONSECUTIVE_ACTION_THRESHOLD: usize = 3;

/// 动作签名循环检测器：检测连续相同工具动作（无论成功与否），防止无效重复动作。
#[derive(Debug, Default)]
pub(crate) struct ActionLoopDetector {
    last_signature: Option<String>,
    repeat_count: usize,
}

impl ActionLoopDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录当前步的动作签名，返回 (当前连续次数, 是否达到循环阈值)
    pub fn record(&mut self, signature: &str) -> (usize, bool) {
        if let Some(ref last) = self.last_signature {
            if last == signature {
                self.repeat_count += 1;
            } else {
                self.last_signature = Some(signature.to_string());
                self.repeat_count = 1;
            }
        } else {
            self.last_signature = Some(signature.to_string());
            self.repeat_count = 1;
        }

        let is_loop = self.repeat_count >= CONSECUTIVE_ACTION_THRESHOLD;
        (self.repeat_count, is_loop)
    }
}

/// 将 JSON 对象的键递归排序，生成规范化的键序无关 JSON。
fn canonicalize_json(val: &serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::Object(map) => {
            let mut sorted: std::collections::BTreeMap<String, serde_json::Value> =
                std::collections::BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k.clone(), canonicalize_json(v));
            }
            serde_json::to_value(sorted).unwrap_or_else(|_| val.clone())
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonicalize_json).collect())
        }
        _ => val.clone(),
    }
}

/// 计算当前步的动作签名（工具名 + 键序无关的参数序列化，排除 thinking/reasoning）。
pub(crate) fn compute_step_action_signature(tool_calls: &[ToolCall]) -> String {
    if tool_calls.is_empty() {
        return String::new();
    }
    let mut parts = Vec::with_capacity(tool_calls.len());
    for tc in tool_calls {
        let canon_input = canonicalize_json(&tc.input);
        let input_str = serde_json::to_string(&canon_input).unwrap_or_default();
        parts.push(format!("{}:{}", tc.name, input_str));
    }
    parts.join(";")
}

/// 工具特征参数 → 建议工具名映射表，用于启发式工具错配诊断。
/// 每项 (特征参数集, 建议工具名)：当输入恰好包含全部特征参数时，提示可能错选了工具。
///
/// 表必须**双向完备**：每个可被误选的工具都要有自己的条目，否则模型在一侧收到
/// 提示、在另一侧收不到，会在两个工具间来回震荡（见 issue #404）。
const TOOL_SIGNATURE_HINTS: &[(&[&str], &str)] = &[
    (&["url", "prompt"], "WebFetch"),
    // WebSearch 独有参数：补全反向条目，避免参数打给别的工具时静默无提示
    (&["query"], "WebSearch"),
    (&["num_results"], "WebSearch"),
    (&["command"], "Bash"),
    (&["pattern"], "Grep"),
    (&["file_path"], "Read"),
    (&["old_string", "new_string"], "Edit"),
    (&["content", "file_path"], "Write"),
    (&["prompt", "description"], "Agent"),
];

/// 自动改道白名单：只有当 LLM 调用的工具**不在白名单**、但参数 100% 符合
/// 白名单中某个工具的 schema（且仅符合这一个）时，才静默把调用改到该只读工具上执行。
///
/// 安全边界：白名单只包含只读工具（Grep/Glob/Read），改道不会把参数指向任何写/执行类工具。
/// 方向永远是"降级到只读"。
///
/// `Bash` 是**唯一**的例外（见 [`try_auto_rescue`] 内的 Bash 分支）：执行类工具
/// 只有在 schema 匹配**且**命令经 [`is_readonly_shell_command`] 判定为只读查看时
/// 才允许改道，因此仍不越过「不把参数引向写/执行类工具」的边界。
const AUTO_RESCUE_WHITELIST: &[&str] = &["Grep", "Glob", "Read"];

/// 尝试把一次 Schema 校验失败的调用自动改道到白名单只读工具执行。
///
/// 判定条件（「参数 100% 符合唯一工具」的字面实现）：
/// 1. 输入为 JSON 对象且非空；
/// 2. 对白名单逐个**按候选自身的 ParamSpec 重新归一化** `raw_input` 后再跑严格
///    schema 校验，**恰好一个**工具校验通过；
///    —— 复用 validate_against_schema 的 Strict 语义：缺必填参数、多余参数、类型不符
///    都不算「符合」；0 个或 ≥2 个通过（如 `{pattern, path}` 同时符合 Grep 与 Glob）
///    属歧义/不匹配，不改道；
/// 3. 目标工具已注册；
/// 4. 原工具与改道工具不同。
///
/// 必须用 `raw_input`（模型原始方言入参）而非已归一化的 input：入站归一化按模型
/// **声明的工具名**查改名规则，名字写错时规则也套错（dsh 下 grep 的 `include`
/// 会因按 glob 的规则处理而残留），按候选重解才能不依赖模型是否写对名字。
///
/// 满足条件时返回 `(新工具名，按候选规则归一化后的参数)`，并在 trace 层记录
/// （不影响工具结果、不触发熔断计数）。
///
/// # Bash 例外（受只读判定约束）
///
/// 模型把 Bash 参数（`{command, description}`）发给只读工具是高频错配。此处额外允许
/// 改道到 `Bash`，但**必须同时满足**：schema 严格匹配，且 `command` 经
/// [`is_readonly_shell_command`] 判定为只读查看（`ls`/`grep`/`sed -n` 等）。
/// 因此改道后的命令不会写盘、不会执行任意程序，仍守住「只读」边界；
/// 判定属启发式而非沙箱，调用方不可据此替代审批环节。
fn try_auto_rescue(
    kernel: Kernel,
    tool_name: &str,
    raw_input: &serde_json::Value,
    all_tools: &HashMap<String, &dyn BaseTool>,
) -> Option<(String, serde_json::Value)> {
    let input_map = raw_input.as_object()?;

    if input_map.is_empty() {
        return None;
    }

    // Bash 是否构成候选：仅当命令被判定为只读查看时才纳入，与只读工具共用「唯一」判定。
    let bash_eligible = tool_name != "Bash"
        && all_tools.contains_key("Bash")
        && raw_input
            .get("command")
            .and_then(serde_json::Value::as_str)
            .is_some_and(is_readonly_shell_command);

    let candidates = AUTO_RESCUE_WHITELIST
        .iter()
        .copied()
        .chain(bash_eligible.then_some("Bash"));

    let mut matched: Option<(String, serde_json::Value)> = None;
    for candidate in candidates {
        // 原工具与目标相同不构成改道
        if tool_name.eq_ignore_ascii_case(candidate) {
            continue;
        }
        // 目标工具必须已注册
        let Some(candidate_tool) = all_tools.get(candidate) else {
            continue;
        };
        // 只放宽「内核方言改名」这一层：方言名按**候选自身**的 ParamSpec 重解，使判定
        // 不依赖模型是否写对了工具名（入站归一化绑定在声明的工具名上，写错就套错规则——
        // 如 dsh 下 grep 的 `include` 会因按 glob 的“不改名”规则处理而残留）。
        // `path→file_path` 这类**参数别名**仍按模型声明的工具名施加，与改道前完全一致，
        // 避免因换候选而把参数引向原本不会命中的工具（凭空扩大改道面）。
        let candidate_input =
            normalize_params(tool_name, kernel.normalize_input(candidate, raw_input));
        if validate_against_schema(&candidate_input, &candidate_tool.parameters()).is_ok() {
            // 第二个命中 = 歧义输入，按「唯一」语义放弃改道
            if matched.is_some() {
                tracing::trace!(
                    tool.from = tool_name,
                    params = ?input_map.keys().collect::<Vec<_>>(),
                    "自动改道放弃：参数同时符合多个候选工具（歧义）"
                );
                return None;
            }
            matched = Some((candidate.to_string(), candidate_input));
        }
    }

    matched.map(|(name, input)| {
        tracing::trace!(
            tool.from = tool_name,
            tool.to = %name,
            "自动改道：LLM 调用了 '{}'，但参数 100% 符合唯一候选工具 '{}'",
            tool_name,
            name
        );
        (name, input)
    })
}

/// 阶段零的改道包装：schema 校验失败的调用改道到唯一候选工具，并**替换调用身份**。
///
/// 只有 schema 校验失败才尝试改道（校验通过 = 参数本就合法，无需纠正）；改道失败
/// 或目标工具未注册时原样返回，交由后续阶段按原有错误路径处理。
///
/// 替换 `name` 是有意为之：事件（ToolPending/ToolStart/ToolEnd）、HITL 审批、
/// 熔断归因都读 `ToolCall.name`，只有在此处一次改对，下游才会一致地把调用
/// 当作真正执行的工具看待。返回给 LLM 的历史不受影响——`dispatch_tools` 的 AI 消息
/// 由 `reasoning.tool_calls`（模型原始声明）构造，tool_result 仅按 `tool_call_id` 关联。
fn rescue_mismatched_call<L: ReactLLM, S: State>(
    agent: &ReActAgent<L, S>,
    call: ToolCall,
    raw_input: Option<&serde_json::Value>,
    all_tools: &HashMap<String, &dyn BaseTool>,
) -> ToolCall {
    let Some(tool) = resolve_tool(&call.name, all_tools) else {
        return call;
    };
    let input = normalize_params(&call.name, call.input.clone());
    if validate_against_schema(&input, &tool.parameters()).is_ok() {
        return call;
    }
    // 改道判定用模型原始方言入参（`normalize_reasoning` 在归一化前留存），
    // 这样候选工具各自的改名规则才能正确套用；缺失时回退到已归一化的入参。
    let raw_input = raw_input.unwrap_or(&input);
    let Some((new_name, new_input)) =
        try_auto_rescue(agent.kernel, &call.name, raw_input, all_tools)
    else {
        return call;
    };
    if !all_tools.contains_key(&new_name) {
        return call;
    }
    tracing::warn!(
        tool.from = %agent.kernel.echo_name(&call.name),
        tool.to = %agent.kernel.echo_name(&new_name),
        call_id = %call.id,
        "Schema 校验失败后自动改道执行：{} -> {}",
        agent.kernel.echo_name(&call.name),
        agent.kernel.echo_name(&new_name)
    );
    ToolCall {
        name: new_name,
        input: new_input,
        ..call
    }
}

/// 单工具 Schema 校验失败阈值：滑动窗口内失败 ≥ 此次数时注入强提示。
///
/// 注意是**滑动窗口**而非“连续”：模型常在多个工具间交替犯错（配错一次 WebSearch、
/// 配错一次 WebFetch、再配错一次 WebSearch），若要求连续则计数永远够不到阈值。
const SCHEMA_FAILURE_CIRCUIT_BREAKER_THRESHOLD: usize = 2;

/// 跨工具聚合 Schema 校验失败阈值：滑动窗口内跨工具失败 ≥ 此次数时触发聚合熔断。
const AGGREGATE_SCHEMA_FAILURE_THRESHOLD: usize = 3;

/// 滑动窗口容量：保留最近 N 次工具调用的结果（成功或失败），单工具与跨工具计数均由此派生。
const AGGREGATE_WINDOW_SIZE: usize = 6;

/// 窗口内一次工具调用的结果（成功 / 指定类型的失败）。
#[derive(Debug, Clone)]
struct ToolCallOutcome {
    tool_name: String,
    is_failure: bool,
}

/// Schema 校验熔断事件
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SchemaBreakerEvent {
    /// 单工具窗口内 Schema 校验失败熔断
    PerTool { tool_name: String, count: usize },
    /// 跨工具窗口内 Schema 校验失败熔断
    Aggregate {
        failed_tools: Vec<String>,
        count: usize,
    },
}

/// 单工具的窗口内失败状态。
#[derive(Debug, Clone)]
struct ToolFailureState {
    /// 下一次触发熔断的窗口内失败次数阈值（指数退避，避免同一模式反复刷屏）
    next_threshold: usize,
}

impl Default for ToolFailureState {
    fn default() -> Self {
        Self {
            next_threshold: SCHEMA_FAILURE_CIRCUIT_BREAKER_THRESHOLD,
        }
    }
}

/// Schema 校验失败追踪器：在滑动窗口内检测单工具及跨工具的 Schema 校验失败，
/// 注入强提示阻断循环。
///
/// 计数口径是「窗口内失败次数」而非「连续失败次数」：真实失败模式以**交替错配 +
/// 中间夹成功**为主，连续计数会被成功调用反复清零而永不触发（见 issue #404）。
#[derive(Debug)]
pub(crate) struct SchemaFailureTracker {
    /// 工具名 → 该工具的退避阈值状态
    tool_states: HashMap<String, ToolFailureState>,
    /// 最近的工具调用结果（成功 / 失败），滑动窗口，成功也参与淘汰
    call_window: VecDeque<ToolCallOutcome>,
    /// 跨工具聚合下一次触发熔断的窗口内失败次数阈值（指数退避）
    aggregate_next_threshold: usize,
    /// 历史累计出现过 Schema 校验失败的工具（按首次失败顺序，不受滑动窗口淘汰影响）
    seen_failure_tools: Vec<String>,
    /// 跨工具首现提示是否已发出（全局仅一次）
    cross_tool_hint_emitted: bool,
    /// 待消费的跨工具首现提示（刚达到第 2 个不同工具时写入）
    pending_cross_tool_hint: Option<Vec<String>>,
}

impl Default for SchemaFailureTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemaFailureTracker {
    pub fn new() -> Self {
        Self {
            tool_states: HashMap::new(),
            call_window: VecDeque::new(),
            aggregate_next_threshold: AGGREGATE_SCHEMA_FAILURE_THRESHOLD,
            seen_failure_tools: Vec::new(),
            cross_tool_hint_emitted: false,
            pending_cross_tool_hint: None,
        }
    }

    /// 记录一次 Schema 校验失败，返回 (该工具窗口内失败次数, 是否触发熔断)
    #[allow(dead_code)]
    pub fn record_failure(&mut self, tool_name: &str) -> (usize, bool) {
        let (count, event) = self.record_failure_detailed(tool_name);
        (count, event.is_some())
    }

    /// 详细记录一次 Schema 校验失败，返回 (该工具窗口内失败次数, 触发的熔断事件)
    pub fn record_failure_detailed(
        &mut self,
        tool_name: &str,
    ) -> (usize, Option<SchemaBreakerEvent>) {
        self.push_outcome(tool_name, true);

        // 跨工具首现：历史累计第 2 个不同工具出现 Schema 失败时，产生一次性「模式点破」提示。
        // 与聚合熔断分层——熔断从第 3 次起强制止动，这里让模型在第 2 次就感知趋势。
        // 用历史累计而非滑动窗口：窗口淘汰后不应把同一个模式重复当作「首现」。
        if !self.cross_tool_hint_emitted && !self.seen_failure_tools.iter().any(|t| t == tool_name)
        {
            self.seen_failure_tools.push(tool_name.to_string());
            if self.seen_failure_tools.len() >= 2 {
                self.cross_tool_hint_emitted = true;
                self.pending_cross_tool_hint = Some(self.seen_failure_tools.clone());
            }
        }

        // 单工具：统计该工具在窗口内的失败次数（而非“连续失败”）
        let per_tool_count = self
            .call_window
            .iter()
            .filter(|o| o.tool_name == tool_name && o.is_failure)
            .count();

        let state = self.tool_states.entry(tool_name.to_string()).or_default();
        let per_tool_triggered = if per_tool_count >= state.next_threshold {
            state.next_threshold *= 2;
            true
        } else {
            false
        };

        // 跨工具聚合：窗口内涉及多个工具时才计数，避免单工具冒充聚合熔断
        let window_failures = self.window_failure_count();
        let distinct_failed = self.distinct_failed_tools();
        let aggregate_triggered =
            if distinct_failed.len() > 1 && window_failures >= self.aggregate_next_threshold {
                if self.aggregate_next_threshold == 0 {
                    self.aggregate_next_threshold = AGGREGATE_SCHEMA_FAILURE_THRESHOLD;
                }
                self.aggregate_next_threshold *= 2;
                true
            } else {
                false
            };

        // 优先单工具熔断，次选跨工具聚合熔断
        let event = if per_tool_triggered {
            Some(SchemaBreakerEvent::PerTool {
                tool_name: tool_name.to_string(),
                count: per_tool_count,
            })
        } else if aggregate_triggered {
            Some(SchemaBreakerEvent::Aggregate {
                failed_tools: distinct_failed,
                count: window_failures,
            })
        } else {
            None
        };

        (per_tool_count, event)
    }

    /// 取走「跨工具首现」提示：历史累计第 2 个不同工具首次 Schema 失败后仅返回一次。
    pub fn take_cross_tool_first_hint(&mut self) -> Option<Vec<String>> {
        self.pending_cross_tool_hint.take()
    }

    /// 工具调用成功：复位该工具的退避阈值，并把成功记入窗口参与淘汰。
    ///
    /// **不清空窗口**——否则「失败之间夹成功」的真实序列会反复归零窗口，
    /// 使熔断永不触发（见 issue #404）。成功调用占用窗口位，推动旧失败自然淘汰。
    pub fn reset(&mut self, tool_name: &str) {
        self.tool_states.remove(tool_name);
        self.push_outcome(tool_name, false);
    }

    /// 推入窗口并按容量淘汰过期项。窗口一经淘汰，被淘汰的失败即不再计入任何计数。
    fn push_outcome(&mut self, tool_name: &str, is_failure: bool) {
        self.call_window.push_back(ToolCallOutcome {
            tool_name: tool_name.to_string(),
            is_failure,
        });
        while self.call_window.len() > AGGREGATE_WINDOW_SIZE {
            self.call_window.pop_front();
        }
    }

    /// 窗口内失败总数
    fn window_failure_count(&self) -> usize {
        self.call_window.iter().filter(|o| o.is_failure).count()
    }

    /// 窗口内失败涉及的（保序去重）工具名
    fn distinct_failed_tools(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for o in self.call_window.iter().filter(|o| o.is_failure) {
            if !seen.iter().any(|t| t == &o.tool_name) {
                seen.push(o.tool_name.clone());
            }
        }
        seen
    }

    /// 获取单工具当前窗口内失败次数
    #[allow(dead_code)]
    pub fn tool_failure_count(&self, tool_name: &str) -> usize {
        self.call_window
            .iter()
            .filter(|o| o.tool_name == tool_name && o.is_failure)
            .count()
    }

    /// 获取跨工具聚合窗口内失败次数
    #[allow(dead_code)]
    pub fn aggregate_failure_count(&self) -> usize {
        self.window_failure_count()
    }
}

/// 工具错配启发式诊断：检测输入参数特征并建议可能的正确工具。
///
/// 当 `tool_name` 不匹配特征参数表中某项的目标工具、但输入恰好包含该项全部特征参数时，
/// 返回 `💡 Did you mean to use '<suggested>'?` 提示。
pub(crate) fn suggest_tool_mismatch(tool_name: &str, input: &serde_json::Value) -> Option<String> {
    let input_map = input.as_object()?;
    let input_keys: Vec<&str> = input_map.keys().map(|k| k.as_str()).collect();

    for (signature_keys, suggested_tool) in TOOL_SIGNATURE_HINTS {
        // 仅当调用的不是建议工具本身时才提示
        if tool_name.eq_ignore_ascii_case(suggested_tool) {
            continue;
        }
        // 检查输入是否包含全部特征参数
        let all_present = signature_keys
            .iter()
            .all(|sig_key| input_keys.contains(sig_key));
        if all_present {
            return Some(format!(
                "💡 Did you mean to use '{suggested_tool}'? The parameters {signature_keys:?} are characteristic of the {suggested_tool} tool."
            ));
        }
    }
    None
}

/// 工具名解析：精确匹配 → 大小写无关匹配 → 语义别名。
fn resolve_tool<'a>(
    name: &str,
    all_tools: &HashMap<String, &'a dyn BaseTool>,
) -> Option<&'a dyn BaseTool> {
    // 1. 精确匹配
    if let Some(tool) = all_tools.get(name).copied() {
        return Some(tool);
    }
    // 2. 大小写无关匹配
    for (key, tool) in all_tools {
        if key.eq_ignore_ascii_case(name) {
            return Some(*tool);
        }
    }
    // 3. 语义别名
    for (alias, real_name) in TOOL_ALIASES {
        if name.eq_ignore_ascii_case(alias) {
            if let Some(tool) = all_tools.get(*real_name).copied() {
                tracing::debug!(alias = %name, resolved = %real_name, "工具名别名匹配");
                return Some(tool);
            }
        }
    }
    None
}

/// 构造 ToolResult 对应的 BaseMessage：记录实际产生该结果的工具名，并把诊断提示
/// 并入其结果正文末尾。
///
/// `tool_name` 记录**实际执行**的工具名（自动改道后与 AI 消息的声明名不同），
/// 展示层以它为准，避免「声明名 + 改道后输出」杂交体；LLM 历史口径不受影响。
fn build_tool_msg_with_hints(result: &ToolResult, hints: &[String]) -> BaseMessage {
    let mut msg = build_tool_msg_body(result, hints);
    if let BaseMessage::Tool { tool_name, .. } = &mut msg {
        *tool_name = Some(result.tool_name.clone());
    }
    msg
}

/// 构造 ToolResult 对应的 BaseMessage，并把诊断提示并入其结果正文末尾。
///
/// 为何并入正文而非新增独立消息（issue #495）：
/// - 独立 `BaseMessage::System` 会被 Anthropic/OpenAI 适配器抽到顶层 `system` 字段，
///   脱离对话流末端，模型在自回归解码时看不到该提示，熔断形同虚设；
/// - 同时会被 `StateSnapshot`/`/export` 的 `!is_system()` 过滤掉，排查时死无对证。
///
/// 并入 ToolResult 正文可同时满足：① 留在出错结果旁，位置精确；② 模型上下文末端可见；
/// ③ 导出可见（`BaseMessage::Tool` 不受 system 过滤影响）；④ 不新增消息，零 UI 污染。
fn build_tool_msg_body(result: &ToolResult, hints: &[String]) -> BaseMessage {
    let suffix = hints.join("\n");

    if result.is_error {
        // 错误结果：正文即纯文本，直接追加
        let mut body = result.output.clone();
        if !suffix.is_empty() {
            body.push('\n');
            body.push_str(&suffix);
        }
        return BaseMessage::tool_error(&result.tool_call_id, body);
    }

    if let Some(ref content) = result.content {
        // 结构化内容（如图片）：仅在尾部追加文本，保留原始多模态 block
        if suffix.is_empty() {
            return BaseMessage::tool_result(&result.tool_call_id, content.clone());
        }
        let mut blocks = content.content_blocks();
        blocks.push(ContentBlock::text(suffix));
        return BaseMessage::tool_result(&result.tool_call_id, MessageContent::blocks(blocks));
    }

    let mut body = result.output.clone();
    if !suffix.is_empty() {
        body.push('\n');
        body.push_str(&suffix);
    }
    BaseMessage::tool_result(&result.tool_call_id, body)
}

/// 工具审批 → 并发执行 → 结果收集（不写 state）→ 统一写入
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_tools<L: ReactLLM, S: State>(
    agent: &ReActAgent<L, S>,
    state: &mut S,
    reasoning: &Reasoning,
    all_tools: &HashMap<String, &dyn BaseTool>,
    cancel: &CancellationToken,
    consecutive_failure_tracker: &mut ConsecutiveFailureTracker,
    action_loop_detector: &mut ActionLoopDetector,
    schema_failure_tracker: &mut SchemaFailureTracker,
) -> AgentResult<Vec<(ToolCall, ToolResult)>> {
    let tc_reqs: Vec<ToolCallRequest> = reasoning
        .tool_calls
        .iter()
        .map(|tc| ToolCallRequest::new(tc.id.clone(), tc.name.clone(), tc.input.clone()))
        .collect();
    let ai_msg = reasoning
        .source_message
        .clone()
        .unwrap_or_else(|| BaseMessage::ai_with_tool_calls(reasoning.thought.clone(), tc_reqs));
    let ai_msg_id = ai_msg.id();

    // emit AI 工具前文本（非流式；流式模式下 LLM 适配器已通过 StreamingContext emit）
    if !reasoning.streamed && !reasoning.thought.trim().is_empty() {
        agent.emit(AgentEvent::TextChunk {
            message_id: ai_msg_id,
            chunk: reasoning.thought.clone(),
            source_agent_id: None,
        });
    }

    // 阶段 A：收集所有工具调用结果（不写 state）
    // 返回 Err 仅在 before_tool 错误路径（此时 state 干净，无 AI 消息）
    tracing::debug!(
        "[DEADLOCK] dispatch_tools: {} tool calls to dispatch, names={:?}",
        reasoning.tool_calls.len(),
        reasoning
            .tool_calls
            .iter()
            .map(|tc| tc.name.as_str())
            .collect::<Vec<_>>()
    );
    let (results, was_cancelled, deferred_error) = collect_tool_results(
        agent,
        state,
        reasoning.tool_calls.clone(),
        all_tools,
        cancel,
        ai_msg_id,
        reasoning.raw_tool_calls.as_deref().unwrap_or(&[]),
    )
    .await?;

    tracing::debug!(
        "[DEADLOCK] dispatch_tools: collect_tool_results done, {} results, was_cancelled={}, deferred={}",
        results.len(), was_cancelled, deferred_error.is_some()
    );

    // 阶段 B：一次性写入 state（Cancel / deferred_error 路径也写入，保证 state 一致）
    agent.emit(AgentEvent::MessageAdded(ai_msg.clone()));
    state.add_message(ai_msg);

    // 动作签名循环检测：连续相同动作（即使成功但无效）注入纠正提示。
    // 提示并入 ToolResult 正文（见 append_hints_to_tool_msg），不再新增独立 system 消息：
    // system 消息会被 Anthropic/OpenAI 适配器抽到顶层 system 字段，脱离对话流末端而失效；
    // 且会被 StateSnapshot/导出按 is_system() 过滤掉（issue #495）。
    let step_sig = compute_step_action_signature(&reasoning.tool_calls);
    let loop_warning: Option<String> = if step_sig.is_empty() {
        None
    } else {
        let (count, is_loop) = action_loop_detector.record(&step_sig);
        if is_loop {
            let text = format!(
                "Warning: You have executed the exact same tool call(s) {} consecutive times with identical parameters. \
                 Stop repeating this action. Analyze why the previous attempts did not produce new progress or achieve the goal. \
                 Try a completely different approach, use different tools, or ask the user for guidance.",
                count
            );
            tracing::warn!(
                signature = %step_sig,
                count = count,
                threshold = CONSECUTIVE_ACTION_THRESHOLD,
                injected = %truncate_for_log(&text),
                "连续 {} 次执行相同工具动作签名，注入纠正消息",
                count
            );
            Some(text)
        } else {
            None
        }
    };

    let result_count = results.len();

    for (idx, (_, result)) in results.iter().enumerate() {
        // 本工具结果自身的诊断提示（随结果正文下发，留在对话流末端）
        let mut hints: Vec<String> = Vec::new();

        let resolved_kind = result.resolved_error_kind();
        if let Some(kind) = resolved_kind {
            // 1. 连续失败追踪 (按 tool_name + error_kind)
            let (consecutive_count, should_warn_consecutive) =
                consecutive_failure_tracker.record_failure(&result.tool_name, kind);
            if should_warn_consecutive {
                let kind_desc = match kind {
                    ToolErrorKind::SchemaValidation => "schema validation",
                    ToolErrorKind::ToolNotFound => "tool not found",
                    ToolErrorKind::ExecutionFailed => "execution",
                    ToolErrorKind::Other => "unclassified",
                };
                let text = format!(
                    "Warning: Tool '{}' has failed {} consecutive times with {} error. \
                     Stop retrying and analyze the root cause. Consider using a different approach \
                     or asking the user for guidance.",
                    result.tool_name, consecutive_count, kind_desc
                );
                tracing::warn!(
                    tool = %result.tool_name,
                    count = consecutive_count,
                    kind = ?kind,
                    injected = %truncate_for_log(&text),
                    "连续 {} 次同类型错误，注入纠正消息",
                    consecutive_count
                );
                hints.push(text);
            }
            // 2. Schema 校验失败熔断（支持单工具快速熔断 + 跨工具聚合熔断）
            if kind == ToolErrorKind::SchemaValidation {
                let (_schema_count, breaker_event) =
                    schema_failure_tracker.record_failure_detailed(&result.tool_name);
                // 跨工具首现提醒（issue #959）：第 2 个不同工具首次参数错误时点破模式。
                // 本结果已触发熔断提示时不再追加，避免同一 ToolResult 上出现两条语义重叠的警告。
                let cross_tool_hint = schema_failure_tracker.take_cross_tool_first_hint();
                if breaker_event.is_none() {
                    if let Some(tools) = cross_tool_hint {
                        let tools_str = tools.join(", ");
                        let text = format!(
                            "You already passed invalid arguments to {} different tools ({}). \
                             Stop guessing; re-read each tool's schema before the next call.",
                            tools.len(),
                            tools_str
                        );
                        tracing::warn!(
                            tools = %tools_str,
                            injected = %truncate_for_log(&text),
                            "跨工具 Schema 校验首次失败，注入模式提醒"
                        );
                        hints.push(text);
                    }
                }
                if let Some(event) = breaker_event {
                    match event {
                        SchemaBreakerEvent::PerTool { tool_name, count } => {
                            let text = format!(
                                "⚠️ SCHEMA VALIDATION CIRCUIT BREAKER: Tool '{}' has failed schema validation {} \
                                 times within the recent window. You are passing wrong parameters repeatedly. \
                                 STOP and carefully re-read the tool's parameter schema before your next attempt. \
                                 Do NOT retry with the same parameters.",
                                tool_name, count
                            );
                            tracing::warn!(
                                tool = %tool_name,
                                schema_fail_count = count,
                                injected = %truncate_for_log(&text),
                                "Schema 校验失败 {} 次（滑动窗口内），注入单工具熔断提示",
                                count
                            );
                            hints.push(text);
                        }
                        SchemaBreakerEvent::Aggregate {
                            failed_tools,
                            count,
                        } => {
                            let tools_str = failed_tools.join(", ");
                            let text = format!(
                                "⚠️ SCHEMA VALIDATION CIRCUIT BREAKER: Schema validation failed {} \
                                 times across different tools ({}) within the recent window. You are \
                                 repeatedly passing invalid arguments. STOP guessing parameters. Carefully \
                                 inspect the parameter schema for each tool before invoking it. Do NOT call \
                                 tools with invented parameters.",
                                count, tools_str
                            );
                            tracing::warn!(
                                tools = %tools_str,
                                aggregate_count = count,
                                injected = %truncate_for_log(&text),
                                "跨工具 Schema 校验失败 {} 次（滑动窗口内），注入聚合熔断提示",
                                count
                            );
                            hints.push(text);
                        }
                    }
                }
            }
        } else {
            // 成功则重置该工具的所有失败计数与聚合计数
            consecutive_failure_tracker.reset_tool(&result.tool_name);
            schema_failure_tracker.reset(&result.tool_name);
        }

        // 循环警告（整步级，非单结果级）只承载在最后一条消息上，避免重复注入。
        // 若存在工具结果则并入其正文；否则（如全部被跳过）退化为追加用户消息。
        if idx + 1 == result_count {
            if let Some(warning) = loop_warning.as_ref() {
                hints.push(warning.clone());
            }
        }

        let tool_msg = build_tool_msg_with_hints(result, &hints);
        let tool_msg_clone = tool_msg.clone();
        state.add_message(tool_msg);
        agent.emit(AgentEvent::MessageAdded(tool_msg_clone));
    }

    // 边界：本轮没有任何工具结果（全部被 before_tool 拒绝且未产出结果）时，
    // 循环警告无 ToolResult 可承载，退化为追加用户消息留在对话流末端。
    if result_count == 0 {
        if let Some(warning) = loop_warning {
            state.add_message(BaseMessage::human(warning));
        }
    }

    // 写入完成后再返回错误
    if was_cancelled {
        tracing::warn!("[DEADLOCK] dispatch_tools: returning Interrupted (was_cancelled)");
        return Err(AgentError::Interrupted);
    }
    if let Some(msg) = deferred_error {
        tracing::warn!(
            "[DEADLOCK] dispatch_tools: returning MiddlewareError: {}",
            msg
        );
        return Err(AgentError::MiddlewareError {
            middleware: "chain".to_string(),
            reason: msg,
        });
    }

    tracing::debug!(
        "[DEADLOCK] dispatch_tools: complete, {} results",
        results.len()
    );
    Ok(results)
}

/// 执行 before_tool 审批 + 并发工具调用，收集所有结果。
///
/// **不变量**：调用期间 state 中不包含本轮 AI 消息。所有 `run_on_error` /
/// `run_after_tool` 实现均不依赖 `state.messages()` 包含本轮新增内容
/// （已验证：全部 17 个中间件的这些钩子均使用 `_state: &mut S` 模式）。
/// 新增中间件时必须遵守此约束。
///
/// 不写入 state，由 `dispatch_tools` 统一写入。
///
/// **完成通知时机**：`ToolEnd` 在每个工具 future 完成的第一时间发出，
/// 不等整批工具结束；`run_on_error` / `run_after_tool` 与结果收集仍按原顺序
/// 串行执行，state 仍由 `dispatch_tools` 一次性批量写入。
///
/// 返回 `(results, was_cancelled, deferred_error)`。
/// - 正常路径：`(results, false, None)`
/// - Cancel 路径：`(results, true, None)`
/// - after_tool 错误：`(results, false, Some(msg))`
/// - before_tool 错误 / Cancel in before_tool：返回 `Err`（state 未修改）
async fn collect_tool_results<L: ReactLLM, S: State>(
    agent: &ReActAgent<L, S>,
    state: &mut S,
    original_calls: Vec<ToolCall>,
    all_tools: &HashMap<String, &dyn BaseTool>,
    cancel: &CancellationToken,
    ai_msg_id: MessageId,
    raw_calls: &[ToolCall],
) -> AgentResult<(Vec<(ToolCall, ToolResult)>, bool, Option<String>)> {
    // 阶段零：schema 错配自动改道。**必须早于所有事件与审批**——ToolPending/ToolStart
    // 携带的工具名与参数、以及 HITL 看到的调用，都要与实际执行的工具一致；否则界面会
    // 显示「原名 + 改道后参数」的杂交体，审批也会因原名不在敏感名单而静默放行。
    //
    // 改道判定需要**模型原始方言入参**（`reasoning.raw_tool_calls`）：入站归一化按模型
    // 声明的工具名查改名规则，名字写错时规则也套错，只有原始入参才能按候选工具各自的
    // 规则正确重解。按 `tool_call_id` 配对（`normalize_reasoning` 不增删调用）。
    let original_calls: Vec<ToolCall> = original_calls
        .into_iter()
        .map(|call| {
            let raw_input = raw_calls
                .iter()
                .find(|rc| rc.id == call.id)
                .map(|rc| &rc.input);
            rescue_mismatched_call(agent, call, raw_input, all_tools)
        })
        .collect();

    let mut ready_calls: Vec<ToolCall> = Vec::with_capacity(original_calls.len());
    let mut settled_results: Vec<(ToolCall, ToolResult)> = Vec::new();

    // 所有工具先进入排队态；审批期间不把本轮 AI 消息写入 state。
    for call in &original_calls {
        agent.emit(AgentEvent::ToolPending {
            message_id: ai_msg_id,
            tool_call_id: call.id.clone(),
            name: call.name.clone(),
            input: call.input.clone(),
            source_agent_id: None,
        });
    }
    // before_tool 出错或被取消时，尚未执行的工具也必须闭合，不能残留 Waiting。
    // 已拒绝的调用有自己的 ToolEnd，避免重复发送终态。
    let close_pending = |output: &str, settled: &[(ToolCall, ToolResult)]| {
        for call in &original_calls {
            if settled.iter().any(|(closed, _)| closed.id == call.id) {
                continue;
            }
            agent.emit(AgentEvent::ToolEnd {
                message_id: ai_msg_id,
                tool_call_id: call.id.clone(),
                name: call.name.clone(),
                output: output.to_string(),
                is_error: true,
                source_agent_id: None,
            });
        }
    };

    // 阶段一：批量 before_tool
    let before_results = tokio::select! {
        biased;
        // 审批可能一直等待用户回应，取消必须能打断 before_tool；此时尚未写入 state。
        _ = cancel.cancelled() => {
            close_pending("Interrupted by user", &settled_results);
            return Err(AgentError::Interrupted);
        },
        results = agent.chain.run_before_tools_batch(state, original_calls.clone()) => results,
    };

    for (tool_call, before_result) in original_calls.iter().zip(before_results) {
        // before_tool 阶段也检查取消
        if cancel.is_cancelled() {
            close_pending("Interrupted by user", &settled_results);
            return Err(AgentError::Interrupted);
        }
        match before_result {
            Ok(modified_call) => {
                let file_snapshot = resolve_tool(&modified_call.name, all_tools).and_then(|tool| {
                    tool.file_snapshot(&normalize_params(
                        &modified_call.name,
                        modified_call.input.clone(),
                    ))
                });
                agent.emit(AgentEvent::ToolStart {
                    message_id: ai_msg_id,
                    tool_call_id: modified_call.id.clone(),
                    name: modified_call.name.clone(),
                    input: modified_call.input.clone(),
                    file_snapshot,
                    source_agent_id: None,
                });
                ready_calls.push(modified_call);
            }
            Err(AgentError::ToolRejected { ref reason, .. }) => {
                let rejection_result =
                    ToolResult::error(&tool_call.id, &tool_call.name, reason.clone());
                agent.emit(AgentEvent::ToolEnd {
                    message_id: ai_msg_id,
                    tool_call_id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    output: rejection_result.output.clone(),
                    is_error: true,
                    source_agent_id: None,
                });
                settled_results.push((tool_call.clone(), rejection_result));
            }
            Err(e) => {
                close_pending(&e.to_string(), &settled_results);
                let _ = agent.chain.run_on_error(state, &e).await;
                return Err(e);
            }
        }
    }

    // 阶段二：所有工具并发执行。每个 future 在自身完成的瞬间就构造结果并
    // 立即 emit ToolEnd，不等同批其他工具（如长跑 Bash）结束，避免快工具的
    // 完成状态被整批 join 拖住。
    // SubAgent 通过 child_handler_factory 的独立 event handler 避免
    // 共享 Langfuse Mutex 的锁竞争，LLM 流式支持取消令牌中断。
    let tool_results: Vec<(ToolResult, Option<AgentError>)> = {
        let futures: Vec<_> = ready_calls
            .iter()
            .map(|call| {
                let finish_call = call.clone();
                let tool_name = call.name.clone();
                let call_id = call.id.clone();
                let input = call.input.clone();
                let input = normalize_params(&tool_name, input);
                let kernel = agent.kernel;
                let tool = resolve_tool(&call.name, all_tools);
                // 执行入口 fail-closed：可见性只控制发给 LLM 的定义，模型手写
                // 未暴露的管辖工具名时也必须拒绝；非管辖工具不受此限。
                let tool = tool
                    .filter(|t| !(kernel.governs(t.name()) && !kernel.is_exposed(t.name())));
                let cancel = cancel.clone();
                let invocation = crate::tools::ToolInvocationContext {
                    tool_call_id: call_id.clone(),
                    source_agent_id: state.get_context("source_agent_id").map(str::to_string),
                };
                let output_context = agent.event_handler.as_ref().map(|handler| {
                    crate::tools::ToolOutputContext::new(
                        ai_msg_id,
                        call_id.clone(),
                        invocation.source_agent_id.clone(),
                        Arc::clone(handler),
                    )
                });
                async move {
                    let span = tracing::info_span!(
                        "agent.tool_call",
                        tool.name = %tool_name,
                        tool.call_id = %call_id,
                    );
                    let _enter = span.enter();
                    let invoke_fut = async {
                        let echo = kernel.echo_name(&tool_name);
                        match tool {
                            Some(t) => {
                                let schema = t.parameters();
                                let (tool_to_run, input_to_run) = if let Err(msg) = validate_against_schema(&input, &schema) {
                                    // 改道已在阶段零（rescue_mismatched_call）处理完毕，
                                    // 走到这里说明无候选工具可改道：按原路径返回结构化参数错误。
                                    let keys: Vec<String> = match &input {
                                        serde_json::Value::Object(map) => {
                                            let mut k: Vec<_> = map.keys().cloned().collect();
                                            k.sort();
                                            k
                                        }
                                        _ => Vec::new(),
                                    };
                                    // 校验用的是规范 schema；反馈给模型前把参数名转成方言名，
                                    // 避免 pi 下模型收到「要求修正 file_path」而 schema 写的是 path。
                                    let msg = if kernel.is_cc() {
                                        msg
                                    } else {
                                        kernel.dialectize_param_names(&msg).into_owned()
                                    };
                                    let keys_text = if kernel.is_cc() {
                                        format!("{keys:?}")
                                    } else {
                                        kernel
                                            .dialectize_param_names(&format!("{keys:?}"))
                                            .into_owned()
                                    };
                                    // 启发式工具错配诊断：cc 沿用静态表（逐字节等价），
                                    // 其他内核用按方言生成的 signature_hints。
                                    let hint = (if kernel.is_cc() {
                                        suggest_tool_mismatch(&tool_name, &input)
                                    } else {
                                        kernel.suggest_mismatch(&tool_name, &input)
                                    })
                                    .map(|h| format!("\n{h}"))
                                    .unwrap_or_default();
                                    return Err(AgentError::ToolSchemaValidationFailed {
                                        tool: tool_name.clone(),
                                        reason: format!(
                                            "Invalid arguments for tool {echo}:\n{msg}\n\
                                             Received keys: {keys_text}. Rewrite the call to satisfy the schema and retry.{hint}"
                                        ),
                                    });
                                } else {
                                    (t, input)
                                };
                                crate::tools::ToolOutputContext::scope_invocation(
                                    output_context,
                                    invocation.scope_with_kernel(kernel, tool_to_run.invoke_content(input_to_run)),
                                ).await.map_err(|e| {
                                    // 代理工具的目标参数错误保留结构化分类与目标名，
                                    // 避免退化为普通执行失败而绕过 Schema 纠错。
                                    if let Some(AgentError::ToolSchemaValidationFailed { tool, reason }) =
                                        e.downcast_ref::<AgentError>()
                                    {
                                        AgentError::ToolSchemaValidationFailed {
                                            tool: tool.clone(),
                                            reason: reason.clone(),
                                        }
                                    } else {
                                        AgentError::ToolExecutionFailed {
                                            tool: tool_name.clone(),
                                            reason: e.to_string(),
                                        }
                                    }
                                })
                            }
                            None => Err(AgentError::ToolNotFound(echo.to_string())),
                        }
                    };
                    let raw_result = tokio::select! {
                        biased;
                        _ = cancel.cancelled() => {
                            Err(AgentError::ToolExecutionFailed {
                                tool: tool_name,
                                reason: "Interrupted by user".to_string(),
                            })
                        }
                        result = invoke_fut => result,
                    };
                    let (result, raw_error) = finalize_tool_result(&finish_call, raw_result);
                    // 工具一完成就闭合展示态，不等整批 join_all 返回。
                    agent.emit(AgentEvent::ToolEnd {
                        message_id: ai_msg_id,
                        tool_call_id: finish_call.id.clone(),
                        name: finish_call.name.clone(),
                        output: result.output.clone(),
                        is_error: result.is_error,
                        source_agent_id: None,
                    });
                    (result, raw_error)
                }
            })
            .collect();
        futures::future::join_all(futures).await
    };

    let was_cancelled = cancel.is_cancelled();

    // 阶段三：串行处理结果——所有 tool_result 收集到 results 中，
    // 不写 state，由 dispatch_tools 统一写入。
    // 工具执行错误不终止循环——错误 ToolResult 收集后由 LLM 下一轮修正。
    // after_tool 中间件错误收集到 deferred_error。
    // ToolEnd 已在阶段二各工具完成时立即发出，此处只做串行后处理，不重复 emit。
    let mut deferred_error: Option<String> = None;
    let mut exec_results: Vec<(ToolCall, ToolResult)> = Vec::with_capacity(ready_calls.len());

    for (modified_call, (result, raw_error)) in ready_calls.into_iter().zip(tool_results) {
        // 原始错误按既有分支交给中间件（ToolNotFound 原路径不调用）。
        if let Some(error) = raw_error {
            let _ = agent.chain.run_on_error(state, &error).await;
        }

        if result.is_error {
            tracing::warn!(
                tool.name = %result.tool_name,
                tool.is_error = true,
                error_len = result.output.len(),
                error = %truncate_for_log(&result.output),
                "tool call failed"
            );
        }

        if modified_call.name == "Agent" {
            tracing::debug!(
                "[DEADLOCK] dispatch: about to run_after_tool for Agent, call_id={}",
                modified_call.id
            );
        }
        if let Err(e) = agent
            .chain
            .run_after_tool(state, &modified_call, &result)
            .await
        {
            let _ = agent.chain.run_on_error(state, &e).await;
            deferred_error = deferred_error.or(Some(e.to_string()));
        }
        if modified_call.name == "Agent" {
            tracing::debug!(
                "[DEADLOCK] dispatch: run_after_tool for Agent completed, call_id={}",
                modified_call.id
            );
        }

        exec_results.push((modified_call, result));
    }

    // 合并 settled（rejected）+ executed 结果
    settled_results.extend(exec_results);

    // Cancel / deferred_error 不在此返回 Err，由 dispatch_tools 在写入 state 后再检查
    Ok((settled_results, was_cancelled, deferred_error))
}

/// 把工具执行的原始结果收敛为 `ToolResult`，并保留需要交给 `run_on_error` 的原始错误。
///
/// 纯转换：不写 state、不调中间件、不 emit 事件。放在工具 future 内调用，
/// 使 `ToolEnd` 能在该工具完成的第一时间发出，而不必等整批工具结束。
/// `Some(error)` 对应原有错误路径中需要 `run_on_error` 的分支；
/// `ToolNotFound` 原路径不调用 `run_on_error`，故返回 `None`。
fn finalize_tool_result(
    call: &ToolCall,
    raw: Result<crate::tools::ToolContent, AgentError>,
) -> (ToolResult, Option<AgentError>) {
    match raw {
        Ok(tool_content) => {
            let result = if let Some(content) = tool_content.content {
                ToolResult::success_rich(&call.id, &call.name, tool_content.output, content)
            } else {
                ToolResult::success(&call.id, &call.name, tool_content.output)
            };
            (result, None)
        }
        Err(e) => {
            let (output, kind, needs_on_error) = match &e {
                AgentError::ToolNotFound(name) => {
                    tracing::warn!(tool.name = %name, "工具未找到，作为错误结果返回");
                    (
                        format!("工具 '{}' 不存在", name),
                        ToolErrorKind::ToolNotFound,
                        false,
                    )
                }
                AgentError::ToolSchemaValidationFailed { reason, .. } => {
                    (reason.clone(), ToolErrorKind::SchemaValidation, true)
                }
                AgentError::ToolExecutionFailed { reason, .. } => {
                    (reason.clone(), ToolErrorKind::ExecutionFailed, true)
                }
                _ => (e.to_string(), ToolErrorKind::Other, true),
            };
            (
                ToolResult::error_with_kind(&call.id, &call.name, output, kind),
                needs_on_error.then_some(e),
            )
        }
    }
}

#[cfg(test)]
#[path = "tool_dispatch_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tool_dispatch_identity_test.rs"]
mod identity_tests;

#[cfg(test)]
#[path = "tool_dispatch_pending_test.rs"]
mod pending_tests;

#[cfg(test)]
#[path = "tool_dispatch_output_test.rs"]
mod output_tests;

#[cfg(test)]
#[path = "tool_dispatch_completion_test.rs"]
mod completion_tests;
