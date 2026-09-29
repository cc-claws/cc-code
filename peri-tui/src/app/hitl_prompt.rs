use peri_middlewares::prelude::{BatchItem, HitlDecision};

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
/// 「本次会话同意」会写入会话级审批记忆，同 `(工具, 路径)` 后续免问。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalChoice {
    /// 一次性同意（仅本次调用）
    Once,
    /// 本次会话同意（写入审批记忆，同工具+路径后续免问）
    Session,
    /// 拒绝
    Reject,
}

impl ApprovalChoice {
    /// Space 键循环：Once → Session → Reject → Once
    pub fn next(self) -> Self {
        match self {
            Self::Once => Self::Session,
            Self::Session => Self::Reject,
            Self::Reject => Self::Once,
        }
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
        // 光标跟随：每项渲染 2 行（tool 名 + 参数预览），用 cursor_row = cursor*2
        // 作为实际行号近似（足够防止光标移出可视区，误差 1-2 行可由 visible_height
        // 的尾数吸收）。底部统计行 +1 但作为缓冲不纳入计算。
        let cursor_row = (self.cursor as u16).saturating_mul(2);
        let vis = if self.last_visible_height > 0 {
            self.last_visible_height
        } else {
            10 // fallback：未渲染前用保守值
        };
        // 钳位到 [vis/3, vis-1] 区间：光标进入上方 1/3 时上滚，
        // 接近底部（最后一行）时下滚。注释与代码对齐（cc-claws PR #76 review）。
        let lower = vis / 3;
        let upper = vis.saturating_sub(1);
        if cursor_row < self.scroll_offset.saturating_add(lower) {
            // 光标进入上方缓冲区，往上滚
            self.scroll_offset = cursor_row.saturating_sub(lower);
        } else if cursor_row >= self.scroll_offset + upper {
            // 光标超出底部，往下滚
            self.scroll_offset = cursor_row.saturating_sub(upper) + 1;
        }
    }

    /// 循环切换当前项的三态选择（Once → Session → Reject → Once）
    pub fn toggle_current(&mut self) {
        if let Some(v) = self.choices.get_mut(self.cursor) {
            *v = v.next();
        }
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
