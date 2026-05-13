use std::hash::{Hash, Hasher};

use crate::ui::theme;
use ratatui::style::Color;
use ratatui::text::Text;
use rust_create_agent::messages::{BaseMessage, ContentBlock};

use super::markdown::parse_markdown_default;

/// 从后台任务结果字符串中解析 task_id 短格式（前 8 位）。
///
/// 输入格式: `"Background task bg-{uuid} started..."`
/// 输出: `Some("{前8位}")` 或 `None`（解析失败时优雅降级）
fn parse_bg_hash(result: &str) -> Option<String> {
    result
        .strip_prefix("Background task bg-")
        .and_then(|rest| rest.split(' ').next())
        .map(|uuid| uuid.chars().take(8).collect())
}

/// 只读工具分类，用于折叠聚合
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    Read,    // Read
    Search,  // Grep
    Glob,    // Glob
    AskUser, // AskUserQuestion
}

impl ToolCategory {
    /// 从工具名判断分类；非只读工具返回 None
    pub fn from_tool_name(name: &str) -> Option<Self> {
        match name {
            "Read" => Some(ToolCategory::Read),
            "Grep" => Some(ToolCategory::Search),
            "Glob" => Some(ToolCategory::Glob),
            "AskUserQuestion" => Some(ToolCategory::AskUser),
            _ => None,
        }
    }

    /// 生成折叠摘要文本，如 "Read 3 files"
    pub fn summary(&self, count: usize) -> String {
        match self {
            ToolCategory::Read => {
                if count == 1 {
                    "Read 1 file".to_string()
                } else {
                    format!("Read {} files", count)
                }
            }
            ToolCategory::Search => {
                if count == 1 {
                    "Searched for 1 pattern".to_string()
                } else {
                    format!("Searched for {} patterns", count)
                }
            }
            ToolCategory::Glob => {
                if count == 1 {
                    "Matched 1 pattern".to_string()
                } else {
                    format!("Matched {} patterns", count)
                }
            }
            ToolCategory::AskUser => {
                if count == 1 {
                    "User answered Peri's questions".to_string()
                } else {
                    format!("User answered Peri's questions ({} batches)", count)
                }
            }
        }
    }

    /// 根据 tools 列表生成摘要，支持混合类别（如 search + read）
    pub fn summary_for_tools(tools: &[ToolEntry]) -> String {
        let count = tools.len();
        let search_count = tools.iter().filter(|t| t.tool_name == "Grep").count();
        let read_count = tools.iter().filter(|t| t.tool_name == "Read").count();
        let glob_count = tools.iter().filter(|t| t.tool_name == "Glob").count();
        let ask_count = tools
            .iter()
            .filter(|t| t.tool_name == "AskUserQuestion")
            .count();
        let has_search = search_count > 0;
        let has_read = read_count > 0;
        let has_glob = glob_count > 0;
        let has_ask = ask_count > 0;
        let mixed = [has_search, has_read, has_glob, has_ask]
            .iter()
            .filter(|&&b| b)
            .count()
            > 1;

        if mixed {
            let mut parts: Vec<String> = Vec::new();
            if search_count > 0 {
                parts.push(ToolCategory::Search.summary(search_count));
            }
            if read_count > 0 {
                parts.push(ToolCategory::Read.summary(read_count));
            }
            if glob_count > 0 {
                parts.push(ToolCategory::Glob.summary(glob_count));
            }
            if ask_count > 0 {
                parts.push(ToolCategory::AskUser.summary(ask_count));
            }
            parts.join(", ")
        } else if has_ask {
            ToolCategory::AskUser.summary(count)
        } else if has_search {
            ToolCategory::Search.summary(count)
        } else if has_read {
            ToolCategory::Read.summary(count)
        } else if has_glob {
            ToolCategory::Glob.summary(count)
        } else if count == 1 {
            "1 operation".to_string()
        } else {
            format!("{} operations", count)
        }
    }
}

/// ToolCallGroup 中的单条工具记录
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolEntry {
    pub tool_name: String,
    pub display_name: String,
    pub args_display: Option<String>,
    pub content: String,
    pub is_error: bool,
}

/// 批次中单个 agent 的摘要信息
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentSummary {
    pub agent_id: String,
    /// 任务描述，截断到 50 字符
    pub task_preview: String,
    /// 工具调用数
    pub tool_count: usize,
    /// 是否以错误结束
    pub is_error: bool,
    /// 最终结果（仅第一行）
    pub final_result: Option<String>,
}

/// 将 view_messages 中相邻的只读 ToolBlock 聚合为 ToolCallGroup（支持跨类别，跳过空 thinking bubble）
pub fn aggregate_tool_groups(messages: &mut Vec<MessageViewModel>) {
    aggregate_tail_tool_groups(messages, 0);
}

