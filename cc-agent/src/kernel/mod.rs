//! Agent 内核：提示词、工具名与参数 schema 的方言边界。
//!
//! 内部只使用**规范名**（`Read` / `Edit` / `Bash` …），只有面向模型与用户的
//! 文本才翻译成内核方言。三个转换点：出站（工具定义 / 历史 / 提示语）、
//! 入站（`normalize_reasoning`）、展示（无需改，事件里已是规范名）。
//!
//! `Kernel` 是 newtype 而非 enum：新增内核只需往 [`ALL_SPECS`] 追加一项，
//! 不触发任何 match 穷举改动。

use std::borrow::Cow;

use serde_json::Value;

use crate::agent::react::Reasoning;
use crate::messages::BaseMessage;
use crate::tools::{ToolDefinition, ToolParameters, ToolParametersKind};

mod policy;
mod spec;
pub mod text;

pub use policy::KernelPolicy;
pub use spec::{CC, DSH, PI};

/// 所有内核定义。新增内核只追加到这里。
pub static ALL_SPECS: &[&KernelSpec] = &[&CC, &DSH, &PI];

/// 内核句柄。`Copy` newtype，内部指向编译期常量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kernel(&'static KernelSpec);

impl Default for Kernel {
    fn default() -> Self {
        Self(&CC)
    }
}

/// 严格解析失败：会话里记着当前版本不认识的内核 id。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKernel(pub String);

impl std::fmt::Display for UnknownKernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown agent kernel '{}'", self.0)
    }
}

impl std::error::Error for UnknownKernel {}

/// 系统提示词来源。
#[derive(Debug, PartialEq, Eq)]
pub enum PromptSource {
    /// 所有模型共用一份（当前 cc / dsh / pi 三个内核）
    Single(&'static str),
    /// 按模型匹配，未命中用 default（预留给按模型分提示词的内核）
    ByModel {
        default: &'static str,
        overrides: &'static [(&'static str, &'static str)],
    },
}

/// 工具变更形态：描述同一个工具在不同内核下的 schema 表达差异。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MutationForm {
    /// 顶层 file_path + old_string + new_string + replace_all（cc / dsh）
    #[default]
    SingleReplace,
    /// path + edits[]{oldText,newText}（pi）
    BatchReplace,
}

/// 参数改名规则。方向统一为 **dialect → canonical**（入站归一化用）。
#[derive(Debug, PartialEq, Eq)]
pub enum ParamSpec {
    /// 顶层改名
    Rename(&'static [(&'static str, &'static str)]),
    /// 顶层改名 + 数组元素内字段改名（pi Edit）
    RenameWithArrayFields {
        top: &'static [(&'static str, &'static str)],
        array_key: &'static str,
        fields: &'static [(&'static str, &'static str)],
    },
}

impl ParamSpec {
    fn top(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Rename(pairs) | Self::RenameWithArrayFields { top: pairs, .. } => pairs,
        }
    }
}

/// 单个工具的内核方言定义。
#[derive(Debug, PartialEq, Eq)]
pub struct ToolSpec {
    /// 规范名（内部唯一）
    pub canonical: &'static str,
    /// 暴露给 LLM 的方言名
    pub exposed: &'static str,
    /// 方言描述；空串表示「沿用工具自身描述」（cc 内核保持逐字节等价）
    pub description: &'static str,
    /// 进入系统提示词 tools section 的一行摘要
    pub prompt_snippet: &'static str,
    /// 进入系统提示词 rules section 的准则
    pub prompt_guidelines: &'static [&'static str],
    pub params: ParamSpec,
    /// 参数特征 → 建议工具；键和值都用方言名
    pub signature_hints: &'static [(&'static [&'static str], &'static str)],
    /// 该工具的执行形态；None = 工具自身默认形态
    pub mutation_form: Option<MutationForm>,
    /// 入参表达方式
    pub parameters_kind: ToolParametersKind,
}

/// 一个内核的完整定义。
#[derive(Debug, PartialEq, Eq)]
pub struct KernelSpec {
    pub id: &'static str,
    pub prompt: PromptSource,
    /// 该内核暴露的工具集（顺序即定义顺序）
    pub tools: &'static [ToolSpec],
    /// 本内核「管辖」的工具规范名集合。只对这些工具做可见性裁剪；
    /// 不在集合内的工具（MCP / LSP / Cron / AgentResult）一律放行。
    pub governed_tools: &'static [&'static str],
    /// 本内核暴露的元工具；必须与 `exposes_tool_search` 等价。
    pub meta_tools: &'static [&'static str],
    /// 是否暴露延迟加载元工具
    pub exposes_tool_search: bool,
}

