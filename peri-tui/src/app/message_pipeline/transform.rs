use peri_agent::messages::BaseMessage;

use crate::{
    app::tool_display,
    ui::{
        markdown::parse_markdown_default_rich,
        message_view::{aggregate_tool_groups, tool_color, ContentBlockView, MessageViewModel},
    },
};

use super::MessagePipeline;

impl MessagePipeline {
    /// 构建当前流式 AI 消息的 AssistantBubble ViewModel。
    ///
    /// 包含 Reasoning block + 已输出的文本 + 已完成的 tool_use blocks。
    /// 不包含 pending tools——它们在 build_tail_vms 中另行处理。
    pub fn build_streaming_bubble(&self) -> MessageViewModel {
        let mut blocks: Vec<ContentBlockView> = Vec::new();
        if !self.current_ai_reasoning.is_empty() {
            // 流式进行中：耗时未知（思考尚未结束），显示为“思考中”
            blocks.push(ContentBlockView::Reasoning {
                char_count: self.current_ai_reasoning.chars().count(),
                duration_ms: None,
                action_summary: None,
                text: self.current_ai_reasoning.clone(),
                tail_lines: None,
            });
        }
        if !self.current_ai_text.trim().is_empty() {
            let doc = parse_markdown_default_rich(&self.current_ai_text);
            let rendered_prefix_lines = doc.text.lines.len();
            let mut scanner = crate::ui::markdown::TableHoldbackScanner::new();
            scanner.set_streaming(true);
            blocks.push(ContentBlockView::Text {
                raw: self.current_ai_text.clone(),
                rendered: doc.text,
                rendered_links: doc.links,
                dirty: false,
                rendered_prefix_len: self.current_ai_text.len(),
                rendered_prefix_lines,
                rendered_width: crate::ui::markdown::DEFAULT_MARKDOWN_WIDTH,
                holdback_scanner: scanner,
            });
        }
        for tc in &self.current_ai_tool_calls {
            if !self.pending_tools.contains_key(&tc.id) {
                blocks.push(ContentBlockView::ToolUse {
                    name: tc.name.clone(),
                });
            }
        }
        let mut vm = MessageViewModel::AssistantBubble {
            blocks,
            is_streaming: true,
            collapsed: false,
            content_hash: 0,
        };
        vm.recompute_hash();
        vm
    }

    /// 从规范 BaseMessage[] 构建完整的 MessageViewModel[]。
    ///
    /// **这是唯一的转换入口**——流式 reconcile 和历史恢复都调用此函数。
    pub fn messages_to_view_models(msgs: &[BaseMessage], cwd: &str) -> Vec<MessageViewModel> {
        let mut vms: Vec<MessageViewModel> = Vec::with_capacity(msgs.len());
        let mut prev_ai_tool_calls: Vec<(String, String, serde_json::Value)> = Vec::new();

        for msg in msgs {
            // System 消息（system prompt / compact summary）是内部状态，不应渲染
            if matches!(msg, BaseMessage::System { .. }) {
                continue;
            }

            if let BaseMessage::Ai { tool_calls, .. } = msg {
                prev_ai_tool_calls = tool_calls
                    .iter()
                    .map(|tc| (tc.id.clone(), tc.name.clone(), tc.arguments.clone()))
                    .collect();
            }

            let vm =
                MessageViewModel::from_base_message_with_cwd(msg, &prev_ai_tool_calls, Some(cwd));

            if let MessageViewModel::AssistantBubble { blocks, .. } = &vm {
                let has_visible = blocks.iter().any(|b| match b {
                    ContentBlockView::Text { raw, .. } => !raw.trim().is_empty(),
                    ContentBlockView::Reasoning { char_count, .. } => *char_count > 0,
                    ContentBlockView::ToolUse { .. } => false,
                });
                if !has_visible {
                    continue;
                }
            }

            vms.push(vm);
        }

        aggregate_tool_groups(&mut vms);
        merge_consecutive_thinking(&mut vms);
        vms
    }

    /// Reconcile：从当前 completed 状态重建完整的 view_models。
    ///
    /// 在 "finalize 边界"（ToolStart / Done）调用，确保流式最终状态
    /// 与 restore 路径 `messages_to_view_models()` 完全一致。
    pub fn reconcile(&self) -> Vec<MessageViewModel> {
        Self::messages_to_view_models(&self.completed, &self.cwd)
    }

