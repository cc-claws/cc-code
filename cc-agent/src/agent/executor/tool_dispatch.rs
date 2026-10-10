use std::collections::{HashMap, VecDeque};

use tokio_util::sync::CancellationToken;

use crate::{
    agent::{
        events::AgentEvent,
        react::{ReactLLM, Reasoning, ToolCall, ToolErrorKind, ToolResult},
        state::State,
    },
    error::{AgentError, AgentResult},
    messages::{message::MessageId, BaseMessage, ToolCallRequest},
    tools::BaseTool,
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

/// 返回 JSON 值的类型名称（用于错误提示）
fn json_type_name(val: &serde_json::Value) -> &'static str {
    match val {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                "number"
            }
        }
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// 从属性 schema 中提取人类可读的期望类型描述
fn get_expected_type_from_schema(prop_schema: &serde_json::Value) -> String {
    if let Some(t) = prop_schema.get("type") {
        if let Some(s) = t.as_str() {
            return s.to_string();
        }
        if let Some(arr) = t.as_array() {
            let types: Vec<_> = arr.iter().filter_map(|v| v.as_str()).collect();
            if !types.is_empty() {
                return types.join(" | ");
            }
        }
    }
    if let Some(enum_vals) = prop_schema.get("enum").and_then(|e| e.as_array()) {
        let vals: Vec<_> = enum_vals.iter().map(|v| v.to_string()).collect();
        return format!("one of [{}]", vals.join(", "));
    }
    "any".to_string()
}

/// 检查单个类型是否匹配
fn matches_single_type(val: &serde_json::Value, expected: &str) -> bool {
    match expected {
        "string" => val.is_string(),
        "integer" => {
            if let Some(n) = val.as_number() {
                n.is_i64() || n.is_u64()
            } else {
                false
            }
        }
        "number" => val.is_number(),
        "boolean" => val.is_boolean(),
        "array" => val.is_array(),
        "object" => val.is_object(),
        "null" => val.is_null(),
        _ => true,
    }
}