/// 与当前内核无关的方言名 → 规范名解析（多对一超集）。
///
/// 供 subagent 的 `disallowedTools` 等「不持有具体内核、但需要统一名字」
/// 的消费方使用：任何内核的方言名都会落到同一个规范名。
pub fn canonical_tool_name(name: &str) -> Option<&'static str> {
    Kernel::cc().canonical_name(name)
}

/// 现有 `TOOL_ALIASES` 的历史别名，并入归一化超集。
const EXTRA_ALIASES: &[(&str, &str)] = &[
    ("shell", "Bash"),
    ("task", "Agent"),
    ("reading", "Read"),
    ("read_file", "Read"),
    ("write_file", "Write"),
    ("edit_file", "Edit"),
];

impl Kernel {
    pub fn cc() -> Self {
        Self(&CC)
    }

    /// 宽松解析：未知值回退 cc（全局配置容错，不阻断启动）。
    pub fn from_config(s: Option<&str>) -> Self {
        match s {
            Some(id) => Self::from_config_checked(id).unwrap_or_else(Self::cc),
            None => Self::cc(),
        }
    }

    /// 严格解析：未知 id 返回 None（供切换 / resume 硬失败用）。
    pub fn from_config_checked(id: &str) -> Option<Self> {
        ALL_SPECS.iter().find(|s| s.id == id).map(|s| Self(s))
    }

    /// 严格解析：未知 id 返回 Err。
    pub fn try_from_id(id: &str) -> Result<Self, UnknownKernel> {
        Self::from_config_checked(id).ok_or_else(|| UnknownKernel(id.to_string()))
    }

