//! 后台任务面板（Phase 3）：List/Detail 双视图，显示 Ctrl+B 后台化的 shell 任务。
//!
//! 对齐效果图场景 3（列表）/ 场景 4（详情）。
//! 分组：Shells（background_shells）+ Local agents（background_agents），
//! 其余分组（Remote agents / Monitors / Workflows / Dreams）预留。

use std::any::Any;
use std::time::{Duration, Instant};

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;
use tui_textarea::Input;
use tokio::sync::mpsc;

use super::panel_component::PanelComponent;
use super::panel_manager::{EventResult, PanelContext, PanelKind};
use super::ShellStatus;
use crate::ui::theme;

/// Detail 视图读取 output 末尾的字节数（对齐 PRD §5）
const SHELL_DETAIL_TAIL_BYTES: u64 = 8192;
/// Detail 视图 output 缓存刷新间隔（避免每帧读磁盘）
const OUTPUT_REFRESH_INTERVAL: Duration = Duration::from_millis(1000);
/// Detail 视图 output 框显示的最大行数
const DETAIL_OUTPUT_MAX_LINES: usize = 10;

/// 后台任务面板：List/Detail 双视图。
pub struct BackgroundTasksPanel {
    pub view: BackgroundTaskView,
    pub selected_index: usize,
    /// Detail 视图 output 缓存（避免每帧读磁盘）
    pub output_cache: String,
    pub output_cache_id: Option<String>,
    pub output_refresh_at: Option<Instant>,
    /// 后台 read_tail task 的输出 channel（render try_recv 非阻塞读取）
    pub output_rx: Option<mpsc::Receiver<String>>,
    /// 后台 read_tail task handle（切换 task / 关闭时 abort）
    pub output_task: Option<tokio::task::JoinHandle<()>>,
}

/// 面板视图状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundTaskView {
    /// 列表视图
    List,
    /// 详情视图（item_id = background_shell.id）
    Detail { item_id: String },
}

impl BackgroundTasksPanel {
    pub fn new() -> Self {
        Self {
            view: BackgroundTaskView::List,
            selected_index: 0,
            output_cache: String::new(),
            output_cache_id: None,
            output_refresh_at: None,
            output_rx: None,
            output_task: None,
        }
    }

    /// 当前选中/查看的任务 id（List 用 selected_index，Detail 用 item_id）。
    fn current_item_id(&self, ctx: &PanelContext<'_>) -> Option<String> {
        match &self.view {
            BackgroundTaskView::List => ctx
                .session_mgr
                .current()
                .background_shells
                .get(self.selected_index)
                .map(|b| b.id.clone()),
            BackgroundTaskView::Detail { item_id } => Some(item_id.clone()),
        }
    }
}

impl Default for BackgroundTasksPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for BackgroundTasksPanel {
    fn drop(&mut self) {
        // 面板关闭/被替换时 abort 后台 output_task，避免 JoinHandle::drop 的 detach 语义
        // 导致 read_tail 循环永不退出（tx 持有，send 不返回 Err）而泄漏
        if let Some(t) = self.output_task.take() {
            t.abort();
        }
    }
}