/// 从 `from_idx` 开始聚合尾部相邻的只读 ToolBlock。
/// `from_idx` 之前的消息保持不变（已聚合的部分不需要重新处理）。
pub fn aggregate_tail_tool_groups(messages: &mut Vec<MessageViewModel>, from_idx: usize) {
    if from_idx >= messages.len() {
        return;
    }
    let mut result: Vec<MessageViewModel> = Vec::with_capacity(messages.len());
    result.extend(messages[..from_idx].iter().cloned());

    let mut i = from_idx;
    let original_len = messages.len();
    while i < original_len {
        let vm = &messages[i];
        if let MessageViewModel::ToolBlock { tool_name, .. } = vm {
            if let Some(cat) = ToolCategory::from_tool_name(tool_name) {
                // 收集连续的同类只读 ToolBlock（跳过中间的空 thinking bubble）
                let mut entries: Vec<ToolEntry> = Vec::new();
                let mut j = i;
                while j < original_len {
                    if let MessageViewModel::ToolBlock {
                        tool_name: tn,
                        display_name,
                        args_display,
                        content,
                        is_error,
                        ..
                    } = &messages[j]
                    {
                        let entry_cat = ToolCategory::from_tool_name(tn);
                        // AskUser 只聚合 AskUser，其他只读工具允许跨类别合并
                        if entry_cat.is_some()
                            && (cat == ToolCategory::AskUser)
                                == (entry_cat == Some(ToolCategory::AskUser))
                        {
                            entries.push(ToolEntry {
                                tool_name: tn.clone(),
                                display_name: display_name.clone(),
                                args_display: args_display.clone(),
                                content: content.clone(),
                                is_error: *is_error,
                            });
                            j += 1;
                            continue;
                        }
                    }
                    // 跳过中间的空 thinking bubble
                    if messages[j].is_reasoning_only() {
                        j += 1;
                        continue;
                    }
                    break;
                }
                result.push(MessageViewModel::ToolCallGroup {
                    category: cat,
                    tools: entries,
                    collapsed: true,
                });
                i = j;
                continue;
            }
        }
        result.push(messages[i].clone());
        i += 1;
    }

    *messages = result;
}

/// 将连续的、已完成的 SubAgentGroup 聚合为批次汇总视图。
///
/// 扫描 messages，找到连续的、`batch_agents` 为空且非运行中的 SubAgentGroup 区间，
/// 区间长度 > 1 时合并为一个带 `batch_agents` 的汇总 VM，默认折叠。
/// 流式期间 `is_running: true` 的 VM 不参与聚合。
pub fn aggregate_batch_groups(messages: &mut Vec<MessageViewModel>) {
    if messages.is_empty() {
        return;
    }

    let mut result: Vec<MessageViewModel> = Vec::with_capacity(messages.len());
    let mut i = 0;
    let len = messages.len();

    while i < len {
        // 检查当前 VM 是否为可聚合的 SubAgentGroup
        let is_aggregatable = matches!(
            &messages[i],
            MessageViewModel::SubAgentGroup {
                is_running: false,
                batch_agents,
                ..
            } if batch_agents.is_empty()
        );

        if !is_aggregatable {
            result.push(messages[i].clone());
            i += 1;
            continue;
        }

        // 收集连续的可聚合 SubAgentGroup
        let run_start = i;
        let mut batch_summaries: Vec<AgentSummary> = Vec::new();

        while i < len {
            if let MessageViewModel::SubAgentGroup {
                agent_id,
                task_preview,
                total_steps,
                is_running: false,
                is_error,
                final_result,
                batch_agents,
                ..
            } = &messages[i]
            {
                if batch_agents.is_empty() {
                    batch_summaries.push(AgentSummary {
                        agent_id: agent_id.clone(),
                        task_preview: task_preview.chars().take(50).collect(),
                        tool_count: *total_steps,
                        is_error: *is_error,
                        final_result: final_result
                            .as_ref()
                            .map(|r| r.lines().next().unwrap_or("").chars().take(80).collect()),
                    });
                    i += 1;
                    continue;
                }
            }
            break;
        }

        let run_len = i - run_start;
        if run_len <= 1 {
            // 单个 SubAgentGroup，不聚合
            result.push(messages[run_start].clone());
        } else {
            // 合并：保留第一个 VM 的位置，设置 batch_agents，collapsed=true
            let mut merged = messages[run_start].clone();
            if let MessageViewModel::SubAgentGroup {
                ref mut batch_agents,
                ref mut collapsed,
                ..
            } = merged
            {
                *batch_agents = batch_summaries;
                *collapsed = true;
            }
            result.push(merged);
        }
    }

    *messages = result;
}

/// 渲染层的视图模型，从 BaseMessage/AgentEvent 转换而来
#[derive(Debug, Clone)]
pub enum MessageViewModel {
    /// 用户输入
    UserBubble {
        #[allow(dead_code)]
        content: String,
        rendered: Text<'static>,
    },
    /// AI 回复（支持流式追加）
    AssistantBubble {
        blocks: Vec<ContentBlockView>,
        is_streaming: bool,
        /// 折叠状态：true 表示完全隐藏，false 表示展开显示
        collapsed: bool,
    },
    /// 工具调用结果
    ToolBlock {
        #[allow(dead_code)]
        tool_name: String,
        tool_call_id: String,
        display_name: String,
        args_display: Option<String>,
        content: String,
        is_error: bool,
        collapsed: bool,
        color: Color,
    },
    /// 系统消息
    SystemNote { content: String },
    /// 缓存率过低警告（黄色纯文本，无前缀符号）
    CacheWarning { content: String },
    /// 只读工具调用聚合组（read/search/glob 折叠显示）
    ToolCallGroup {
        category: ToolCategory,
        tools: Vec<ToolEntry>,
        collapsed: bool,
    },
    /// SubAgent 执行块（可折叠，含滑动窗口消息）
    SubAgentGroup {
        agent_id: String,
        task_preview: String,
        /// 总步数（工具调用 + AI 回复），不受滑动窗口截断影响
        total_steps: usize,
        /// 滑动窗口，最多 4 条最近消息
        recent_messages: Vec<MessageViewModel>,
        /// 子 agent 执行中为 true
        is_running: bool,
        /// 默认展开，完成后用户可折叠
        collapsed: bool,
        /// SubAgentEnd 携带的结果摘要（工具返回值）
        final_result: Option<String>,
        /// SubAgent 执行是否以错误结束
        is_error: bool,
        /// 是否为后台 agent
        is_background: bool,
        /// 后台任务的短 ID（task_id 前 8 位）
        bg_hash: Option<String>,
        /// 批次汇总信息：空 = 单 agent，非空 = 批次汇总模式
        batch_agents: Vec<AgentSummary>,
    },
}