    pub fn spec(&self) -> &'static KernelSpec {
        self.0
    }

    pub fn id(&self) -> &'static str {
        self.0.id
    }

    pub fn is_cc(&self) -> bool {
        self.0.id == CC.id
    }

    /// 按模型解析系统提示词：精确 → 最长前缀 → 失败回 default。
    pub fn prompt_for(&self, model: &str) -> &'static str {
        match &self.spec().prompt {
            PromptSource::Single(p) => p,
            PromptSource::ByModel { default, overrides } => {
                if let Some((_, p)) = overrides.iter().find(|(m, _)| *m == model) {
                    return p;
                }
                overrides
                    .iter()
                    .filter(|(m, _)| {
                        model.strip_prefix(*m).is_some_and(|rest| {
                            rest.is_empty() || rest.starts_with(['-', '.', '/'])
                        })
                    })
                    .max_by_key(|(m, _)| m.len())
                    .map(|(_, p)| *p)
                    .unwrap_or(*default)
            }
        }
    }

    pub fn tool_spec(&self, canonical: &str) -> Option<&'static ToolSpec> {
        self.spec().tools.iter().find(|t| t.canonical == canonical)
    }

    /// 方言名 → 规范名（多对一超集：规范名 + 全部内核方言名 + 历史别名）。
    pub fn canonical_name(&self, name: &str) -> Option<&'static str> {
        for spec in ALL_SPECS {
            for tool in spec.tools {
                if tool.canonical == name || tool.exposed.eq_ignore_ascii_case(name) {
                    return Some(tool.canonical);
                }
            }
            for meta in spec.meta_tools {
                if *meta == name {
                    return Some(*meta);
                }
            }
        }
        for (alias, canonical) in EXTRA_ALIASES {
            if alias.eq_ignore_ascii_case(name) {
                return Some(canonical);
            }
        }
        None
    }

    /// 规范名 → 本内核方言名（None = 该内核不暴露）。
    pub fn dialect_name(&self, canonical: &str) -> Option<&'static str> {
        self.tool_spec(canonical).map(|t| t.exposed).or_else(|| {
            self.spec()
                .meta_tools
                .iter()
                .find(|m| **m == canonical)
                .copied()
        })
    }

    /// 回显给模型的工具名；任何面向模型的反馈都用它。
    pub fn echo_name<'a>(&self, canonical: &'a str) -> &'a str {
        self.dialect_name(canonical).unwrap_or(canonical)
    }

    pub fn governs(&self, canonical: &str) -> bool {
        self.spec().governed_tools.contains(&canonical)
    }

    pub fn is_exposed(&self, canonical: &str) -> bool {
        self.tool_spec(canonical).is_some() || self.is_meta_tool(canonical)
    }

    /// 直接暴露（在 spec.tools 里，不含元工具）。
    pub fn is_directly_exposed(&self, canonical: &str) -> bool {
        self.tool_spec(canonical).is_some()
    }

    pub fn is_meta_tool(&self, canonical: &str) -> bool {
        self.spec().meta_tools.contains(&canonical)
    }

    pub fn exposes_tool_search(&self) -> bool {
        self.spec().exposes_tool_search
    }

    /// 返回 true = 隐藏（与现有 `tool_filter` 极性一致）。
    ///
    /// 三层叠加：① 元工具始终可见 ② 非管辖工具一律放行 ③ 管辖内未列出则隐藏
    /// ④ 已列出的管辖工具，只有声明了 tool_search 的内核才把非直接暴露项走检索。
    pub fn should_hide(&self, name: &str) -> bool {
        let canonical = self.canonical_name(name).unwrap_or(name);
        if self.is_meta_tool(canonical) {
            return false;
        }
        if !self.governs(canonical) {
            return false;
        }
        if !self.is_exposed(canonical) {
            return true;
        }
        self.exposes_tool_search() && !self.is_directly_exposed(canonical)
    }

    /// 出站：改写工具定义的 name / description / parameters。
    /// cc 内核是恒等变换（逐字节等价回归基线）。
    pub fn translate_definitions(&self, defs: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
        if self.is_cc() {
            return defs;
        }
        defs.into_iter()
            .map(|mut def| {
                if let Some(spec) = self.tool_spec(&def.name) {
                    def.name = spec.exposed.to_string();
                    if !spec.description.is_empty() {
                        def.description = spec.description.to_string();
                    }
                    if let ToolParameters::Json(v) = &def.parameters {
                        def.parameters = ToolParameters::Json(self.translate_schema(spec, v));
                    }
                }
                def
            })
            .collect()
    }

    /// 出站：把历史 AI 消息里的工具调用名字回译成本内核方言名。
    /// 只改名字，参数原样（历史是不可变的事实记录）。
    pub fn dialectize_messages(&self, messages: &mut [BaseMessage]) {
        if self.is_cc() {
            return;
        }
        for msg in messages {
            msg.map_ai_tool_names(|name, _input| {
                self.canonical_name(name)
                    .and_then(|c| self.dialect_name(c))
                    .unwrap_or(name)
                    .to_string()
            });
        }
    }

    /// 入站：一次性归一化 reasoning（dialect → canonical）。
    ///
    /// 同时改写三处存储：`tool_calls[].name/.input`、
    /// `source_message.content` 的 `ContentBlock::ToolUse`、`source_message.tool_calls[].name`。
    pub fn normalize_reasoning(&self, mut reasoning: Reasoning) -> Reasoning {
        reasoning.raw_tool_calls = Some(reasoning.tool_calls.clone());
        for call in &mut reasoning.tool_calls {
            let Some(canonical) = self.canonical_name(&call.name) else {
                continue;
            };
            let canonical = canonical.to_string();
            call.name = canonical.clone();
            call.input = self.normalize_input(&canonical, &call.input);
        }
        if let Some(msg) = &mut reasoning.source_message {
            msg.map_ai_tool_names(|name, input| {
                let Some(canonical) = self.canonical_name(name) else {
                    return name.to_string();
                };
                let canonical = canonical.to_string();
                *input = self.normalize_input(&canonical, input);
                canonical
            });
        }
        reasoning
    }

    /// 入站参数改名（dialect → canonical），含 `edits[]` 数组元素内字段。
    pub fn normalize_input(&self, canonical: &str, input: &Value) -> Value {
        let Some(spec) = self.tool_spec(canonical) else {
            return input.clone();
        };
        let mut out = input.clone();
        rename_keys(&mut out, spec.params.top());
        if let ParamSpec::RenameWithArrayFields {
            array_key, fields, ..
        } = &spec.params
        {
            let key = *array_key;
            if let Some(items) = out.get_mut(key).and_then(Value::as_array_mut) {
                for item in items.iter_mut() {
                    rename_keys(item, fields);
                }
            }
        }
        out
    }

    /// 出站 schema 键名改写（canonical → dialect）。
    fn translate_schema(&self, spec: &'static ToolSpec, schema: &Value) -> Value {
        let mut out = schema.clone();
        let reverse: Vec<(&str, &str)> = spec
            .params
            .top()
            .iter()
            .map(|(dialect, canonical)| (*canonical, *dialect))
            .collect();
        rename_schema_keys(&mut out, &reverse);
        if let ParamSpec::RenameWithArrayFields {
            array_key, fields, ..
        } = &spec.params
        {
            let reverse_fields: Vec<(&str, &str)> = fields
                .iter()
                .map(|(dialect, canonical)| (*canonical, *dialect))
                .collect();
            if let Some(props) = out
                .pointer_mut(&format!("/properties/{array_key}/items/properties"))
                .and_then(Value::as_object_mut)
            {
                let old = std::mem::take(props);
                for (k, v) in old {
                    let nk = reverse_fields
                        .iter()
                        .find(|(from, _)| *from == k)
                        .map(|(_, to)| to.to_string())
                        .unwrap_or(k);
                    props.insert(nk, v);
                }
            }
            if let Some(required) = out
                .pointer_mut(&format!("/properties/{array_key}/items/required"))
                .and_then(Value::as_array_mut)
            {
                for r in required.iter_mut() {
                    if let Some(s) = r.as_str() {
                        if let Some((_, to)) = reverse_fields.iter().find(|(from, _)| *from == s) {
                            *r = Value::String(to.to_string());
                        }
                    }
                }
            }
        }
        out
    }

    /// 把一段**由代码生成的提示语**中的规范名替换为方言名。
    /// 只允许传入提示语常量 / format! 结果，禁止传入工具输出数据。
    /// cc 内核零开销返回 Borrowed。
    pub fn dialectize_hint<'a>(&self, hint: &'a str) -> Cow<'a, str> {
        if self.is_cc() {
            return Cow::Borrowed(hint);
        }
        let mut rules: Vec<(&str, &str)> = self
            .spec()
            .tools
            .iter()
            .map(|t| (t.canonical, t.exposed))
            .collect();
        rules.sort_by_key(|(canonical, _)| std::cmp::Reverse(canonical.len()));
        let mut out = String::with_capacity(hint.len());
        let mut i = 0;
        while i < hint.len() {
            let mut matched = false;
            for (canonical, exposed) in &rules {
                if hint[i..].starts_with(canonical) {
                    out.push_str(exposed);
                    i += canonical.len();
                    matched = true;
                    break;
                }
            }
            if !matched {
                let ch = hint[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
        Cow::Owned(out)
    }

    /// 按当前内核方言拼装的错配建议（键和值都用方言名）。
    pub fn suggest_mismatch(&self, canonical_tool: &str, input: &Value) -> Option<String> {
        let input_map = input.as_object()?;
        let input_keys: Vec<&str> = input_map.keys().map(String::as_str).collect();
        let echo = self.echo_name(canonical_tool);
        for spec in self.spec().tools {
            for (signature_keys, suggested) in spec.signature_hints {
                if suggested.eq_ignore_ascii_case(echo) {
                    continue;
                }
                if signature_keys
                    .iter()
                    .all(|sig_key| input_keys.contains(sig_key))
                {
                    return Some(format!(
                        "💡 Did you mean to use '{suggested}'? The parameters {signature_keys:?} are characteristic of the {suggested} tool."
                    ));
                }
            }
        }
        None
    }
}

/// 把 JSON 对象里的 `from` 键改名为 `to`（in-place，客户端参数用）。
fn rename_keys(value: &mut Value, renames: &[(&str, &str)]) {
    let Some(map) = value.as_object_mut() else {
        return;
    };
    for (from, to) in renames {
        if from == to {
            continue;
        }
        if let Some(v) = map.remove(*from) {
            map.insert(to.to_string(), v);
        }
    }
}

/// 把 JSON Schema 的 `properties` 键名与 `required` 数组按 renames 改写。
fn rename_schema_keys(value: &mut Value, renames: &[(&str, &str)]) {
    if let Some(props) = value.get_mut("properties").and_then(Value::as_object_mut) {
        let old = std::mem::take(props);
        for (k, v) in old {
            let nk = renames
                .iter()
                .find(|(from, _)| *from == k)
                .map(|(_, to)| to.to_string())
                .unwrap_or(k);
            props.insert(nk, v);
        }
    }
    if let Some(required) = value.get_mut("required").and_then(Value::as_array_mut) {
        for r in required.iter_mut() {
            if let Some(s) = r.as_str() {
                if let Some((_, to)) = renames.iter().find(|(from, _)| *from == s) {
                    *r = Value::String(to.to_string());
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "kernel_test.rs"]
mod tests;