impl PanelComponent for BackgroundTasksPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::BackgroundTasks
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;
        let count = ctx.session_mgr.current().background_shells.len();
        match input {
            Input { key: Key::Up, .. } if self.view == BackgroundTaskView::List => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } if self.view == BackgroundTaskView::List => {
                if self.selected_index + 1 < count {
                    self.selected_index += 1;
                }
                EventResult::Consumed
            }
            Input { key: Key::Enter, .. } if self.view == BackgroundTaskView::List => {
                let id = ctx
                    .session_mgr
                    .current()
                    .background_shells
                    .get(self.selected_index)
                    .map(|b| b.id.clone());
                if let Some(id) = id {
                    self.view = BackgroundTaskView::Detail { item_id: id };
                }
                EventResult::Consumed
            }
            Input { key: Key::Char('x'), .. } => {
                if let Some(id) = self.current_item_id(ctx) {
                    kill_background_shell(ctx, &id);
                }
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } | Input { key: Key::Left, .. } => {
                if self.view == BackgroundTaskView::List {
                    EventResult::ClosePanel
                } else {
                    // Detail→List：abort 后台 output task，释放资源
                    if let Some(t) = self.output_task.take() {
                        t.abort();
                    }
                    self.output_rx = None;
                    self.output_cache_id = None;
                    self.view = BackgroundTaskView::List;
                    EventResult::Consumed
                }
            }
            _ => EventResult::Consumed,
        }
    }

    fn desired_height(&self, screen_height: u16, _screen_width: u16) -> u16 {
        match self.view {
            BackgroundTaskView::List => (screen_height * 50 / 100).max(10),
            BackgroundTaskView::Detail { .. } => (screen_height * 65 / 100).max(20),
        }
    }

    fn render(&mut self, f: &mut Frame, app: &mut super::App, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme::BORDER))
            .title(Span::styled(
                " Background tasks ",
                Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        f.render_widget(block, area);

        // clone view 释放 self 借用，再 &mut self 传给子渲染函数
        let view = self.view.clone();
        match &view {
            BackgroundTaskView::List => render_list(f, self, app, inner),
            BackgroundTaskView::Detail { item_id } => render_detail(f, self, app, item_id, inner),
        }
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self, _lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        match self.view {
            BackgroundTaskView::List => vec![
                ("↑↓".to_string(), "选择".to_string()),
                ("Enter".to_string(), "详情".to_string()),
                ("x".to_string(), "停止".to_string()),
                ("Esc".to_string(), "关闭".to_string()),
            ],
            BackgroundTaskView::Detail { .. } => vec![
                ("←".to_string(), "返回".to_string()),
                ("x".to_string(), "停止".to_string()),
                ("Esc".to_string(), "关闭".to_string()),
            ],
        }
    }
}

/// 渲染用的 shell 行数据。
struct ShellRow {
    command: String,
    status: ShellStatus,
    elapsed: Duration,
}

/// 渲染用的 agent 行数据。
struct AgentRow {
    label: String,
    elapsed: Duration,
}

fn render_list(f: &mut Frame, panel: &mut BackgroundTasksPanel, app: &mut super::App, area: Rect) {
    // 一次性收集数据，释放 app 借用
    let (shells, agents): (Vec<ShellRow>, Vec<AgentRow>) = {
        let session = app.session_mgr.current();
        let shells = session
            .background_shells
            .iter()
            .map(|b| ShellRow {
                command: b.command.clone(),
                status: b.status,
                elapsed: b.elapsed(),
            })
            .collect();
        let agents = session
            .background_agents
            .iter()
            .map(|a| AgentRow {
                label: format!("{} \"{}\"", a.agent_name, a.instance_id),
                elapsed: a.started_at.elapsed(),
            })
            .collect();
        (shells, agents)
    };

    // 钳位 selected_index（cleanup 删除任务后避免选中越界/错位）
    let total = shells.len() + agents.len();
    if total > 0 && panel.selected_index >= total {
        panel.selected_index = total - 1;
    }

    let mut lines: Vec<Line> = Vec::new();
    // 副标题：汇总（对齐效果图场景 3 "1 active shell · 1 completed agent · ..."）
    let active_shell = shells
        .iter()
        .filter(|r| r.status == ShellStatus::Running)
        .count();
    let completed_shell = shells
        .iter()
        .filter(|r| r.status == ShellStatus::Completed)
        .count();
    let mut summary_parts: Vec<String> = Vec::new();
    if active_shell > 0 {
        summary_parts.push(format!("{} active shell", active_shell));
    }
    if completed_shell > 0 {
        summary_parts.push(format!("{} completed shell", completed_shell));
    }
    if !agents.is_empty() {
        summary_parts.push(format!("{} running agent", agents.len()));
    }
    let summary = if summary_parts.is_empty() {
        "no background tasks".to_string()
    } else {
        summary_parts.join(" · ")
    };
    lines.push(Line::from(Span::styled(
        summary,
        Style::default().fg(theme::MUTED),
    )));
    lines.push(Line::from(""));

    if shells.is_empty() && agents.is_empty() {
        lines.push(Line::from(Span::styled(
            "暂无后台任务（前台命令运行时 Ctrl+B 可转入后台）".to_string(),
            Style::default().fg(theme::MUTED),
        )));
    } else {
        // Shells 分组
        if !shells.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("SHELLS ({})", shells.len()),
                Style::default()
                    .fg(theme::MUTED)
                    .add_modifier(Modifier::BOLD),
            )));
            for (i, row) in shells.iter().enumerate() {
                lines.push(render_task_row(
                    i,
                    panel.selected_index,
                    row.command.clone(),
                    row.status,
                    row.elapsed,
                ));
            }
            if !agents.is_empty() {
                lines.push(Line::from(""));
            }
        }
        // Local agents 分组（background_agents，对齐效果图场景 3 "Local agents"）
        if !agents.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("LOCAL AGENTS ({})", agents.len()),
                Style::default()
                    .fg(theme::MUTED)
                    .add_modifier(Modifier::BOLD),
            )));
            for (i, row) in agents.iter().enumerate() {
                // agent 选中索引延续 shells 编号
                lines.push(render_task_row(
                    shells.len() + i,
                    panel.selected_index,
                    row.label.clone(),
                    ShellStatus::Running,
                    row.elapsed,
                ));
            }
        }
    }

    f.render_widget(Paragraph::new(lines), area);
}

