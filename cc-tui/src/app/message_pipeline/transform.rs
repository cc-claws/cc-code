use cc_agent::messages::BaseMessage;
use ratatui::text::Text;

use crate::{
    app::tool_display,
    ui::message_view::{aggregate_tool_groups, tool_color, ContentBlockView, MessageViewModel},
};

use super::MessagePipeline;

/// 流式尾部未完成行的挂起阈值（字符数）。
///
/// 超过该长度时，最后一个换行之后的内容暂不进入 VM，等换行到达再一次性显示，
/// 避免长段落每帧重排最后一行。阈值以内的短句照常逐字流出，保证首字即时可见。
const STREAMING_TAIL_HOLD_CHARS: usize = 200;

/// 计算流式气泡应显示的文本范围。
///
/// 尚未出现换行时整段照常显示（短回答保持逐字观感）；出现换行后，
/// 若未完成行超过 [`STREAMING_TAIL_HOLD_CHARS`] 则先隐藏它。
fn visible_streaming_text(text: &str) -> &str {
    let Some(nl) = text.rfind('\n') else {
        return text;
    };
    let tail = &text[nl + 1..];
    if tail.chars().count() >= STREAMING_TAIL_HOLD_CHARS {
        &text[..=nl]
    } else {
        text
    }
}

