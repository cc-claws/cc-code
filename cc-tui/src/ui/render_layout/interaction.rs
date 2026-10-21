#[cfg(test)]
use crate::ui::render_thread::SelectionAnchor;
use crate::ui::render_thread::{RenderCache, ViewportAnchor};

impl RenderCache {
    /// 只从已物化行提取锚点；估算区域没有可用的精确行内坐标。
    pub fn viewport_anchor_at(&self, top: usize) -> Option<ViewportAnchor> {
        let layout = self.virtual_layout.as_ref()?;
        let index = self
            .wrap_map
            .partition_point(|line| line.visual_row_end <= top);
        let line = self.wrap_map.get(index)?;
        if top < line.visual_row_start {
            return None;
        }
        let &(message_index, logical_line) = layout.line_origins.get(line.line_idx)?;
        Some(ViewportAnchor {
            message_index,
            logical_line,
            row_in_line: top - line.visual_row_start,
        })
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