/// 渲染单个任务行（marker + label + badge + elapsed，选中项整行高亮）。
fn render_task_row(
    idx: usize,
    selected_index: usize,
    label: String,
    status: ShellStatus,
    elapsed: Duration,
) -> Line<'static> {
    let selected = idx == selected_index;
    let marker = if selected { "▸ " } else { "  " };
    let badge_text = format!(" {} ", status.badge());
    let mut spans = vec![
        Span::raw(marker.to_string()),
        Span::styled(label, Style::default().fg(theme::SELECTED_FG)),
        Span::raw(" "),
        Span::styled(badge_text, status_badge_style(status)),
        Span::raw(" "),
        Span::styled(
            format_elapsed(elapsed),
            Style::default().fg(theme::MUTED),
        ),
    ];
    if selected {
        for s in &mut spans {
            s.style = s.style.bg(theme::SELECTION_BG);
        }
    }
    Line::from(spans)
}

fn render_detail(
    f: &mut Frame,
    panel: &mut BackgroundTasksPanel,
    app: &mut super::App,
    item_id: &str,
    area: Rect,
) {
    let bg_info = app
        .session_mgr
        .current()
        .background_shells
        .iter()
        .find(|b| b.id == item_id)
        .map(|b| (b.status, b.command.clone(), b.elapsed(), b.output_path.clone()));
    let Some((status, command, elapsed, output_path)) = bg_info else {
        // 任务不存在（可能被 cleanup 淘汰）：abort 后台 output task + 切回 List，避免卡在"任务不存在"
        if let Some(t) = panel.output_task.take() {
            t.abort();
        }
        panel.output_rx = None;
        panel.output_cache_id = None;
        panel.view = BackgroundTaskView::List;
        f.render_widget(Paragraph::new("任务不存在").alignment(Alignment::Center), area);
        return;
    };

    // 切换 task：abort 旧后台 task + spawn 新 task（后台 read_tail，避免 render 阻塞）
    if panel.output_cache_id.as_deref() != Some(item_id) {
        if let Some(t) = panel.output_task.take() {
            t.abort();
        }
        panel.output_cache.clear();
        panel.output_cache_id = Some(item_id.to_string());
        panel.output_refresh_at = None;
        let (tx, rx) = mpsc::channel::<String>(4);
        panel.output_rx = Some(rx);
        let path = output_path.clone();
        panel.output_task = Some(tokio::spawn(async move {
            // 每 OUTPUT_REFRESH_INTERVAL 读 tail 推送（tokio::interval 首个 tick 立即，首次读不延迟）
            let mut interval = tokio::time::interval(OUTPUT_REFRESH_INTERVAL);
            loop {
                interval.tick().await;
                let tail = match peri_agent::task_output::DiskOutput::read_tail(
                    &path,
                    SHELL_DETAIL_TAIL_BYTES,
                )
                .await
                {
                    Ok(b) => String::from_utf8_lossy(&b).to_string(),
                    Err(_) => String::new(),
                };
                if tx.send(tail).await.is_err() {
                    break; // panel drop rx（关闭/切换），退出
                }
            }
        }));
    }
    // try_recv 后台推送（非阻塞，render 不卡帧）
    if let Some(rx) = panel.output_rx.as_mut() {
        while let Ok(tail) = rx.try_recv() {
            panel.output_cache = tail;
            panel.output_refresh_at = Some(Instant::now());
        }
    }

    // 布局：上半信息行 + 下半 Output 框
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(8)])
        .split(area);

    let info_lines = vec![
        Line::from(vec![
            Span::styled("Status:  ", Style::default().fg(theme::MUTED)),
            Span::styled(format!(" {} ", status.badge()), status_badge_style(status)),
        ]),
        Line::from(vec![
            Span::styled("Runtime: ", Style::default().fg(theme::MUTED)),
            Span::raw(format_elapsed(elapsed)),
        ]),
        Line::from(vec![
            Span::styled("Command: ", Style::default().fg(theme::MUTED)),
            Span::styled(command, Style::default().fg(theme::SELECTED_FG)),
        ]),
    ];
    f.render_widget(Paragraph::new(info_lines), chunks[0]);

    // Output 框（圆角边框，对齐效果图场景 4）
    let output_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled("Output", Style::default().fg(theme::MUTED)));
    let output_inner = output_block.inner(chunks[1]);
    f.render_widget(output_block, chunks[1]);

    // 显示末尾 N 行 + "Showing N lines of X.X KB"
    let total_kb = panel.output_cache.len() as f64 / 1024.0;
    let all_lines: Vec<&str> = panel.output_cache.lines().collect();
    let start = all_lines.len().saturating_sub(DETAIL_OUTPUT_MAX_LINES);
    let showing_lines = &all_lines[start..];
    let mut output_lines: Vec<Line> =
        showing_lines.iter().map(|l| Line::from(l.to_string())).collect();
    output_lines.push(Line::from(""));
    output_lines.push(Line::from(Span::styled(
        format!("Showing {} lines of {:.1} KB", showing_lines.len(), total_kb),
        Style::default().fg(theme::MUTED),
    )));
    f.render_widget(Paragraph::new(output_lines), output_inner);
}

