use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarAction {
    Checkout,
    CreateTag,
    CreateBranch,
    Merge,
    CherryPick,
    Reset,
    DeleteBranch,
    StashPop,
    StashDrop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalAction {
    RemoteFetch,
    RemotePull,
    RemotePush,
    ToggleBranches,
    ToggleTags,
    ToggleStash,
    FileSearch,
}

pub struct ToolbarButton {
    pub label: &'static str,
    pub action: ToolbarAction,
    pub group: u8,
}

#[allow(dead_code)]
pub struct GlobalToolbarButton {
    pub label: &'static str,
    pub shortcut: char,
    pub action: GlobalAction,
    pub group: u8,
}

pub fn global_buttons() -> Vec<GlobalToolbarButton> {
    vec![
        GlobalToolbarButton {
            label: "fetch",
            shortcut: 'f',
            action: GlobalAction::RemoteFetch,
            group: 0,
        },
        GlobalToolbarButton {
            label: "push",
            shortcut: 'p',
            action: GlobalAction::RemotePush,
            group: 0,
        },
        GlobalToolbarButton {
            label: "pull",
            shortcut: 'P',
            action: GlobalAction::RemotePull,
            group: 0,
        },
        GlobalToolbarButton {
            label: "branches",
            shortcut: 'b',
            action: GlobalAction::ToggleBranches,
            group: 1,
        },
        GlobalToolbarButton {
            label: "tags",
            shortcut: 't',
            action: GlobalAction::ToggleTags,
            group: 1,
        },
        GlobalToolbarButton {
            label: "stash",
            shortcut: 's',
            action: GlobalAction::ToggleStash,
            group: 1,
        },
        GlobalToolbarButton {
            label: "files",
            shortcut: 'p', // Ctrl+P
            action: GlobalAction::FileSearch,
            group: 2,
        },
    ]
}

/// 获取基于当前选中 commit 的操作按钮
pub fn commit_buttons(app: &App) -> Vec<ToolbarButton> {
    let mut buttons = vec![
        ToolbarButton {
            label: "checkout",
            action: ToolbarAction::Checkout,

            group: 0,
        },
        ToolbarButton {
            label: "tag",
            action: ToolbarAction::CreateTag,

            group: 0,
        },
        ToolbarButton {
            label: "branch",
            action: ToolbarAction::CreateBranch,

            group: 0,
        },
        ToolbarButton {
            label: "merge",
            action: ToolbarAction::Merge,

            group: 1,
        },
        ToolbarButton {
            label: "pick",
            action: ToolbarAction::CherryPick,

            group: 1,
        },
        ToolbarButton {
            label: "reset",
            action: ToolbarAction::Reset,

            group: 2,
        },
    ];

    if let Some(detail) = &app.selected_detail {
        if !detail.branches.is_empty() {
            buttons.push(ToolbarButton {
                label: "del",
                action: ToolbarAction::DeleteBranch,

                group: 3,
            });
        }
    }

    if let Some(oid) = app.selected_oid {
        if app.stash_map.contains_key(&oid) {
            buttons.push(ToolbarButton {
                label: "pop",
                action: ToolbarAction::StashPop,

                group: 4,
            });
            buttons.push(ToolbarButton {
                label: "drop",
                action: ToolbarAction::StashDrop,

                group: 4,
            });
        }
    }

    buttons
}

/// 追踪按钮位置用于点击检测
pub struct ToolbarState {
    pub button_starts: Vec<u16>,
    pub button_widths: Vec<u16>,
    pub y: u16,
    pub width_per_button: u16,
}

impl ToolbarState {
    pub fn new() -> Self {
        Self {
            button_starts: Vec::new(),
            button_widths: Vec::new(),
            y: 0,
            width_per_button: 10,
        }
    }

    pub fn hit_test(&self, col: u16, row: u16) -> Option<usize> {
        if row != self.y {
            return None;
        }
        for (i, &start) in self.button_starts.iter().enumerate() {
            let width = self
                .button_widths
                .get(i)
                .copied()
                .unwrap_or(self.width_per_button);
            if col >= start && col < start + width {
                return Some(i);
            }
        }
        None
    }
}

/// commit 工具栏每个按钮独立配色（按按钮在 vec 中的位置分配）
const COMMIT_BG_PALETTE: [Color; 9] = [
    Color::Rgb(50, 100, 140), // checkout — 蓝绿
    Color::Rgb(130, 110, 40), // tag — 金
    Color::Rgb(100, 50, 130), // branch — 紫
    Color::Rgb(40, 120, 80),  // merge — 绿
    Color::Rgb(140, 90, 40),  // pick — 橙
    Color::Rgb(140, 40, 40),  // reset — 深红
    Color::Rgb(160, 70, 50),  // del — 砖红（与 reset 区分）
    Color::Rgb(50, 110, 120), // pop — 青
    Color::Rgb(120, 60, 40),  // drop — 暗红棕
];

pub fn draw_toolbar(
    f: &mut Frame,
    area: Rect,
    buttons: &[ToolbarButton],
    state: &mut ToolbarState,
) {
    state.button_starts.clear();
    state.button_widths.clear();
    state.y = area.y;

    let mut spans: Vec<Span> = Vec::new();
    let mut x = area.x;
    let mut prev_group: Option<u8> = None;
    for (i, btn) in buttons.iter().enumerate() {
        // 不同组之间加空格分隔
        if prev_group.is_some_and(|pg| pg != btn.group) {
            spans.push(Span::raw(" "));
            x += 1;
        }
        prev_group = Some(btn.group);

        let text = format!(" {} ", btn.label);
        let text_width = UnicodeWidthStr::width(text.as_str()) as u16;
        state.button_starts.push(x);
        state.button_widths.push(text_width);
        let bg = COMMIT_BG_PALETTE[i % COMMIT_BG_PALETTE.len()];
        spans.push(Span::styled(
            text.clone(),
            Style::default().fg(Color::White).bg(bg),
        ));
        x += text_width;
    }

    let para = Paragraph::new(Line::from(spans));
    f.render_widget(para, area);
}

/// 全局工具栏状态追踪
pub struct GlobalToolbarState {
    pub button_starts: Vec<u16>,
    pub y: u16,
}

impl GlobalToolbarState {
    pub fn new() -> Self {
        Self {
            button_starts: Vec::new(),
            y: 0,
        }
    }

    pub fn hit_test(&self, col: u16, row: u16) -> Option<usize> {
        if row != self.y {
            return None;
        }
        let mut prev_end = 0u16;
        for (i, &start) in self.button_starts.iter().enumerate() {
            let end = if i + 1 < self.button_starts.len() {
                self.button_starts[i + 1]
            } else {
                start + 200
            };
            if col >= start && col < end {
                return Some(i);
            }
            prev_end = end;
        }
        let _ = prev_end;
        None
    }
}

pub fn draw_global_toolbar(f: &mut Frame, area: Rect, app: &mut App) {
    let buttons = global_buttons();
    app.global_toolbar_state.button_starts.clear();
    app.global_toolbar_state.y = area.y;

    let mut spans: Vec<Span> = Vec::new();
    let mut x = area.x;

    // 左侧：分支名 + ahead/behind + dirty 标记
    if let Some(branch) = app.repo.head_branch() {
        spans.push(Span::styled(
            format!(" {} ", branch),
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(80, 50, 130))
                .add_modifier(Modifier::BOLD),
        ));
        x += UnicodeWidthStr::width(format!(" {} ", branch).as_str()) as u16;

        // ahead/behind 标记
        if let Some((ahead, behind)) = app.ahead_behind {
            if ahead > 0 || behind > 0 {
                let mut ab_text = String::new();
                if behind > 0 {
                    ab_text = format!("↓{}", behind);
                }
                if ahead > 0 {
                    if !ab_text.is_empty() {
                        ab_text.push(' ');
                    }
                    ab_text.push_str(&format!("↑{}", ahead));
                }
                let styled = format!(" {} ", ab_text);
                spans.push(Span::styled(
                    styled.clone(),
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Rgb(40, 70, 110)),
                ));
                x += UnicodeWidthStr::width(styled.as_str()) as u16;
            }
        }

        // dirty 标记：工作区不干净时显示 *
        if !app.git_status.is_empty() {
            spans.push(Span::styled(
                " * ".to_string(),
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Rgb(140, 100, 20)),
            ));
            x += 3;
        }

        spans.push(Span::raw("  "));
        x += 2;
    }

    /// 全局工具栏每个按钮的底色
    const GLOBAL_BG_COLORS: [Color; 7] = [
        Color::Rgb(40, 80, 140),  // fetch — 蓝
        Color::Rgb(160, 80, 30),  // push — 橙
        Color::Rgb(40, 120, 60),  // pull — 绿
        Color::Rgb(100, 50, 130), // branches — 紫
        Color::Rgb(130, 110, 40), // tags — 金
        Color::Rgb(50, 110, 120), // stash — 青
        Color::Rgb(90, 90, 100),  // files — 灰
    ];

    let mut prev_group: Option<u8> = None;
    for (i, btn) in buttons.iter().enumerate() {
        // 不同组之间加空格分隔
        if prev_group.is_some_and(|pg| pg != btn.group) {
            spans.push(Span::raw(" "));
            x += 1;
        }
        prev_group = Some(btn.group);

        let text = format!(" {} ", btn.label);
        let text_width = UnicodeWidthStr::width(text.as_str()) as u16;
        app.global_toolbar_state.button_starts.push(x);
        let bg = GLOBAL_BG_COLORS[i % GLOBAL_BG_COLORS.len()];
        spans.push(Span::styled(
            text.clone(),
            Style::default().fg(Color::White).bg(bg),
        ));
        x += text_width;
    }

    // 右侧：CPU/MEM 读数（右对齐）
    let cpu_text = format!(" CPU {:.0}% ", app.cpu_usage);
    let mem_text = format!(" MEM {:.1}/{:.0}GB ", app.mem_used_gb, app.mem_total_gb);
    let cpu_width = UnicodeWidthStr::width(cpu_text.as_str()) as u16;
    let mem_width = UnicodeWidthStr::width(mem_text.as_str()) as u16;
    let right_width = cpu_width + mem_width;

    let total_content = (x - area.x) + right_width;
    if total_content < area.width {
        let padding = area.width - total_content;
        spans.push(Span::raw(" ".repeat(padding as usize)));
    }

    spans.push(Span::styled(
        cpu_text,
        Style::default().fg(Color::White).bg(Color::Rgb(60, 60, 80)),
    ));
    spans.push(Span::styled(
        mem_text,
        Style::default().fg(Color::White).bg(Color::Rgb(50, 80, 60)),
    ));

    let para = Paragraph::new(Line::from(spans));
    f.render_widget(para, area);
}