    /// Finalize 当前 AI 消息：将流式状态转为 BaseMessage 加入 completed
    pub(crate) fn finalize_current_ai(&mut self) {
        if self.current_ai_finalized {
            return;
        }
        let has_content = !self.current_ai_text.trim().is_empty()
            || !self.current_ai_reasoning.is_empty()
            || !self.current_ai_tool_calls.is_empty();

        if !has_content {
            return;
        }

        self.current_ai_finalized = true;
    }

    /// 构建 ToolStart 的 ToolBlock VM（与 from_base_message_with_cwd 的 Tool 路径一致）
    pub(crate) fn build_tool_start_vm(
        &self,
        tool_call_id: &str,
        name: &str,
        input: &serde_json::Value,
        started_at: Option<std::time::Instant>,
    ) -> MessageViewModel {
        let display_name = tool_display::format_tool_name(name);
        let args_display = tool_display::format_tool_args(name, input, Some(&self.cwd));
        let mut vm = MessageViewModel::ToolBlock {
            tool_name: name.to_string(),
            tool_call_id: tool_call_id.to_string(),
            display_name,
            args_display,
            content: String::new(),
            is_error: false,
            collapsed: true,
            color: tool_color(name),
            diff_input: None,
            started_at,
            execution_timeout_ms: None,
            shell_backgrounded: false,
            content_hash: 0,
        };
        vm.recompute_hash();
        vm
    }
}