/// 语义级相等比较，用于判断 Done 时是否需要 RebuildAll。
///
/// 忽略 UI-only 字段（rendered、is_streaming、collapsed、color 等），
/// 只比较影响显示内容的字段。
impl PartialEq for MessageViewModel {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                MessageViewModel::UserBubble { content: a, .. },
                MessageViewModel::UserBubble { content: b, .. },
            ) => a == b,
            (
                MessageViewModel::AssistantBubble { blocks: a, .. },
                MessageViewModel::AssistantBubble { blocks: b, .. },
            ) => a == b,
            (
                MessageViewModel::ToolBlock {
                    tool_name: a_name,
                    tool_call_id: a_tc,
                    args_display: a_args,
                    content: a_content,
                    is_error: a_err,
                    ..
                },
                MessageViewModel::ToolBlock {
                    tool_name: b_name,
                    tool_call_id: b_tc,
                    args_display: b_args,
                    content: b_content,
                    is_error: b_err,
                    ..
                },
            ) => {
                a_name == b_name
                    && a_tc == b_tc
                    && a_args == b_args
                    && a_content == b_content
                    && a_err == b_err
            }
            (
                MessageViewModel::SystemNote { content: a },
                MessageViewModel::SystemNote { content: b },
            ) => a == b,
            (
                MessageViewModel::CacheWarning { content: a },
                MessageViewModel::CacheWarning { content: b },
            ) => a == b,
            (
                MessageViewModel::ToolCallGroup {
                    category: a,
                    tools: a_tools,
                    ..
                },
                MessageViewModel::ToolCallGroup {
                    category: b,
                    tools: b_tools,
                    ..
                },
            ) => a == b && a_tools == b_tools,
            (
                MessageViewModel::SubAgentGroup {
                    agent_id: a_id,
                    task_preview: a_preview,
                    total_steps: a_steps,
                    recent_messages: a_msgs,
                    final_result: a_result,
                    is_error: a_err,
                    is_background: a_bg,
                    bg_hash: a_hash,
                    batch_agents: a_batch,
                    ..
                },
                MessageViewModel::SubAgentGroup {
                    agent_id: b_id,
                    task_preview: b_preview,
                    total_steps: b_steps,
                    recent_messages: b_msgs,
                    final_result: b_result,
                    is_error: b_err,
                    is_background: b_bg,
                    bg_hash: b_hash,
                    batch_agents: b_batch,
                    ..
                },
            ) => {
                a_id == b_id
                    && a_preview == b_preview
                    && a_steps == b_steps
                    && a_msgs == b_msgs
                    && a_result == b_result
                    && a_err == b_err
                    && a_bg == b_bg
                    && a_hash == b_hash
                    && a_batch == b_batch
            }
            _ => false,
        }
    }
}

/// Hash 包含所有影响渲染输出的字段（内容 + UI 状态如 collapsed/is_streaming/is_running）。
/// `rendered` 和 `color` 不参与 hash（rendered 依赖宽度缓存，color 可从 tool_name 推导）。
impl Hash for MessageViewModel {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            MessageViewModel::UserBubble { content, .. } => {
                0u8.hash(state);
                content.hash(state);
            }
            MessageViewModel::AssistantBubble {
                blocks,
                is_streaming,
                collapsed,
                ..
            } => {
                1u8.hash(state);
                blocks.hash(state);
                is_streaming.hash(state);
                collapsed.hash(state);
            }
            MessageViewModel::ToolBlock {
                tool_name,
                tool_call_id,
                display_name,
                args_display,
                content,
                is_error,
                collapsed,
                ..
            } => {
                2u8.hash(state);
                tool_name.hash(state);
                tool_call_id.hash(state);
                display_name.hash(state);
                args_display.hash(state);
                content.hash(state);
                is_error.hash(state);
                collapsed.hash(state);
            }
            MessageViewModel::SystemNote { content } => {
                3u8.hash(state);
                content.hash(state);
            }
            MessageViewModel::CacheWarning { content } => {
                4u8.hash(state);
                content.hash(state);
            }
            MessageViewModel::ToolCallGroup {
                category,
                tools,
                collapsed,
            } => {
                5u8.hash(state);
                category.hash(state);
                tools.hash(state);
                collapsed.hash(state);
            }
            MessageViewModel::SubAgentGroup {
                agent_id,
                task_preview,
                total_steps,
                recent_messages,
                is_running,
                collapsed,
                final_result,
                is_error,
                is_background,
                bg_hash,
                batch_agents,
            } => {
                6u8.hash(state);
                agent_id.hash(state);
                task_preview.hash(state);
                total_steps.hash(state);
                recent_messages.hash(state);
                is_running.hash(state);
                collapsed.hash(state);
                final_result.hash(state);
                is_error.hash(state);
                is_background.hash(state);
                bg_hash.hash(state);
                batch_agents.hash(state);
            }
        }
    }
}

/// ContentBlock 的视图化表示
#[derive(Debug, Clone)]
pub enum ContentBlockView {
    /// 文本内容（含 markdown 解析缓存）
    Text {
        raw: String,
        rendered: Text<'static>,
        dirty: bool,
        /// 已渲染到 `raw` 的字节偏移（增量解析用）
        rendered_prefix_len: usize,
        /// `rendered` 中对应前缀的行数（避免重解析计数）
        rendered_prefix_lines: usize,
    },
    /// 推理/思考过程（仅显示字数摘要）
    Reasoning { char_count: usize },
    /// 工具使用请求（AI 发起的调用请求）
    ToolUse { name: String },
}