impl MessagePipeline {
    /// 构建当前流式 AI 消息的 AssistantBubble ViewModel。
    ///
    /// 包含 Reasoning block + 已输出的文本 + 已完成的 tool_use blocks。
    /// 不包含 pending tools——它们在 build_tail_vms 中另行处理。
    ///
    /// **不在 pipeline 侧解析 markdown**：文本块以 `dirty=true` + 空 `rendered` 交出去，
    /// 由渲染线程的 [`crate::ui::markdown::ensure_rendered_incremental`] 按增量解析。
    /// 早期版本在这里全量解析并声称已渲染（`rendered_prefix_len = raw.len()`），
    /// 使渲染线程的增量路径直接短路，每帧重复解析整篇文本。
    pub fn build_streaming_bubble(&self) -> MessageViewModel {
        let mut blocks: Vec<ContentBlockView> = Vec::new();
        // 只有"思考已落进 completed"时才停止从流式气泡输出，避免重复渲染；
        // 仅 finalize（ToolStart）不足以判定——那样会导致思考气泡消失、
        // 与工具组无法合并，计数丢失（真机表现为 Thought 行没有计数）。
        if !self.reasoning_flushed && !self.current_ai_reasoning.is_empty() {
            // 跟随后台段落状态：还在思考 → Thinking（走秒交给 active_since）；
            // 一旦正文到达（段落已冻结）→ 立刻变成 Thought for Ns。
            let thinking_now = self.group_active && self.current_ai_text.trim().is_empty();
            blocks.push(ContentBlockView::Reasoning {
                char_count: self.current_ai_reasoning.chars().count(),
                duration_ms: Some(if thinking_now { self.group_ms } else { self.group_ms }),
                action_summary: None,
                text: self.current_ai_reasoning.clone(),
                active: thinking_now,
                active_since: self.last_event_at,
                hint: None,
            });
        }
        let display_text = visible_streaming_text(&self.current_ai_text);
        if !display_text.trim().is_empty() {
            let mut scanner = crate::ui::markdown::TableHoldbackScanner::new();
            scanner.set_streaming(true);
            blocks.push(ContentBlockView::Text {
                raw: display_text.to_string(),
                rendered: Text::raw(""),
                rendered_links: Vec::new(),
                dirty: true,
                rendered_prefix_len: 0,
                rendered_prefix_lines: 0,
                rendered_width: 0,
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
    /// 历史恢复入口，与流式尾部复用同一底层消息转换和聚合逻辑。
    pub fn messages_to_view_models(msgs: &[BaseMessage], cwd: &str) -> Vec<MessageViewModel> {
        let mut vms = Self::unaggregated_view_models(msgs, cwd);
        aggregate_tool_groups(&mut vms);
        merge_consecutive_thinking(&mut vms);
        vms
    }

    /// 活跃尾部先拼接流式工具并恢复运行时状态，再统一聚合，避免在快照边界拆组。
    pub(super) fn unaggregated_view_models(
        msgs: &[BaseMessage],
        cwd: &str,
    ) -> Vec<MessageViewModel> {
        let mut vms: Vec<MessageViewModel> = Vec::with_capacity(msgs.len());
        let mut prev_ai_tool_calls: Vec<(String, String, serde_json::Value)> = Vec::new();

        for msg in msgs {
            // System 消息（system prompt / compact summary）是内部状态，不应渲染
            if matches!(msg, BaseMessage::System { .. }) {
                continue;
            }

            // 前台 `!` 命令回流的上下文片段：仅用于喂给模型，展示由 ShellCommand VM 负责。
            // 若不跳过，resume/实时重建时会出现与 ShellCommand VM 重复的气泡。
            // caveat 同理（peri 无 isMeta 字段，只能靠前缀识别）。
            //
            // 判定要求「前缀 + 闭合标签」双重匹配：仅靠 starts_with 会误伤用户手输的
            // 裸标签（如单输 `<bash-input>`），且这是唯一可用的识别手段（无 isMeta）。
            if let BaseMessage::Human { content, .. } = msg {
                let raw = content.text_content();
                // compact 恢复的技能只有上下文作用，不重复生成技能加载卡片。
                if crate::ui::message_view::skill_invocation_display(&raw)
                    .is_some_and(|invocation| !invocation.display)
                {
                    continue;
                }
                if crate::app::is_shell_context_fragment(&raw) {
                    continue;
                }
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

        vms
    }

    /// Reconcile：从当前 completed 状态重建完整的 view_models。
    ///
    /// 在 "finalize 边界"（ToolStart / Done）调用，确保流式最终状态
    /// 与 restore 路径 `messages_to_view_models()` 完全一致。
    pub fn reconcile(&self) -> Vec<MessageViewModel> {
        let mut vms = Self::messages_to_view_models(&self.completed, &self.cwd);
        self.mark_live_thinking(&mut vms);
        vms
    }

    /// 标记"仍在进行中"的思考段：渲染层据此显示
    /// `● Thinking for Ns, <现在时计数>…`（对齐参考实现 Jc 的走秒 + 现在时）。
    ///
    /// 锚点 `reasoning_started_at` 在本轮结束时（`done()`）清空，
    /// 因此只要有锚点就说明还在这一轮里。
    pub(crate) fn mark_live_thinking(&self, vms: &mut [MessageViewModel]) {
        if self.group_ms == 0 && self.finished_groups.is_empty() && self.last_event_at.is_none() {
            return;
        }
        // 把每一组的间隔累计贴到对应的 Thought 行上（从后往前对齐）：
        // 最新一组用 group_ms + 锚点（活跃时走秒），更早的组用已冻结值。
        // 先清掉所有残留的 active：只有最新的那一组可以是 Thinking。
        for vm in vms.iter_mut() {
            let MessageViewModel::AssistantBubble { blocks, .. } = vm else {
                continue;
            };
            for b in blocks.iter_mut() {
                if let ContentBlockView::Reasoning {
                    active,
                    active_since,
                    ..
                } = b
                {
                    *active = false;
                    *active_since = None;
                }
            }
        }
        let mut groups = self.finished_groups.iter().rev();
        // 只有"当前组还没结束且还没开始输出正文"才算活跃；
        // 正文一旦出现（包括 block 模式路径）立即回落成 Thought。
        let active = self.group_active && self.current_ai_text.trim().is_empty();
        let mut first = true;
        for vm in vms.iter_mut().rev() {
            let MessageViewModel::AssistantBubble { blocks, .. } = vm else {
                continue;
            };
            let target = blocks
                .iter_mut()
                .rev()
                .find(|b| matches!(b, ContentBlockView::Reasoning { .. }));
            let Some(ContentBlockView::Reasoning {
                duration_ms,
                active: a,
                active_since,
                ..
            }) = target
            else {
                continue;
            };
            if first {
                *duration_ms = Some(self.group_ms.max(1000));
                *a = active;
                *active_since = self.last_event_at;
                first = false;
            } else if let Some(ms) = groups.next() {
                *duration_ms = Some((*ms).max(1000));
                *a = false;
                *active_since = None;
            }
        }
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
        // 思考段已落定：把锚点推到"最后一段思考结束的时刻"，运行中的走秒从这里继续累加
        // （对齐参考实现 Jc 的 baseMs + now - lastThinkingAt）。
        if self.reasoning_started_at.is_some() {
            self.reasoning_started_at = Some(std::time::Instant::now());
        }
    }

    /// 构建 ToolStart 的 ToolBlock VM（与 from_base_message_with_cwd 的 Tool 路径一致）
    pub(crate) fn build_tool_start_vm(
        &self,
        tool_call_id: &str,
        name: &str,
        input: &serde_json::Value,
        started_at: Option<std::time::Instant>,
    ) -> MessageViewModel {
        let display_name = tool_display::format_tool_name_with_input(name, Some(input));
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
            phase: crate::ui::message_view::ToolPhase::Running,
            live_output: None,
            source_agent_id: None,
            started_at,
            timeout_ms: input.get("timeout").and_then(serde_json::Value::as_u64),
            shell_backgrounded: false,
            file_inspect: crate::ui::message_view::is_file_inspect(name, input),
            content_hash: 0,
        };
        vm.recompute_hash();
        vm
    }
}

fn is_memory_path(path: Option<&str>) -> bool {
    if let Some(p) = path {
        let n = p.replace('\\', "/").to_lowercase();
        n.contains("/memory/") || n.ends_with("/memory") || n.ends_with("/memory.md")
    } else {
        false
    }
}

/// 只读与记忆工具动作计数文案（对齐 CC 优雅渲染）：
/// `read N file(s)` / `listed N director(y|ies)` / `searched for N pattern(s)` /
/// `ran N shell command(s)` / `recalled N memor(y|ies)` / `updated N memor(y|ies)`
pub fn readonly_action_summary_ext(
    reads: usize,
    globs: usize,
    greps: usize,
    shell_cmds: usize,
    recalled: usize,
    updated: usize,
) -> Option<String> {
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
    if shell_cmds > 0 {
        parts.push(format!(
            "ran {shell_cmds} shell command{}",
            if shell_cmds > 1 { "s" } else { "" }
        ));
    }
    if recalled > 0 {
        parts.push(format!(
            "recalled {recalled} memor{}",
            if recalled > 1 { "ies" } else { "y" }
        ));
    }
    if updated > 0 {
        parts.push(format!(
            "updated {updated} memor{}",
            if updated > 1 { "ies" } else { "y" }
        ));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

#[allow(dead_code)]
pub fn readonly_action_summary(reads: usize, globs: usize, greps: usize) -> Option<String> {
    readonly_action_summary_ext(reads, globs, greps, 0, 0, 0)
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
/// 从 `from` 起重跑一次合并（前缀与尾部之间跨边界时用）。
///
/// 只重建 `vms[from..]`，前缀之前的 VM 保持原样；调用方需保证 `from` 落在
/// "上一段思考/工具"的起点附近，否则段会被错误切断。
pub(crate) fn retry_merge_from(vms: &mut Vec<MessageViewModel>, from: usize) {
    if from >= vms.len() {
        return;
    }
    let mut tail: Vec<MessageViewModel> = vms.split_off(from);
    clear_action_summaries(&mut tail);
    merge_consecutive_thinking(&mut tail);
    vms.extend(tail);
}

pub(crate) fn merge_consecutive_thinking(vms: &mut Vec<MessageViewModel>) {
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
                    let mut reads = 0;
                    let mut globs = 0;
                    let mut greps = 0;
                    let mut shell_cmds = 0;
                    let mut recalled = 0;
                    let mut updated = 0;
                    for t in tools.iter() {
                        let is_mem = is_memory_path(t.args_display.as_deref());
                        match t.tool_name.as_str() {
                            "Read" if is_mem => recalled += 1,
                            "Read" => reads += 1,
                            "Glob" => globs += 1,
                            "Grep" => greps += 1,
                            "Bash" if t.file_inspect => shell_cmds += 1,
                            "Write" | "Edit" if is_mem => updated += 1,
                            _ => {}
                        }
                    }
                    *standalone_action = readonly_action_summary_ext(
                        reads, globs, greps, shell_cmds, recalled, updated,
                    );
                    vm.recompute_hash();
                }
            }
            out.push(vm);
            i += 1;
            continue;
        }

        // 收集段：bubble 与其后连续工具交替，直到边界
        let (mut tot_ms, mut missing_ms) = (0u64, false);
        // 段落里"最新的动作"（供 `⎿` 提示行显示：思考→摘要、read→路径、bash→命令）
        let mut last_hint: Option<String> = None;
        // 只统计**本段内**的工具（不再跨段吸收前面的工具计数）：
        // 跨段吸收会把上一组工具的计数抢到这一行，同时让工具块排到思考行之前，
        // 造成"Thinking 行出现在工具块后面 + 计数归错行"。
        let (mut reads, mut globs, mut greps, mut shell_cmds, mut recalled, mut updated) =
            (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
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
                            // 对齐参考实现：完成态耗时回落为**最后一段**的时长
                            // （活跃态的走秒由渲染层用锚点增量补足，见 Thinking for Ns）。
                            Some(ms) => tot_ms = *ms,
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
                        if let Some(t) = tools.last() {
                            last_hint = t.args_display.clone();
                        }
                        for t in tools {
                            let is_mem = is_memory_path(t.args_display.as_deref());
                            match t.tool_name.as_str() {
                                "Read" if is_mem => recalled += 1,
                                "Read" => reads += 1,
                                "Glob" => globs += 1,
                                "Grep" => greps += 1,
                                "Bash" if t.file_inspect => shell_cmds += 1,
                                "Write" | "Edit" if is_mem => updated += 1,
                                _ => {}
                            }
                        }
                        k += 1;
                    }
                    MessageViewModel::ToolBlock {
                        tool_name,
                        args_display,
                        ..
                    } => {
                        let is_mem = is_memory_path(args_display.as_deref());
                        // 不从 ToolBlock 取提示：留在顶层的块会自己单条渲染，
                        // 若同时当 hint 会让同一条命令出现两次（只有入组的条目才配当提示）。
                        match tool_name.as_str() {
                            "Read" if is_mem => recalled += 1,
                            "Read" => reads += 1,
                            "Glob" => globs += 1,
                            "Grep" => greps += 1,
                            "Write" | "Edit" if is_mem => updated += 1,
                            _ => {}
                        }
                        k += 1;
                    }
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
        let summary =
            readonly_action_summary_ext(reads, globs, greps, shell_cmds, recalled, updated);
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
                    hint,
                    ..
                } = &mut blocks[idx]
                {
                    *text = merged_text;
                    *char_count = merged_char_count;
                    // R6：任一段缺耗时 → 显式清空（否则残留段首旧值造成低估）
                    *duration_ms = if missing_ms { None } else { Some(tot_ms) };
                    *hint = last_hint.clone();
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
pub(crate) fn clear_action_summaries(vms: &mut [MessageViewModel]) {
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