/// 只读工具动作计数文案（PRD §2.3）：
/// `read N file(s)` / `listed N director(y|ies)` / `searched for N pattern(s)`
pub fn readonly_action_summary(reads: usize, globs: usize, greps: usize) -> Option<String> {
    let mut parts = Vec::new();
    if reads > 0 {
        parts.push(format!(
            "read {reads} file{}",
            if reads > 1 { "s" } else { "" }
        ));
    }
    if globs > 0 {
        parts.push(format!(
            "listed {globs} director{}",
            if globs > 1 { "ies" } else { "y" }
        ));
    }
    if greps > 0 {
        parts.push(format!(
            "searched for {greps} pattern{}",
            if greps > 1 { "s" } else { "" }
        ));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

/// 后处理（PRD §2.8）：**合并同一回合内连续的「思考 + 只读工具」为一行**。
///
/// 真实会话中一轮任务常产生多段「Reasoning → 只读工具」，逐条渲染会刷屏：
/// ```text
/// ∴ Thought for 147 chars, read 1 file
/// ∴ Thought for 257 chars, read 1 file    ← 应合并为一行
/// ```
/// 合并后：
/// ```text
/// Thought for 32s, read 4 files, searched for 1 pattern
/// ```
///
/// **算法**：扫描「含 Reasoning 的 AssistantBubble」连续段；段内
/// ① 所有 reasoning 文本**拼接**、② `duration_ms` **累加**、③ 只读工具计数**累加**，
/// 全部写入**段首** bubble；段内其余 bubble 从列表中**移除**。
/// 只读工具的 `ToolCallGroup` 保留（折叠态不显示内容）。
///
/// **段边界**（以下情况结束当前合并段）：
/// - AssistantBubble 含**可见正文文本**（`Text` 非空）
/// - 用户消息、非「thinking bubble / 只读工具组」的 VM
///
/// **非只读工具**（Bash/Write/Edit）不参与计数，但**不打断**合并段。
pub(super) fn merge_consecutive_thinking(vms: &mut Vec<MessageViewModel>) {
    // 计数摘要由 VM 序列派生；合并已归一化前缀与实时尾部时需要重新计算。
    clear_action_summaries(vms);

    let mut out: Vec<MessageViewModel> = Vec::with_capacity(vms.len());
    let mut i = 0;
    while i < vms.len() {
        if !is_thinking_bubble(&vms[i]) {
            // 暂存独立动作摘要。若后续同一连续段出现 Thought，会把摘要转移到 Thought 行。
            let mut vm = vms[i].clone();
            if let MessageViewModel::ToolCallGroup {
                tools,
                standalone_action,
                ..
            } = &mut vm
            {
                let prev_is_thinking = out
                    .iter()
                    .rev()
                    .find(|previous| !is_tool_vm(previous))
                    .is_some_and(is_thinking_bubble_with_summary);
                if !prev_is_thinking {
                    let reads = tools.iter().filter(|t| t.tool_name == "Read").count();
                    let globs = tools.iter().filter(|t| t.tool_name == "Glob").count();
                    let greps = tools.iter().filter(|t| t.tool_name == "Grep").count();
                    *standalone_action = readonly_action_summary(reads, globs, greps);
                    vm.recompute_hash();
                }
            }
            out.push(vm);
            i += 1;
            continue;
        }

        // 收集段：bubble 与其后连续工具交替，直到边界
        let (mut tot_ms, mut missing_ms) = (0u64, false);
        // 工具也可能先于 Thought 出现。吸收 out 尾部尚未归属的独立只读计数，
        // 并清掉工具组摘要，避免同一计数显示两次。
        let (mut reads, mut globs, mut greps) = absorb_preceding_tool_summaries(&mut out);
        let mut merged_text = String::new();
        let mut j = i;

        loop {
            if !is_thinking_bubble(&vms[j]) {
                break;
            }
            // 含可见文本的 bubble 属「回答段」，不并入工具探索段：
            // 若它是段首（i==j），仍作为单条处理；否则结束当前段，留给下轮。
            if bubble_has_visible_text(&vms[j]) && j > i {
                break;
            }
            // 累加耗时 + 拼接文本
            if let MessageViewModel::AssistantBubble { blocks, .. } = &vms[j] {
                for b in blocks {
                    if let ContentBlockView::Reasoning {
                        text, duration_ms, ..
                    } = b
                    {
                        match duration_ms {
                            Some(ms) => tot_ms += ms,
                            // R6：任一段缺耗时 → 整体降级（避免低估）
                            None => missing_ms = true,
                        }
                        if !merged_text.is_empty() && !text.is_empty() {
                            merged_text.push_str("\n\n");
                        }
                        merged_text.push_str(text);
                    }
                }
            }
            let has_text = bubble_has_visible_text(&vms[j]);

            // 扫描其后连续工具：只读计入数；**非只读（Bash/Write/Edit）跳过但不打断**
            // （PRD §2.8：非只读工具不参与计数，但不打断合并段）
            let mut k = j + 1;
            while k < vms.len() {
                match &vms[k] {
                    MessageViewModel::ToolCallGroup { tools, .. } => {
                        for t in tools {
                            match t.tool_name.as_str() {
                                "Read" => reads += 1,
                                "Glob" => globs += 1,
                                "Grep" => greps += 1,
                                _ => {}
                            }
                        }
                        k += 1;
                    }
                    MessageViewModel::ToolBlock { tool_name, .. } => match tool_name.as_str() {
                        "Read" => {
                            reads += 1;
                            k += 1;
                        }
                        "Glob" => {
                            globs += 1;
                            k += 1;
                        }
                        "Grep" => {
                            greps += 1;
                            k += 1;
                        }
                        // 非只读工具（Bash/Write/Edit）：跳过，不打断合并段，不计入
                        _ => k += 1,
                    },
                    _ => break,
                }
            }

            if has_text {
                j = k;
                break;
            }
            if k < vms.len() && is_thinking_bubble(&vms[k]) {
                j = k; // 紧接下一个思考 bubble → 继续本段
                continue;
            }
            j = k;
            break;
        }

        let seg_end = j; // 段覆盖 [i, seg_end)

        // 写回段首 bubble：累加结果写入**第一个** Reasoning，并**移除其余 Reasoning**
        // （R2：同一 bubble 内多个 Reasoning 时，若不移除会导致渲染文本重复）
        let summary = readonly_action_summary(reads, globs, greps);
        let mut first = vms[i].clone();
        if let MessageViewModel::AssistantBubble { blocks, .. } = &mut first {
            // 定位第一个 Reasoning 的下标
            let first_idx = blocks
                .iter()
                .position(|b| matches!(b, ContentBlockView::Reasoning { .. }));
            if let Some(idx) = first_idx {
                // N1：先算合并后字数（降级为 chars 分支时须显示合并后字数，而非段首旧值）
                let merged_char_count = merged_text.chars().count();
                if let ContentBlockView::Reasoning {
                    text,
                    char_count,
                    duration_ms,
                    action_summary,
                    ..
                } = &mut blocks[idx]
                {
                    *text = merged_text;
                    *char_count = merged_char_count;
                    // R6：任一段缺耗时 → 显式清空（否则残留段首旧值造成低估）
                    *duration_ms = if missing_ms { None } else { Some(tot_ms) };
                    *action_summary = summary;
                }
                // 移除该 bubble 内其余 Reasoning block（文本已并入第一个）
                let mut seen_first = false;
                blocks.retain(|b| {
                    if matches!(b, ContentBlockView::Reasoning { .. }) {
                        if !seen_first {
                            seen_first = true;
                            return true; // 保留第一个
                        }
                        return false; // 移除其余
                    }
                    true
                });
            }
        }
        first.recompute_hash();
        out.push(first);

        // 段内其余 bubble 跳过；保留工具 VM（ToolCallGroup / ToolBlock）
        let mut m = i + 1;
        while m < seg_end {
            if !is_thinking_bubble(&vms[m]) {
                out.push(vms[m].clone()); // 工具 VM 保留
            }
            m += 1;
        }

        i = seg_end.max(i + 1);
    }
    *vms = out;
}

/// 清除归并器生成的派生摘要，以便对已归一化前缀和新增尾部再次归一化。
fn clear_action_summaries(vms: &mut [MessageViewModel]) {
    for vm in vms {
        let mut changed = false;
        match vm {
            MessageViewModel::AssistantBubble { blocks, .. } => {
                for block in blocks {
                    if let ContentBlockView::Reasoning { action_summary, .. } = block {
                        if action_summary.take().is_some() {
                            changed = true;
                        }
                    }
                }
            }
            MessageViewModel::ToolCallGroup {
                standalone_action, ..
            } => {
                if standalone_action.take().is_some() {
                    changed = true;
                }
            }
            _ => {}
        }
        if changed {
            vm.recompute_hash();
        }
    }
}

/// 将尚未归属的独立只读动作摘要从紧邻 Thought 之前的工具段转移到 Thought。
fn absorb_preceding_tool_summaries(out: &mut [MessageViewModel]) -> (usize, usize, usize) {
    let mut reads = 0;
    let mut globs = 0;
    let mut greps = 0;
    let mut start = out.len();

    while start > 0 && is_tool_vm(&out[start - 1]) {
        start -= 1;
    }

    for vm in &mut out[start..] {
        let mut changed = false;
        if let MessageViewModel::ToolCallGroup {
            tools,
            standalone_action,
            ..
        } = vm
        {
            // None 表示该工具组已由更早的 Thought 行统计，不能重复吸收。
            if standalone_action.is_some() {
                for tool in tools {
                    match tool.tool_name.as_str() {
                        "Read" => reads += 1,
                        "Glob" => globs += 1,
                        "Grep" => greps += 1,
                        _ => {}
                    }
                }
                *standalone_action = None;
                changed = true;
            }
        }
        if changed {
            vm.recompute_hash();
        }
    }

    (reads, globs, greps)
}

fn is_tool_vm(vm: &MessageViewModel) -> bool {
    matches!(
        vm,
        MessageViewModel::ToolCallGroup { .. } | MessageViewModel::ToolBlock { .. }
    )
}

/// 该 VM 是否为「含 Reasoning 的 AssistantBubble」
fn is_thinking_bubble(vm: &MessageViewModel) -> bool {
    matches!(
        vm,
        MessageViewModel::AssistantBubble { blocks, .. }
            if blocks.iter().any(|b| matches!(b, ContentBlockView::Reasoning { .. }))
    )
}

/// 该 AssistantBubble 是否含**可见正文文本**（段边界判定）
fn bubble_has_visible_text(vm: &MessageViewModel) -> bool {
    matches!(
        vm,
        MessageViewModel::AssistantBubble { blocks, .. }
            if blocks
                .iter()
                .any(|b| matches!(b, ContentBlockView::Text { raw, .. } if !raw.trim().is_empty()))
    )
}

/// 该 VM 是否为「含 Reasoning 且已注入计数」的 AssistantBubble。
/// 用于判定后续只读工具组是否已被汇总（避免重复显示计数）。
fn is_thinking_bubble_with_summary(vm: &MessageViewModel) -> bool {
    matches!(
        vm,
        MessageViewModel::AssistantBubble { blocks, .. }
            if blocks.iter().any(|b| matches!(
                b,
                ContentBlockView::Reasoning { action_summary: Some(_), .. }
            ))
    )
}