/// badge 样式：fg + 半透明背景色块（近似效果图 rgba 0.15，用深色 Rgb 近似）。
fn status_badge_style(status: ShellStatus) -> Style {
    match status {
        ShellStatus::Running => Style::default().fg(theme::CYAN).bg(Color::Rgb(28, 38, 68)),
        ShellStatus::Completed => Style::default().fg(theme::SAGE).bg(Color::Rgb(28, 48, 32)),
        ShellStatus::Failed => Style::default().fg(theme::ERROR).bg(Color::Rgb(58, 30, 38)),
        ShellStatus::Killed => Style::default().fg(theme::WARNING).bg(Color::Rgb(52, 46, 26)),
    }
}

fn format_elapsed(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 60 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{}s", secs)
    }
}

/// kill 指定后台 shell 任务：abort 进程 + watchdog，清理 result_rx，标记 Killed。
fn kill_background_shell(ctx: &mut PanelContext<'_>, id: &str) {
    let session = ctx.session_mgr.current_mut();
    if let Some(bg) = session.background_shells.iter_mut().find(|b| b.id == id) {
        if let Some(handle) = bg.abort_handle.take() {
            handle.abort();
        }
        if let Some(watchdog) = bg.stall_watchdog.take() {
            watchdog.abort();
        }
        bg.result_rx.take();
        bg.mark_ended(ShellStatus::Killed, Some(-1));
    }
}

impl super::App {
    /// 打开后台任务面板（↓ 键 / pill 入口，对齐效果图"↓ to view"）。
    pub fn open_background_tasks_panel(&mut self) {
        self.open_panel(super::panel_manager::PanelState::BackgroundTasks(
            BackgroundTasksPanel::new(),
        ));
    }
}
