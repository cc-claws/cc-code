use super::{MessageViewModel, SegmentPayload, ViewportAnchor, VirtualRenderer};
use std::sync::Arc;

fn anchor_at_row(index: usize, payload: &SegmentPayload, row: usize) -> ViewportAnchor {
    let line = payload
        .wrap
        .partition_point(|line| line.visual_row_end <= row)
        .min(payload.wrap.len().saturating_sub(1));
    ViewportAnchor {
        message_index: index,
        logical_line: payload.origins.get(line).copied().unwrap_or(0),
        row_in_line: payload
            .wrap
            .get(line)
            .map_or(0, |line| row.saturating_sub(line.visual_row_start)),
        relative_rows: 0,
    }
}

impl VirtualRenderer {
    /// 跨过的每条消息都先实测；估算高度不能用于消耗用户的滚动距离。
    pub fn resolve_relative_anchor(
        &mut self,
        messages: &mut [Arc<MessageViewModel>],
        anchor: ViewportAnchor,
        tick: u64,
    ) -> ViewportAnchor {
        self.resolve_relative_anchor_with_overflow(messages, anchor, tick)
            .0
    }

    /// 返回越过真实顶部的剩余距离，欢迎卡占位也必须逐步参与钳位。
    pub fn resolve_relative_anchor_with_overflow(
        &mut self,
        messages: &mut [Arc<MessageViewModel>],
        anchor: ViewportAnchor,
        tick: u64,
    ) -> (ViewportAnchor, usize) {
        let mut index = anchor.message_index.min(messages.len().saturating_sub(1));
        let mut payload = self.payload(index, messages, tick);
        let line = payload
            .origins
            .partition_point(|line| *line < anchor.logical_line)
            .min(payload.wrap.len().saturating_sub(1));
        let mut row = payload.wrap.get(line).map_or(0, |line| {
            line.visual_row_start
                + anchor.row_in_line.min(
                    line.visual_row_end
                        .saturating_sub(line.visual_row_start + 1),
                )
        });
        let mut remaining = anchor.relative_rows.unsigned_abs();
        let mut overflow = 0;
        if anchor.relative_rows < 0 {
            while remaining > row && index > 0 {
                remaining -= row;
                index -= 1;
                payload = self.payload(index, messages, tick);
                row = payload.total;
            }
            overflow = remaining.saturating_sub(row);
            row = row.saturating_sub(remaining);
        } else {
            while remaining >= payload.total.saturating_sub(row) && index + 1 < messages.len() {
                remaining -= payload.total.saturating_sub(row);
                index += 1;
                payload = self.payload(index, messages, tick);
                row = 0;
            }
            row = row
                .saturating_add(remaining)
                .min(payload.total.saturating_sub(1));
        }
        (anchor_at_row(index, &payload, row), overflow)
    }
}

#[cfg(test)]
#[path = "scroll_test.rs"]
mod tests;