/// 校验单个字段值的类型与枚举
fn validate_value_type(
    val: &serde_json::Value,
    prop_schema: &serde_json::Value,
    field_name: &str,
) -> Result<(), String> {
    // 检查 enum
    if let Some(enum_vals) = prop_schema.get("enum").and_then(|e| e.as_array()) {
        if !enum_vals.contains(val) {
            let vals: Vec<_> = enum_vals.iter().map(|v| v.to_string()).collect();
            return Err(format!(
                "field '{field_name}' value {val} is not one of [{}]",
                vals.join(", ")
            ));
        }
    }

    // 检查 type
    if let Some(type_val) = prop_schema.get("type") {
        let matched = match type_val {
            serde_json::Value::String(s) => matches_single_type(val, s),
            serde_json::Value::Array(arr) => arr.iter().any(|item| {
                if let Some(s) = item.as_str() {
                    matches_single_type(val, s)
                } else {
                    false
                }
            }),
            _ => true,
        };

        if !matched {
            let expected = get_expected_type_from_schema(prop_schema);
            return Err(format!(
                "field '{field_name}' has invalid type: expected {expected}, got {}",
                json_type_name(val)
            ));
        }
    }

    Ok(())
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

/// 基于 JSON Schema 预校验工具入参（结构化错误消息版本）。
///
/// 校验（汇总所有错误，不再 early return）：
/// 1. 顶层是否期望为 Object
/// 2. 必填字段（required）是否存在且非 null（除非明确允许 null）
/// 3. 未定义的意外字段（Unexpected parameters）
/// 4. 已传属性的类型是否符合 properties 定义
pub(crate) fn validate_against_schema(
    input: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(), String> {
    let schema_obj = match schema.as_object() {
        Some(obj) if !obj.is_empty() => obj,
        _ => return Ok(()),
    };

    let expects_object = schema_obj.get("type").and_then(|t| t.as_str()) == Some("object")
        || schema_obj.contains_key("properties")
        || schema_obj.contains_key("required");

    if expects_object && !input.is_object() {
        return Err(format!(
            "expected an object for arguments, got {}",
            json_type_name(input)
        ));
    }

    let input_map = match input.as_object() {
        Some(map) => map,
        None => return Ok(()),
    };

    let props = schema_obj.get("properties").and_then(|p| p.as_object());
    let mut errors: Vec<String> = Vec::new();

    // 1. 检查必填字段（汇总所有缺失项）
    if let Some(required) = schema_obj.get("required").and_then(|r| r.as_array()) {
        for item in required {
            if let Some(field) = item.as_str() {
                let val = input_map.get(field);
                let is_missing = match val {
                    None => true,
                    Some(serde_json::Value::Null) => {
                        let allows_null = props
                            .and_then(|p| p.get(field))
                            .and_then(|p_schema| p_schema.get("type"))
                            .is_some_and(|t| match t {
                                serde_json::Value::String(s) => s == "null",
                                serde_json::Value::Array(arr) => {
                                    arr.iter().any(|v| v.as_str() == Some("null"))
                                }
                                _ => false,
                            });
                        !allows_null
                    }
                    _ => false,
                };

                if is_missing {
                    let expected_type = props
                        .and_then(|p| p.get(field))
                        .map(get_expected_type_from_schema)
                        .unwrap_or_else(|| "any".to_string());
                    errors.push(format!(
                        "The required parameter '{field}' is missing (expected {expected_type})"
                    ));
                }
            }
        }
    }

    // 2. 检查未定义的意外字段
    if let Some(props_map) = props {
        let mut allowed: Vec<&str> = props_map.keys().map(|k| k.as_str()).collect();
        allowed.sort();
        for key in input_map.keys() {
            if !props_map.contains_key(key) {
                errors.push(format!(
                    "Unexpected parameter '{key}' was provided (allowed parameters: {allowed:?})"
                ));
            }
        }
    }

    // 3. 检查属性类型（汇总所有类型错误）
    if let Some(props_map) = props {
        for (key, val) in input_map {
            if let Some(prop_schema) = props_map.get(key) {
                if let Err(msg) = validate_value_type(val, prop_schema, key) {
                    // 转换为 Issue 要求的格式
                    let expected = get_expected_type_from_schema(prop_schema);
                    let actual = json_type_name(val);
                    errors.push(format!(
                        "The parameter '{key}' type is expected as {expected}, but received {actual}"
                    ));
                    // 同时保留 enum 不匹配的原始信息（如果是 enum 错误而非类型错误）
                    if msg.contains("not one of") {
                        // 替换最后一条为更精确的 enum 信息
                        errors.pop();
                        errors.push(msg);
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
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
    )
    .await?;

    tracing::debug!(
        "[DEADLOCK] dispatch_tools: collect_tool_results done, {} results, was_cancelled={}, deferred={}",
        results.len(), was_cancelled, deferred_error.is_some()
    );

    // 阶段 B：一次性写入 state（Cancel / deferred_error 路径也写入，保证 state 一致）
    agent.emit(AgentEvent::MessageAdded(ai_msg.clone()));
    state.add_message(ai_msg);

    let mut warnings_to_inject = Vec::new();

    for (_, result) in &results {
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
                tracing::warn!(
                    tool = %result.tool_name,
                    count = consecutive_count,
                    kind = ?kind,
                    "连续 {} 次同类型错误，注入纠正消息",
                    consecutive_count
                );
                warnings_to_inject.push(format!(
                    "Warning: Tool '{}' has failed {} consecutive times with {} error. \
                     Stop retrying and analyze the root cause. Consider using a different approach \
                     or asking the user for guidance.",
                    result.tool_name, consecutive_count, kind_desc
                ));
            }

            // 2. Schema 校验连续失败熔断（支持单工具快速熔断 + 跨工具聚合熔断）
            if kind == ToolErrorKind::SchemaValidation {
                let (_schema_count, breaker_event) =
                    schema_failure_tracker.record_failure_detailed(&result.tool_name);
                if let Some(event) = breaker_event {
                    match event {
                        SchemaBreakerEvent::PerTool { tool_name, count } => {
                            tracing::warn!(
                                tool = %tool_name,
                                schema_fail_count = count,
                                "Schema 校验失败 {} 次（滑动窗口内），注入单工具熔断提示",
                                count
                            );
                            warnings_to_inject.push(format!(
                                "⚠️ SCHEMA VALIDATION CIRCUIT BREAKER: Tool '{}' has failed schema validation {} \
                                 times within the recent window. You are passing wrong parameters repeatedly. \
                                 STOP and carefully re-read the tool's parameter schema before your next attempt. \
                                 Do NOT retry with the same parameters.",
                                tool_name, count
                            ));
                        }
                        SchemaBreakerEvent::Aggregate {
                            failed_tools,
                            count,
                        } => {
                            let tools_str = failed_tools.join(", ");
                            tracing::warn!(
                                tools = %tools_str,
                                aggregate_count = count,
                                "跨工具 Schema 校验失败 {} 次（滑动窗口内），注入聚合熔断提示",
                                count
                            );
                            warnings_to_inject.push(format!(
                                "⚠️ SCHEMA VALIDATION CIRCUIT BREAKER: Schema validation failed {} \
                                 times across different tools ({}) within the recent window. You are \
                                 repeatedly passing invalid arguments. STOP guessing parameters. Carefully \
                                 inspect the parameter schema for each tool before invoking it. Do NOT call \
                                 tools with invented parameters.",
                                count, tools_str
                            ));
                        }
                    }
                }
            }
        } else {
            // 成功则重置该工具的所有失败计数与聚合计数
            consecutive_failure_tracker.reset_tool(&result.tool_name);
            schema_failure_tracker.reset(&result.tool_name);
        }

        let tool_msg = if result.is_error {
            BaseMessage::tool_error(&result.tool_call_id, result.output.as_str())
        } else if let Some(ref content) = result.content {
            // 多模态内容（如图片）：直接使用结构化 MessageContent
            BaseMessage::tool_result(&result.tool_call_id, content.clone())
        } else {
            BaseMessage::tool_result(&result.tool_call_id, result.output.as_str())
        };
        let tool_msg_clone = tool_msg.clone();
        state.add_message(tool_msg);
        agent.emit(AgentEvent::MessageAdded(tool_msg_clone));
    }

    // 统一在所有 ToolResult 写入完成后追加告警消息，保证 ToolUse 与 ToolResult 的紧邻性（去重）
    warnings_to_inject.dedup();
    for warning in warnings_to_inject {
        state.add_message(BaseMessage::system(warning));
    }

    // 动作签名循环检测：连续相同动作（即使成功但无效）注入纠正提示
    let step_sig = compute_step_action_signature(&reasoning.tool_calls);
    if !step_sig.is_empty() {
        let (count, is_loop) = action_loop_detector.record(&step_sig);
        if is_loop {
            tracing::warn!(
                signature = %step_sig,
                count = count,
                threshold = CONSECUTIVE_ACTION_THRESHOLD,
                "连续 {} 次执行相同工具动作签名，注入纠正消息",
                count
            );
            state.add_message(BaseMessage::system(format!(
                "Warning: You have executed the exact same tool call(s) {} consecutive times with identical parameters. \
                 Stop repeating this action. Analyze why the previous attempts did not produce new progress or achieve the goal. \
                 Try a completely different approach, use different tools, or ask the user for guidance.",
                count
            )));
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
) -> AgentResult<(Vec<(ToolCall, ToolResult)>, bool, Option<String>)> {
    let mut ready_calls: Vec<ToolCall> = Vec::with_capacity(original_calls.len());
    let mut settled_results: Vec<(ToolCall, ToolResult)> = Vec::new();

    // 阶段一：批量 before_tool
    let before_results = tokio::select! {
        biased;
        // 审批可能一直等待用户回应，取消必须能打断 before_tool；此时尚未写入 state。
        _ = cancel.cancelled() => return Err(AgentError::Interrupted),
        results = agent.chain.run_before_tools_batch(state, original_calls.clone()) => results,
    };

    for (tool_call, before_result) in original_calls.iter().zip(before_results) {
        // before_tool 阶段也检查取消
        if cancel.is_cancelled() {
            // 为已 emit ToolStart 的 ready_calls 补发 ToolEnd，
            // 避免 TUI 的 pending_tools 短暂残留
            for tc in &ready_calls {
                agent.emit(AgentEvent::ToolEnd {
                    message_id: ai_msg_id,
                    tool_call_id: tc.id.clone(),
                    name: tc.name.clone(),
                    output: "interrupted by user".to_string(),
                    is_error: true,
                    source_agent_id: None,
                });
            }
            return Err(AgentError::Interrupted);
        }
        match before_result {
            Ok(modified_call) => {
                agent.emit(AgentEvent::ToolStart {
                    message_id: ai_msg_id,
                    tool_call_id: modified_call.id.clone(),
                    name: modified_call.name.clone(),
                    input: modified_call.input.clone(),
                    source_agent_id: None,
                });
                ready_calls.push(modified_call);
            }
            Err(AgentError::ToolRejected { ref reason, .. }) => {
                let rejection_result =
                    ToolResult::error(&tool_call.id, &tool_call.name, reason.clone());
                agent.emit(AgentEvent::ToolStart {
                    message_id: ai_msg_id,
                    tool_call_id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    input: tool_call.input.clone(),
                    source_agent_id: None,
                });
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
                let _ = agent.chain.run_on_error(state, &e).await;
                // 为已 emit ToolStart 的 ready_calls 补发 ToolEnd
                for tc in &ready_calls {
                    agent.emit(AgentEvent::ToolEnd {
                        message_id: ai_msg_id,
                        tool_call_id: tc.id.clone(),
                        name: tc.name.clone(),
                        output: e.to_string(),
                        is_error: true,
                        source_agent_id: None,
                    });
                }
                return Err(e);
            }
        }
    }

    // 阶段二：所有工具并发执行。
    // SubAgent 通过 child_handler_factory 的独立 event handler 避免
    // 共享 Langfuse Mutex 的锁竞争，LLM 流式支持取消令牌中断。
    let tool_results: Vec<Result<crate::tools::ToolContent, AgentError>> = {
        let futures: Vec<_> = ready_calls
            .iter()
            .map(|call| {
                let tool_name = call.name.clone();
                let call_id = call.id.clone();
                let input = call.input.clone();
                let input = normalize_params(&tool_name, input);
                let tool = resolve_tool(&call.name, all_tools);
                let cancel = cancel.clone();
                let invocation = crate::tools::ToolInvocationContext {
                    tool_call_id: call_id.clone(),
                    source_agent_id: state.get_context("source_agent_id").map(str::to_string),
                };
                async move {
                    let span = tracing::info_span!(
                        "agent.tool_call",
                        tool.name = %tool_name,
                        tool.call_id = %call_id,
                    );
                    let _enter = span.enter();
                    let invoke_fut = async {
                        match tool {
                            Some(t) => {
                                let schema = t.parameters();
                                if let Err(msg) = validate_against_schema(&input, &schema) {
                                    let keys: Vec<String> = match &input {
                                        serde_json::Value::Object(map) => {
                                            let mut k: Vec<_> = map.keys().cloned().collect();
                                            k.sort();
                                            k
                                        }
                                        _ => Vec::new(),
                                    };
                                    // 启发式工具错配诊断
                                    let hint = suggest_tool_mismatch(&tool_name, &input)
                                        .map(|h| format!("\n{h}"))
                                        .unwrap_or_default();
                                    return Err(AgentError::ToolSchemaValidationFailed {
                                        tool: tool_name.clone(),
                                        reason: format!(
                                            "Invalid arguments for tool {tool_name}:\n{msg}\n\
                                             Received keys: {keys:?}. Rewrite the call to satisfy the schema and retry.{hint}"
                                        ),
                                    });
                                }
                                invocation.scope(t.invoke_content(input)).await.map_err(|e| {
                                    AgentError::ToolExecutionFailed {
                                        tool: tool_name.clone(),
                                        reason: e.to_string(),
                                    }
                                })
                            }
                            None => Err(AgentError::ToolNotFound(tool_name.clone())),
                        }
                    };
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => {
                            Err(AgentError::ToolExecutionFailed {
                                tool: tool_name,
                                reason: "interrupted by user".to_string(),
                            })
                        }
                        result = invoke_fut => result,
                    }
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
    let mut deferred_error: Option<String> = None;
    let mut exec_results: Vec<(ToolCall, ToolResult)> = Vec::with_capacity(ready_calls.len());

    for (modified_call, tool_result) in ready_calls.into_iter().zip(tool_results) {
        let result = match tool_result {
            Ok(tool_content) => {
                if let Some(content) = tool_content.content {
                    ToolResult::success_rich(
                        &modified_call.id,
                        &modified_call.name,
                        tool_content.output,
                        content,
                    )
                } else {
                    ToolResult::success(&modified_call.id, &modified_call.name, tool_content.output)
                }
            }
            Err(AgentError::ToolNotFound(ref name)) => {
                tracing::warn!(tool.name = %name, "工具未找到，作为错误结果返回");
                ToolResult::error_with_kind(
                    &modified_call.id,
                    &modified_call.name,
                    format!("工具 '{}' 不存在", name),
                    ToolErrorKind::ToolNotFound,
                )
            }
            Err(AgentError::ToolSchemaValidationFailed {
                ref tool,
                ref reason,
            }) => {
                let _ = agent
                    .chain
                    .run_on_error(
                        state,
                        &AgentError::ToolSchemaValidationFailed {
                            tool: tool.clone(),
                            reason: reason.clone(),
                        },
                    )
                    .await;
                ToolResult::error_with_kind(
                    &modified_call.id,
                    &modified_call.name,
                    reason.clone(),
                    ToolErrorKind::SchemaValidation,
                )
            }
            Err(AgentError::ToolExecutionFailed {
                ref tool,
                ref reason,
            }) => {
                let _ = agent
                    .chain
                    .run_on_error(
                        state,
                        &AgentError::ToolExecutionFailed {
                            tool: tool.clone(),
                            reason: reason.clone(),
                        },
                    )
                    .await;
                ToolResult::error_with_kind(
                    &modified_call.id,
                    &modified_call.name,
                    reason.clone(),
                    ToolErrorKind::ExecutionFailed,
                )
            }
            Err(ref e) => {
                let _ = agent.chain.run_on_error(state, e).await;
                ToolResult::error_with_kind(
                    &modified_call.id,
                    &modified_call.name,
                    e.to_string(),
                    ToolErrorKind::Other,
                )
            }
        };

        if result.is_error {
            tracing::warn!(
                tool.name = %result.tool_name,
                tool.is_error = true,
                error_len = result.output.len(),
                "tool call failed"
            );
        }
        agent.emit(AgentEvent::ToolEnd {
            message_id: ai_msg_id,
            tool_call_id: modified_call.id.clone(),
            name: modified_call.name.clone(),
            output: result.output.clone(),
            is_error: result.is_error,
            source_agent_id: None,
        });

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

#[cfg(test)]
#[path = "tool_dispatch_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tool_dispatch_identity_test.rs"]
mod identity_tests;
