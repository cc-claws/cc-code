use crate::list::{ListState, SelectableList};
use ratatui::{layout::Rect, style::Style, text::Line, widgets::Clear, Frame};

/// 锚点位置——浮动面板相对于锚点的定位方式
#[derive(Debug, Clone, Copy)]
pub enum Anchor {
    Below { x: u16, y: u16 },
    Above { x: u16, y: u16 },
    Centered,
}

/// 浮动面板位置策略
#[derive(Debug, Clone, Copy, Default)]
pub enum OverlayPosition {
    Auto,
    Below,
    Above,
    #[default]
    Centered,
}

/// ListOverlay 状态——追踪渲染后的面板区域
pub struct ListOverlayState {
    last_area: Option<Rect>,
}

impl ListOverlayState {
    pub fn new() -> Self {
        Self { last_area: None }
    }

    pub fn area(&self) -> Option<Rect> {
        self.last_area
    }
}

impl Default for ListOverlayState {
    fn default() -> Self {
        Self::new()
    }
}

/// 浮动列表容器——组合 SelectableList + 边框 + 锚点定位
///
/// 在指定锚点附近渲染一个带边框的列表，自动处理边界钳位。
pub struct ListOverlay<'a, T> {
    list: SelectableList<'a, T>,
    title: Line<'a>,
    border_style: Style,
    position: OverlayPosition,
    max_height: u16,
    anchor: Anchor,
    width: u16,
}

impl<'a, T> ListOverlay<'a, T> {
    pub fn new(list: SelectableList<'a, T>) -> Self {
        Self {
            list,
            title: Line::from(""),
            border_style: Style::default(),
            position: OverlayPosition::default(),
            max_height: 10,
            anchor: Anchor::Centered,
            width: 30,
        }
    }

    pub fn title(mut self, title: impl Into<Line<'a>>) -> Self {
        self.title = title.into();
        self
    }

    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    pub fn position(mut self, pos: OverlayPosition) -> Self {
        self.position = pos;
        self
    }

    pub fn max_height(mut self, max: u16) -> Self {
        self.max_height = max;
        self
    }

    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn width(mut self, width: u16) -> Self {
        self.width = width;
        self
    }

    /// 渲染浮动列表
    pub fn render(
        self,
        f: &mut Frame,
        viewport: Rect,
        list_state: &mut ListState<T>,
        overlay_state: &mut ListOverlayState,
    ) {
        let content_height = list_state.items().len().min(self.max_height as usize) as u16;
        let panel_height = content_height + 2; // 上下边框各 1 行

        if content_height == 0 {
            overlay_state.last_area = None;
            return;
        }

        let area = self.calculate_area(viewport, panel_height);
        overlay_state.last_area = Some(area);

        // Clear 背景区域
        f.render_widget(Clear, area);

        // 渲染边框
        let block = ratatui::widgets::Block::default()
            .title(self.title.clone())
            .borders(ratatui::widgets::Borders::TOP | ratatui::widgets::Borders::BOTTOM)
            .border_style(self.border_style);
        let inner = block.inner(area);
        f.render_widget(&block, area);

        // 渲染列表内容
        f.render_stateful_widget(self.list, inner, list_state);
    }

    fn calculate_area(&self, viewport: Rect, panel_height: u16) -> Rect {
        let effective_position = match self.position {
            OverlayPosition::Auto | OverlayPosition::Below => {
                // 检查下方空间是否足够
                let below_space = viewport.height.saturating_sub(self.anchor_y());
                if below_space >= panel_height {
                    OverlayPosition::Below
                } else {
                    OverlayPosition::Above
                }
            }
            OverlayPosition::Above => OverlayPosition::Above,
            OverlayPosition::Centered => OverlayPosition::Centered,
        };

        let (x, y) = match effective_position {
            OverlayPosition::Auto | OverlayPosition::Below => {
                let x = self
                    .anchor_x()
                    .min(viewport.width.saturating_sub(self.width));
                let y = self.anchor_y().saturating_add(1);
                (x, y)
            }
            OverlayPosition::Above => {
                let x = self
                    .anchor_x()
                    .min(viewport.width.saturating_sub(self.width));
                let anchor_y = self.anchor_y();
                let space_above = anchor_y.saturating_sub(viewport.y);
                if space_above < panel_height {
                    // 上方空间不足，回退到 Below
                    (x, anchor_y.saturating_add(1))
                } else {
                    (x, anchor_y - panel_height)
                }
            }
            OverlayPosition::Centered => {
                let x = (viewport.width.saturating_sub(self.width)) / 2;
                let y = (viewport.height.saturating_sub(panel_height)) / 2;
                (x, y)
            }
        };

        Rect::new(x, y, self.width, panel_height)
    }

    fn anchor_x(&self) -> u16 {
        match self.anchor {
            Anchor::Below { x, .. } | Anchor::Above { x, .. } => x,
            Anchor::Centered => 0,
        }
    }

    fn anchor_y(&self) -> u16 {
        match self.anchor {
            Anchor::Below { y, .. } | Anchor::Above { y, .. } => y,
            Anchor::Centered => 0,
        }
    }
}

#[cfg(test)]
#[path = "list_overlay_test.rs"]
mod tests;
