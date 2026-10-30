#[cfg(test)]
use crate::ui::render_thread::SelectionAnchor;
use crate::ui::render_thread::{RenderCache, ViewportAnchor};

impl RenderCache {
    /// 只从已物化行提取锚点；估算区域没有可用的精确行内坐标。
    pub fn viewport_anchor_at(&self, top: usize) -> Option<ViewportAnchor> {
        let index = self
            .wrap_map
            .partition_point(|line| line.visual_row_end <= top);
        let line = self.wrap_map.get(index)?;
        if top < line.visual_row_start {
            return None;
        }
        let origins = self
            .virtual_layout
            .as_ref()
            .map_or(self.line_origins.as_slice(), |layout| &layout.line_origins);
        let &(message_index, logical_line) = origins.get(line.line_idx)?;
        Some(ViewportAnchor {
            message_index,
            logical_line,
            row_in_line: top - line.visual_row_start,
        })
    }

    /// 在经典缓存的新布局中恢复消息内的位置；折叠或裁剪后钳到同一消息的可见行。
    pub fn visual_row_for_anchor(&self, anchor: ViewportAnchor) -> Option<usize> {
        if self.virtual_layout.is_some() {
            return None;
        }
        let target = (anchor.message_index, anchor.logical_line);
        let next = self.line_origins.partition_point(|origin| *origin < target);
        let index = if self
            .line_origins
            .get(next)
            .is_some_and(|origin| origin.0 == target.0)
        {
            next
        } else {
            let previous = next.checked_sub(1)?;
            (self.line_origins.get(previous)?.0 == target.0).then_some(previous)?
        };
        let line = self.wrap_map.get(index)?;
        Some(
            line.visual_row_start.saturating_add(
                anchor.row_in_line.min(
                    line.visual_row_end
                        .saturating_sub(line.visual_row_start + 1),
                ),
            ),
        )
    }

    /// 将终端显示列转换为字素偏移，并固定消息的内容版本。
    #[cfg(test)]
    pub fn selection_anchor_at(
        &self,
        row: usize,
        column: u16,
        width: u16,
    ) -> Option<SelectionAnchor> {
        let layout = self.virtual_layout.as_ref()?;
        crate::app::text_selection::selection_anchor_from_parts(
            layout,
            &self.wrap_map,
            row,
            column,
            width,
        )
    }

    #[cfg(test)]
    pub fn link_at_selection_anchor(&self, anchor: SelectionAnchor) -> Option<String> {
        let layout = self.virtual_layout.as_ref()?;
        crate::app::text_selection::link_from_parts(layout, &self.links, anchor)
    }
}

#[cfg(test)]
#[path = "interaction_test.rs"]
mod tests;
