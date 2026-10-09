use cc_middlewares::prelude::{BatchItem, HitlDecision};

// ─── PendingAttachment ────────────────────────────────────────────────────────

/// 待发送的图片附件（Ctrl+V 从剪贴板粘贴）
pub struct PendingAttachment {
    /// 显示名称，如 "clipboard_1.png"
    pub label: String,
    /// MIME 类型，固定为 "image/png"
    pub media_type: String,
    /// base64 编码的 PNG 数据
    pub base64_data: String,
    /// PNG 文件大小（字节，用于显示）
    pub size_bytes: usize,
    /// textarea 内嵌占位符 `[Image #N]` 中的稳定 ID，用于提交时映射。
    /// SessionMetadata.next_image_id 单调递增分配。
    pub image_id: usize,
}

// ─── HitlBatchPrompt ──────────────────────────────────────────────────────────

/// 单项审批选择（三态）：一次性同意 / 本次会话同意 / 拒绝。
///
/// 对齐 ACP `PermissionOption`：`allow_once` / `allow_always` / `reject_once`。
/// 「本次会话同意」写入会话级审批记忆：文件按工具与路径，命令按完整命令与执行目录。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalChoice {
    /// 一次性同意（仅本次调用）
    Once,
    /// 本次会话同意（写入审批记忆）
    Session,
    /// 拒绝
    Reject,
}

impl ApprovalChoice {
    pub const ALL: [Self; 3] = [Self::Once, Self::Session, Self::Reject];

    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|choice| *choice == self)
            .unwrap_or(0)
    }

    /// ACP `PermissionOption` id
    pub fn option_id(self) -> &'static str {
        match self {
            Self::Once => "allow_once",
            Self::Session => "allow_always",
            Self::Reject => "reject_once",
        }
    }

    /// 是否放行
    pub fn is_approved(self) -> bool {
        !matches!(self, Self::Reject)
    }
}

/// 批量 HITL 弹窗状态：每项独立的审批选择
pub struct HitlBatchPrompt {
    /// 待审批的工具调用列表
    pub items: Vec<BatchItem>,
    /// 每项的当前选择（三态）
    pub choices: Vec<ApprovalChoice>,
    /// 当前光标所在的行（工具索引）
    pub cursor: usize,
    /// 渲染时记录的内容区可见行数（hitl_move 据此判断是否滚动）
    pub last_visible_height: u16,
    /// 当前滚动偏移（行）。Paragraph::scroll 用，让光标保持可见。
    pub scroll_offset: u16,
    /// 回复 channel
    pub response_tx: tokio::sync::oneshot::Sender<Vec<HitlDecision>>,
}

impl HitlBatchPrompt {
    pub fn new(
        items: Vec<BatchItem>,
        response_tx: tokio::sync::oneshot::Sender<Vec<HitlDecision>>,
    ) -> Self {
        let len = items.len();
        Self {
            items,
            // 默认一次性同意（最保守的放行档）
            choices: vec![ApprovalChoice::Once; len],
            cursor: 0,
            last_visible_height: 0,
            scroll_offset: 0,
            response_tx,
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let len = self.items.len();
        if len == 0 {
            return;
        }
        self.cursor = ((self.cursor as isize + delta).rem_euclid(len as isize)) as usize;
        self.keep_choice_visible();
    }

    /// 上下键直接选择当前工具的审批选项，到边界后停留。
    pub fn move_choice(&mut self, delta: isize) {
        if let Some(choice) = self.choices.get_mut(self.cursor) {
            let index = (choice.index() as isize + delta).clamp(0, 2) as usize;
            *choice = ApprovalChoice::ALL[index];
        }
        self.keep_choice_visible();
    }

    /// 每项固定五行：工具、参数、三个选项；滚动跟随实际选项行。
    pub fn keep_choice_visible(&mut self) {
        let Some(choice) = self.choices.get(self.cursor) else {
            return;
        };
        let cursor_row = self
            .cursor
            .saturating_mul(5)
            .saturating_add(2 + choice.index());
        let cursor_row = u16::try_from(cursor_row).unwrap_or(u16::MAX);
        let vis = if self.last_visible_height > 0 {
            self.last_visible_height
        } else {
            10 // fallback：未渲染前用保守值
        };
        // 可容纳一整项时同时保留工具信息和三个选项；极小窗口才只跟随选中行。
        let item_start = u16::try_from(self.cursor.saturating_mul(5)).unwrap_or(u16::MAX);
        let (first_row, last_row) = if vis >= 5 {
            (item_start, item_start.saturating_add(4))
        } else {
            (cursor_row, cursor_row)
        };
        if first_row < self.scroll_offset {
            self.scroll_offset = first_row;
        } else if last_row >= self.scroll_offset.saturating_add(vis) {
            self.scroll_offset = last_row.saturating_add(1).saturating_sub(vis);
        }
        let content_height = u16::try_from(self.items.len().saturating_mul(5)).unwrap_or(u16::MAX);
        self.scroll_offset = self.scroll_offset.min(content_height.saturating_sub(vis));
    }

    /// 全部设为一次性同意
    pub fn approve_all(&mut self) {
        self.choices
            .iter_mut()
            .for_each(|v| *v = ApprovalChoice::Once);
    }

    /// 全部拒绝
    pub fn reject_all(&mut self) {
        self.choices
            .iter_mut()
            .for_each(|v| *v = ApprovalChoice::Reject);
    }

    /// 确认并发送决策。
    ///
    /// 三态 → `HitlDecision` 的映射：`Once`/`Session` 都是 `Approve`（scope 差异在 ACP
    /// 应答层用 `option_id` 表达，不经由此通道）；`Reject` → `Reject`。
    pub fn confirm(self) {
        let decisions: Vec<HitlDecision> = self
            .choices
            .iter()
            .map(|&c| {
                if c.is_approved() {
                    HitlDecision::Approve
                } else {
                    HitlDecision::Reject
                }
            })
            .collect();
        let _ = self.response_tx.send(decisions);
    }
}

#[cfg(test)]
#[path = "hitl_prompt_test.rs"]
mod tests;