/// 只比较有意义的字段（忽略 `rendered`，因为 markdown 解析可能因宽度不同而异）
impl PartialEq for ContentBlockView {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                ContentBlockView::Text {
                    raw: a_raw,
                    dirty: a_dirty,
                    ..
                },
                ContentBlockView::Text {
                    raw: b_raw,
                    dirty: b_dirty,
                    ..
                },
            ) => a_raw == b_raw && a_dirty == b_dirty,
            (
                ContentBlockView::Reasoning { char_count: a },
                ContentBlockView::Reasoning { char_count: b },
            ) => a == b,
            (ContentBlockView::ToolUse { name: a }, ContentBlockView::ToolUse { name: b }) => {
                a == b
            }
            _ => false,
        }
    }
}

/// Hash 基于 PartialEq 的语义字段（忽略 `rendered` 缓存）。
/// 用于渲染线程的 hash diff：判断消息是否需要重新渲染。
impl Hash for ContentBlockView {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            ContentBlockView::Text { raw, dirty, .. } => {
                0u8.hash(state);
                raw.hash(state);
                dirty.hash(state);
            }
            ContentBlockView::Reasoning { char_count } => {
                1u8.hash(state);
                char_count.hash(state);
            }
            ContentBlockView::ToolUse { name } => {
                2u8.hash(state);
                name.hash(state);
            }
        }
    }
}

impl MessageViewModel {
    /// 从 BaseMessage 转换为视图模型（向后兼容，cwd 为 None）
    ///
    /// `prev_ai_tool_calls` 用于为 Tool 消息提供工具名和参数（BaseMessage::Tool 只存储 tool_use_id）
    pub fn from_base_message(
        msg: &BaseMessage,
        prev_ai_tool_calls: &[(String, String, serde_json::Value)],
    ) -> Self {
        Self::from_base_message_with_cwd(msg, prev_ai_tool_calls, None)
    }

    /// 从 BaseMessage 转换为视图模型（带 cwd 上下文，统一管线入口）
    ///
    /// `cwd` 用于工具参数路径缩短，确保流式和恢复路径产生一致的显示。
    /// 这是统一管线的核心转换函数——`MessagePipeline::messages_to_view_models()` 调用此方法。
    pub fn from_base_message_with_cwd(
        msg: &BaseMessage,
        prev_ai_tool_calls: &[(String, String, serde_json::Value)],
        cwd: Option<&str>,
    ) -> Self {
        match msg {
            BaseMessage::Human { content, .. } => {
                let raw = content.text_content();
                let rendered = parse_markdown_default(&raw);
                MessageViewModel::UserBubble {
                    content: raw,
                    rendered,
                }
            }
            BaseMessage::Ai {
                content,
                tool_calls,
                ..
            } => {
                // 先处理 content 中的 blocks
                let mut blocks: Vec<ContentBlockView> = content
                    .content_blocks()
                    .into_iter()
                    .map(|block| match block {
                        ContentBlock::Text { text } => {
                            let rendered = parse_markdown_default(&text);
                            let rendered_prefix_lines = rendered.lines.len();
                            ContentBlockView::Text {
                                raw: text.clone(),
                                rendered,
                                dirty: false,
                                rendered_prefix_len: text.len(),
                                rendered_prefix_lines,
                            }
                        }
                        ContentBlock::Reasoning { text, .. } => ContentBlockView::Reasoning {
                            char_count: text.chars().count(),
                        },
                        ContentBlock::ToolUse { name, .. } => ContentBlockView::ToolUse { name },
                        ContentBlock::Image { .. } => ContentBlockView::Text {
                            raw: "[Image]".to_string(),
                            rendered: Text::raw("[Image]"),
                            dirty: false,
                            rendered_prefix_len: 7,
                            rendered_prefix_lines: 1,
                        },
                        ContentBlock::Document { title, .. } => {
                            let label = title.as_deref().unwrap_or("Document");
                            let raw = format!("[Document: {}]", label);
                            let len = raw.len();
                            ContentBlockView::Text {
                                raw,
                                rendered: Text::raw(format!("[Document: {}]", label)),
                                dirty: false,
                                rendered_prefix_len: len,
                                rendered_prefix_lines: 1,
                            }
                        }
                        ContentBlock::Unknown(v) => {
                            let type_name =
                                v.get("type").and_then(|t| t.as_str()).unwrap_or("unknown");
                            let raw = format!("[{}]", type_name);
                            let len = raw.len();
                            ContentBlockView::Text {
                                raw,
                                rendered: Text::raw(format!("[{}]", type_name)),
                                dirty: false,
                                rendered_prefix_len: len,
                                rendered_prefix_lines: 1,
                            }
                        }
                        // ToolResult 在 Ai 消息中不常见，静默跳过
                        _ => ContentBlockView::Text {
                            raw: String::new(),
                            rendered: Text::raw(""),
                            dirty: false,
                            rendered_prefix_len: 0,
                            rendered_prefix_lines: 0,
                        },
                    })
                    .collect();

                // 补充 tool_calls 字段中的工具调用（当 content 中没有对应的 ToolUse block 时）
                // 避免重复：如果 content_blocks 中已有同名 ToolUse，跳过
                let existing_tool_names: std::collections::HashSet<String> = blocks
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlockView::ToolUse { name } = b {
                            Some(name.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                for tc in tool_calls {
                    if !existing_tool_names.contains(&tc.name) {
                        blocks.push(ContentBlockView::ToolUse {
                            name: tc.name.clone(),
                        });
                    }
                }

                MessageViewModel::AssistantBubble {
                    blocks,
                    is_streaming: false,
                    collapsed: false,
                }
            }
            BaseMessage::Tool {
                tool_call_id,
                content,
                is_error,
                ..
            } => {
                // 从前一条 Ai 消息的 tool_calls 中查找工具名和参数
                let (tool_name, input) = prev_ai_tool_calls
                    .iter()
                    .find(|(id, _, _)| id == tool_call_id)
                    .map(|(_, name, input)| (name.clone(), input.clone()))
                    .unwrap_or_else(|| (tool_call_id.clone(), serde_json::Value::Null));
                let raw_content = content.text_content();
                // Agent 工具恢复为 SubAgentGroup（完成状态，展开显示 final_result）
                if tool_name == "Agent" {
                    let agent_id = input
                        .get("subagent_type")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .unwrap_or("fork")
                        .to_string();
                    let task_preview = input["prompt"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(40)
                        .collect::<String>();
                    // 检测是否为后台 agent（从 result 字符串检测 "Background task" 前缀）
                    let is_background = raw_content.starts_with("Background task");
                    // 解析 bg_hash（如果是后台任务）
                    let bg_hash = if is_background {
                        parse_bg_hash(&raw_content)
                    } else {
                        None
                    };
                    return MessageViewModel::SubAgentGroup {
                        agent_id,
                        task_preview,
                        total_steps: parse_subagent_tool_count(&raw_content),
                        recent_messages: Vec::new(), // 子 agent 内部消息不持久化
                        is_running: false,
                        collapsed: false, // 展开显示 final_result
                        final_result: Some(raw_content),
                        is_error: *is_error,
                        is_background,
                        bg_hash,
                        batch_agents: Vec::new(),
                    };
                }
                // 使用统一格式化函数生成 display_name 和 args_display
                // cwd 参数确保流式和恢复路径产生一致的路径显示
                let display_name = crate::app::tool_display::format_tool_name(&tool_name);
                let args_display =
                    crate::app::tool_display::format_tool_args(&tool_name, &input, cwd);
                let color = if *is_error {
                    theme::ERROR
                } else {
                    tool_color(&tool_name)
                };
                MessageViewModel::ToolBlock {
                    tool_name,
                    tool_call_id: tool_call_id.clone(),
                    display_name,
                    args_display,
                    content: raw_content,
                    is_error: *is_error,
                    collapsed: true,
                    color,
                }
            }
            BaseMessage::System { content, .. } => MessageViewModel::SystemNote {
                content: content.text_content(),
            },
        }
    }

    /// 追加流式文本 chunk
    pub fn append_chunk(&mut self, chunk: &str) {
        if let MessageViewModel::AssistantBubble {
            blocks, collapsed, ..
        } = self
        {
            // 如果有内容追加，自动展开
            if *collapsed && !chunk.is_empty() {
                *collapsed = false;
            }
            if let Some(ContentBlockView::Text { raw, dirty, .. }) = blocks.last_mut() {
                raw.push_str(chunk);
                *dirty = true;
                return;
            }
            // 没有 Text block，创建新的
            let mut raw = String::new();
            raw.push_str(chunk);
            blocks.push(ContentBlockView::Text {
                raw,
                rendered: Text::raw(""),
                dirty: true,
                rendered_prefix_len: 0,
                rendered_prefix_lines: 0,
            });
        }
    }

    /// 切换折叠状态（对 ToolBlock、AssistantBubble、SubAgentGroup、ToolCallGroup 生效）
    #[allow(dead_code)]
    pub fn toggle_collapse(&mut self) {
        match self {
            MessageViewModel::ToolBlock { collapsed, .. }
            | MessageViewModel::AssistantBubble { collapsed, .. }
            | MessageViewModel::SubAgentGroup { collapsed, .. }
            | MessageViewModel::ToolCallGroup { collapsed, .. } => {
                *collapsed = !*collapsed;
            }
            _ => {}
        }
    }

    /// 判断是否为 AssistantBubble
    pub fn is_assistant(&self) -> bool {
        matches!(self, MessageViewModel::AssistantBubble { .. })
    }

    /// 判断是否为"仅含推理内容"的 AssistantBubble（渲染时不可见）
    /// 用于在工具分组合并时跳过中间的空 thinking bubble
    pub fn is_reasoning_only(&self) -> bool {
        match self {
            MessageViewModel::AssistantBubble { blocks, .. } => {
                blocks.is_empty()
                    || blocks.iter().all(|b| match b {
                        ContentBlockView::Reasoning { .. } => true,
                        ContentBlockView::Text { raw, .. } => raw.trim().is_empty(),
                        _ => false,
                    })
            }
            _ => false,
        }
    }

    /// 创建用户消息
    pub fn user(content: String) -> Self {
        let rendered = parse_markdown_default(&content);
        MessageViewModel::UserBubble { content, rendered }
    }

    /// 创建助手消息
    pub fn assistant() -> Self {
        MessageViewModel::AssistantBubble {
            blocks: Vec::new(),
            is_streaming: true,
            collapsed: false,
        }
    }

    /// 创建工具消息
    pub fn tool_block(
        tool_name: String,
        display: String,
        args: Option<String>,
        is_error: bool,
    ) -> Self {
        Self::tool_block_with_id(String::new(), tool_name, display, args, is_error)
    }

    /// 创建带 tool_call_id 的工具消息（SubAgent 内部并行工具调用精确匹配）
    pub fn tool_block_with_id(
        tool_call_id: String,
        tool_name: String,
        display: String,
        args: Option<String>,
        is_error: bool,
    ) -> Self {
        let color = if is_error {
            theme::ERROR
        } else {
            tool_color(&tool_name)
        };
        MessageViewModel::ToolBlock {
            tool_call_id,
            tool_name,
            display_name: display,
            args_display: args,
            content: String::new(),
            is_error,
            collapsed: true,
            color,
        }
    }

    /// 创建系统消息
    pub fn system(content: String) -> Self {
        MessageViewModel::SystemNote { content }
    }

    /// 创建缓存率警告消息（黄色纯文本，无前缀符号）
    pub fn cache_warning(content: String) -> Self {
        MessageViewModel::CacheWarning { content }
    }

    /// 创建 SubAgentGroup（初始状态：运行中、展开、0 步）
    pub fn subagent_group(agent_id: String, task_preview: String) -> Self {
        MessageViewModel::SubAgentGroup {
            agent_id,
            task_preview,
            total_steps: 0,
            recent_messages: Vec::new(),
            is_running: true,
            collapsed: false,
            final_result: None,
            is_error: false,
            is_background: false,
            bg_hash: None,
            batch_agents: Vec::new(),
        }
    }

    /// 判断是否为 SubAgentGroup
    pub fn is_subagent_group(&self) -> bool {
        matches!(self, MessageViewModel::SubAgentGroup { .. })
    }
}

/// 从 SubAgent 返回结果中解析工具调用次数。
///
/// `format_subagent_result()` 输出格式：`[Sub-agent executed N tool calls: ...]`
/// 或中文版 `[子 agent 执行了 N 个工具调用: ...]`。
/// 解析失败时返回 0（优雅降级）。
fn parse_subagent_tool_count(content: &str) -> usize {
    // 英文格式: "[Sub-agent executed N tool calls: ...]"
    if let Some(rest) = content.strip_prefix("[Sub-agent executed ") {
        if let Some(n_str) = rest.split(' ').next() {
            if let Ok(n) = n_str.parse::<usize>() {
                return n;
            }
        }
    }
    // 中文格式: "[子 agent 执行了 N 个工具调用: ...]"
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
///
/// | 类别 | 颜色 | 色值 |
/// |------|------|------|
/// | 读取/搜索 | SAGE | #4EBA65 |
/// | 写入/编辑 | WARNING | #FFC107 |
/// | 执行(bash) | BASH_BORDER | #FD5DB1 |
/// | 代理/交互 | THINKING | #AF87FF |
/// | 错误 | ERROR | #FF6B80 |
/// | 其他 | MUTED | #999999 |
pub fn tool_color(name: &str) -> Color {
    match name {
        // 读取/搜索 — 哑光绿
        "Read" | "Glob" | "Grep" => theme::SAGE,
        // 写入/编辑 — 暖米灰
        "Write" | "Edit" | "folder_operations" | "delete_file" | "delete_folder" | "rm"
        | "rm_rf" => theme::WARNING,
        // 执行 — Bash 粉红边框色
        "Bash" => theme::BASH_BORDER,
        // 代理/交互 — 紫色
        "Agent" | "AskUserQuestion" | "TodoWrite" => theme::THINKING,
        // 错误
        _ if name.contains("error") => theme::ERROR,
        // 其他
        _ => theme::MUTED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::messages::{MessageContent, ToolCallRequest};
    use serde_json::json;

    /// 测试：AI 消息只有 tool_calls（无 content）时，应正确渲染工具调用
    #[test]
    fn test_ai_message_with_only_tool_calls_renders_tool_use() {
        // 模拟：AI 消息只包含 tool_calls，content 为空
        let msg = BaseMessage::ai_with_tool_calls(
            MessageContent::text(""),
            vec![
                ToolCallRequest::new("toolu_001", "Bash", json!({"command": "ls"})),
                ToolCallRequest::new("toolu_002", "Read", json!({"path": "test.txt"})),
            ],
        );

        let vm = MessageViewModel::from_base_message(&msg, &[]);
        match vm {
            MessageViewModel::AssistantBubble { blocks, .. } => {
                // 应该有 2 个 ToolUse block
                let tool_uses: Vec<_> = blocks
                    .iter()
                    .filter(|b| matches!(b, ContentBlockView::ToolUse { .. }))
                    .collect();
                assert_eq!(tool_uses.len(), 2, "应该有 2 个 ToolUse block");

                // 验证工具名称
                let names: Vec<&str> = blocks
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlockView::ToolUse { name } = b {
                            Some(name.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                assert!(names.contains(&"Bash"), "应包含 bash 工具");
                assert!(names.contains(&"Read"), "应包含 read_file 工具");
            }
            _ => panic!("应该是 AssistantBubble"),
        }
    }

    /// 测试：AI 消息同时有文本和 tool_calls 时，两者都应渲染
    #[test]
    fn test_ai_message_with_text_and_tool_calls_renders_both() {
        let msg = BaseMessage::ai_with_tool_calls(
            MessageContent::text("I'll run a command"),
            vec![ToolCallRequest::new(
                "toolu_001",
                "Bash",
                json!({"command": "ls"}),
            )],
        );

        let vm = MessageViewModel::from_base_message(&msg, &[]);
        match vm {
            MessageViewModel::AssistantBubble { blocks, .. } => {
                // 应该有 1 个 Text block 和 1 个 ToolUse block
                let text_count = blocks
                    .iter()
                    .filter(|b| matches!(b, ContentBlockView::Text { .. }))
                    .count();
                let tool_count = blocks
                    .iter()
                    .filter(|b| matches!(b, ContentBlockView::ToolUse { .. }))
                    .count();

                assert_eq!(text_count, 1, "应该有 1 个 Text block");
                assert_eq!(tool_count, 1, "应该有 1 个 ToolUse block");
            }
            _ => panic!("应该是 AssistantBubble"),
        }
    }

    /// 测试：content 中已有 ToolUse block 时，不重复添加 tool_calls
    #[test]
    fn test_no_duplicate_tool_use_from_tool_calls() {
        use rust_create_agent::messages::ContentBlock;

        // content 中包含 ToolUse block，同时 tool_calls 也有相同的
        let blocks = vec![
            ContentBlock::text("I'll run bash"),
            ContentBlock::tool_use("toolu_001", "Bash", json!({"command": "ls"})),
        ];
        let msg = BaseMessage::ai_from_blocks(blocks);

        let vm = MessageViewModel::from_base_message(&msg, &[]);
        match vm {
            MessageViewModel::AssistantBubble { blocks, .. } => {
                // 应该只有 1 个 ToolUse block（不重复）
                let tool_count = blocks
                    .iter()
                    .filter(|b| matches!(b, ContentBlockView::ToolUse { .. }))
                    .count();
                assert_eq!(tool_count, 1, "不应该重复添加 ToolUse block");
            }
            _ => panic!("应该是 AssistantBubble"),
        }
    }

    /// 测试：纯文本 AI 消息正常渲染
    #[test]
    fn test_ai_message_with_only_text_renders_text() {
        let msg = BaseMessage::ai("Hello, how can I help?");

        let vm = MessageViewModel::from_base_message(&msg, &[]);
        match vm {
            MessageViewModel::AssistantBubble { blocks, .. } => {
                assert_eq!(blocks.len(), 1, "应该有 1 个 block");
                assert!(
                    matches!(blocks[0], ContentBlockView::Text { .. }),
                    "应该是 Text block"
                );
            }
            _ => panic!("应该是 AssistantBubble"),
        }
    }

    #[test]
    fn test_tool_category_new_names() {
        assert_eq!(
            ToolCategory::from_tool_name("Read"),
            Some(ToolCategory::Read)
        );
        assert_eq!(
            ToolCategory::from_tool_name("Grep"),
            Some(ToolCategory::Search)
        );
        assert_eq!(
            ToolCategory::from_tool_name("Glob"),
            Some(ToolCategory::Glob)
        );
        assert_eq!(ToolCategory::from_tool_name("Write"), None);
        assert_eq!(ToolCategory::from_tool_name("Bash"), None);
        assert_eq!(ToolCategory::from_tool_name("Agent"), None);
        assert_eq!(
            ToolCategory::from_tool_name("AskUserQuestion"),
            Some(ToolCategory::AskUser)
        );
    }

    #[test]
    fn test_tool_color_new_names() {
        // 读取/搜索 — SAGE
        assert_eq!(tool_color("Read"), theme::SAGE);
        assert_eq!(tool_color("Glob"), theme::SAGE);
        assert_eq!(tool_color("Grep"), theme::SAGE);
        // 写入/编辑 — WARNING
        assert_eq!(tool_color("Write"), theme::WARNING);
        assert_eq!(tool_color("Edit"), theme::WARNING);
        // 执行 — BASH_BORDER
        assert_eq!(tool_color("Bash"), theme::BASH_BORDER);
        // 代理/交互 — THINKING
        assert_eq!(tool_color("Agent"), theme::THINKING);
        assert_eq!(tool_color("AskUserQuestion"), theme::THINKING);
        assert_eq!(tool_color("TodoWrite"), theme::THINKING);
    }

    // ── aggregate_batch_groups 测试 ──

    /// 创建一个已完成的单 agent SubAgentGroup VM
    fn make_done_subagent(agent_id: &str, task: &str) -> MessageViewModel {
        MessageViewModel::SubAgentGroup {
            agent_id: agent_id.to_string(),
            task_preview: task.to_string(),
            total_steps: 3,
            recent_messages: Vec::new(),
            is_running: false,
            collapsed: false,
            final_result: Some("done".to_string()),
            is_error: false,
            is_background: false,
            bg_hash: None,
            batch_agents: Vec::new(),
        }
    }

    /// 创建一个运行中的 SubAgentGroup VM
    fn make_running_subagent(agent_id: &str, task: &str) -> MessageViewModel {
        MessageViewModel::SubAgentGroup {
            agent_id: agent_id.to_string(),
            task_preview: task.to_string(),
            total_steps: 0,
            recent_messages: Vec::new(),
            is_running: true,
            collapsed: false,
            final_result: None,
            is_error: false,
            is_background: false,
            bg_hash: None,
            batch_agents: Vec::new(),
        }
    }

    #[test]
    fn test_aggregate_batch_groups_single_agent_noop() {
        let mut vms = vec![make_done_subagent("explorer", "explore code")];
        aggregate_batch_groups(&mut vms);
        assert_eq!(vms.len(), 1, "单个 SubAgentGroup 不应聚合");
        // batch_agents 应保持为空
        if let MessageViewModel::SubAgentGroup { batch_agents, .. } = &vms[0] {
            assert!(batch_agents.is_empty());
        } else {
            panic!("应为 SubAgentGroup");
        }
    }

    #[test]
    fn test_aggregate_batch_groups_consecutive_agents() {
        let mut vms = vec![
            make_done_subagent("agent-1", "task one"),
            make_done_subagent("agent-2", "task two"),
            make_done_subagent("agent-3", "task three"),
        ];
        aggregate_batch_groups(&mut vms);
        assert_eq!(vms.len(), 1, "3 个连续已完成 SubAgentGroup 应合并为 1 个");
        if let MessageViewModel::SubAgentGroup {
            batch_agents,
            collapsed,
            ..
        } = &vms[0]
        {
            assert_eq!(batch_agents.len(), 3);
            assert!(*collapsed, "合并后应默认折叠");
            assert_eq!(batch_agents[0].agent_id, "agent-1");
            assert_eq!(batch_agents[1].agent_id, "agent-2");
            assert_eq!(batch_agents[2].agent_id, "agent-3");
        } else {
            panic!("应为 SubAgentGroup");
        }
    }

    #[test]
    fn test_aggregate_batch_groups_running_agent_skip() {
        let mut vms = vec![
            make_done_subagent("agent-1", "task one"),
            make_running_subagent("agent-2", "task two"),
            make_done_subagent("agent-3", "task three"),
        ];
        aggregate_batch_groups(&mut vms);
        // running agent 打断连续区间，不应聚合
        assert_eq!(vms.len(), 3, "中间有 is_running=true 时不合并");
    }

    #[test]
    fn test_aggregate_batch_groups_mixed_batch() {
        // 3 完成 + 1 running + 2 完成 = 两个独立的聚合区间
        // 但由于 running 打断，每个区间只有 1 个或 2 个
        let mut vms = vec![
            make_done_subagent("agent-1", "task one"),
            make_done_subagent("agent-2", "task two"),
            make_done_subagent("agent-3", "task three"),
            make_running_subagent("agent-4", "task four"),
            make_done_subagent("agent-5", "task five"),
            make_done_subagent("agent-6", "task six"),
        ];
        aggregate_batch_groups(&mut vms);
        // 前 3 个合并为 1，running 保持独立，后 2 个合并为 1
        assert_eq!(vms.len(), 3, "3+1+2 = 两个聚合区间 + 1 个 running");
        // 第一个是合并后的
        if let MessageViewModel::SubAgentGroup { batch_agents, .. } = &vms[0] {
            assert_eq!(batch_agents.len(), 3);
        } else {
            panic!("第一个应为聚合的 SubAgentGroup");
        }
        // 第二个是 running 的
        if let MessageViewModel::SubAgentGroup { is_running, .. } = &vms[1] {
            assert!(*is_running);
        } else {
            panic!("第二个应为 running 的 SubAgentGroup");
        }
        // 第三个是合并后的
        if let MessageViewModel::SubAgentGroup { batch_agents, .. } = &vms[2] {
            assert_eq!(batch_agents.len(), 2);
        } else {
            panic!("第三个应为聚合的 SubAgentGroup");
        }
    }

    #[test]
    fn test_aggregate_batch_groups_already_aggregated_skip() {
        // 已聚合的 SubAgentGroup（batch_agents 非空）不参与二次聚合
        let aggregated = MessageViewModel::SubAgentGroup {
            agent_id: "agent-1".to_string(),
            task_preview: "task one".to_string(),
            total_steps: 3,
            recent_messages: Vec::new(),
            is_running: false,
            collapsed: true,
            final_result: Some("done".to_string()),
            is_error: false,
            is_background: false,
            bg_hash: None,
            batch_agents: vec![
                AgentSummary {
                    agent_id: "agent-1".to_string(),
                    task_preview: "task one".to_string(),
                    tool_count: 3,
                    is_error: false,
                    final_result: Some("done".to_string()),
                },
                AgentSummary {
                    agent_id: "agent-2".to_string(),
                    task_preview: "task two".to_string(),
                    tool_count: 5,
                    is_error: false,
                    final_result: Some("done".to_string()),
                },
            ],
        };
        let mut vms = vec![aggregated.clone()];
        aggregate_batch_groups(&mut vms);
        assert_eq!(vms.len(), 1, "已聚合的不应二次聚合");
        if let MessageViewModel::SubAgentGroup { batch_agents, .. } = &vms[0] {
            assert_eq!(batch_agents.len(), 2, "batch_agents 应保持不变");
        } else {
            panic!("应为 SubAgentGroup");
        }
    }

    #[test]
    fn test_agent_summary_truncation() {
        let long_task = "这是一个非常非常非常非常非常非常非常非常非常非常非常非常长的任务描述需要超过五十个字符的长度才能触发截断逻辑验证";
        assert!(long_task.chars().count() > 50, "测试数据应超过 50 字符");
        let mut vms = vec![
            make_done_subagent("agent-1", long_task),
            make_done_subagent("agent-2", "short task"),
        ];
        aggregate_batch_groups(&mut vms);
        if let MessageViewModel::SubAgentGroup { batch_agents, .. } = &vms[0] {
            assert_eq!(
                batch_agents[0].task_preview.chars().count(),
                50,
                "task_preview 应截断到 50 字符"
            );
            assert_eq!(batch_agents[1].task_preview, "short task", "短文本不应截断");
        } else {
            panic!("应为 SubAgentGroup");
        }
    }

    #[test]
    fn test_batch_group_default_collapsed() {
        let mut vms = vec![
            make_done_subagent("agent-1", "task one"),
            make_done_subagent("agent-2", "task two"),
        ];
        aggregate_batch_groups(&mut vms);
        if let MessageViewModel::SubAgentGroup { collapsed, .. } = &vms[0] {
            assert!(*collapsed, "合并后应默认折叠");
        } else {
            panic!("应为 SubAgentGroup");
        }
    }
}
