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
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;
use tokio::sync::mpsc;
use tui_textarea::Input;
use unicode_width::UnicodeWidthStr;

use super::panel_component::PanelComponent;
use super::panel_manager::{EventResult, PanelContext, PanelKind};
use super::ShellStatus;
use crate::ui::theme;

/// Detail 视图读取 output 末尾的字节数（对齐 PRD §5）
const SHELL_DETAIL_TAIL_BYTES: u64 = 8192;
/// Detail 视图 output 缓存刷新间隔（避免每帧读磁盘）
const OUTPUT_REFRESH_INTERVAL: Duration = Duration::from_millis(1000);

/// 后台任务面板：List/Detail 双视图。
pub struct BackgroundTasksPanel {
    pub view: BackgroundTaskView,
    selected_item: Option<BackgroundTaskItem>,
    /// 最近显示的任务顺序仅用于导航；执行操作始终按稳定 ID 查找。
    visible_items: Vec<BackgroundTaskItem>,
    /// Detail 视图 output 缓存（避免每帧读磁盘）
    pub output_cache: String,
    pub output_cache_id: Option<String>,
    pub output_refresh_at: Option<Instant>,
    /// 后台 read_tail task 的输出 channel（render try_recv 非阻塞读取）
    pub output_rx: Option<mpsc::Receiver<String>>,
    /// 后台 read_tail task handle（切换 task / 关闭时 abort）
    pub output_task: Option<tokio::task::JoinHandle<()>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BackgroundTaskItem {
    Shell(String),
    Agent(String),
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
            selected_item: None,
            visible_items: Vec::new(),
            output_cache: String::new(),
            output_cache_id: None,
            output_refresh_at: None,
            output_rx: None,
            output_task: None,
        }
    }

    pub fn select_shell(&mut self, task_id: String) {
        self.selected_item = Some(BackgroundTaskItem::Shell(task_id));
    }

    fn refresh_items(&mut self, items: Vec<BackgroundTaskItem>) {
        // 仅首次选择默认项。选中目标消失后保持原 ID，禁止悄悄改选相邻任务。
        if self.selected_item.is_none() {
            self.selected_item = items.first().cloned();
        }
        self.visible_items = items;
    }

    fn selected_index(&self) -> Option<usize> {
        self.visible_items
            .iter()
            .position(|item| Some(item) == self.selected_item.as_ref())
    }

    fn move_selection(&mut self, down: bool) {
        if self.visible_items.is_empty() {
            return;
        }
        let index = match (self.selected_index(), down) {
            (Some(index), true) => (index + 1).min(self.visible_items.len() - 1),
            (Some(index), false) => index.saturating_sub(1),
            (None, true) => 0,
            (None, false) => self.visible_items.len() - 1,
        };
        self.selected_item = self.visible_items.get(index).cloned();
    }

    /// 取稳定选中 ID，并检查它仍存在；列表变化不得让操作落到另一条任务。
    fn current_item_id(&self, ctx: &PanelContext<'_>) -> Option<String> {
        match &self.view {
            BackgroundTaskView::List => {
                let BackgroundTaskItem::Shell(id) = self.selected_item.as_ref()? else {
                    return None;
                };
                let session = ctx.session_mgr.current();
                let exists = session.background_shells.iter().any(|bg| bg.id == *id)
                    || session.agent_shells.iter().any(|slot| {
                        slot.task_id == *id
                            && (slot.is_backgrounded() || slot.is_foreground_running())
                    });
                exists.then(|| id.clone())
            }
            BackgroundTaskView::Detail { item_id } => Some(item_id.clone()),
        }
    }

    fn stop_output_reader(&mut self) {
        if let Some(t) = self.output_task.take() {
            t.abort();
        }
        self.output_rx = None;
        self.output_cache_id = None;
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
        match input {
            Input {
                key: Key::Char('b'),
                ctrl: true,
                ..
            } => {
                if let Some(id) = self.current_item_id(ctx) {
                    let bg_event_tx = ctx.services.bg_event_tx.clone();
                    if let Some(slot) = ctx
                        .session_mgr
                        .current_mut()
                        .agent_shells
                        .iter_mut()
                        .find(|slot| slot.task_id == id)
                    {
                        super::shell_command::background_agent_slot(slot, bg_event_tx);
                    }
                }
                EventResult::Consumed
            }
            Input { key: Key::Up, .. } if self.view == BackgroundTaskView::List => {
                self.move_selection(false);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } if self.view == BackgroundTaskView::List => {
                self.move_selection(true);
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } if self.view == BackgroundTaskView::List => {
                if let Some(id) = self.current_item_id(ctx) {
                    self.view = BackgroundTaskView::Detail { item_id: id };
                }
                EventResult::Consumed
            }
            Input {
                key: Key::Char('x'),
                ..
            } => {
                if let Some(id) = self.current_item_id(ctx) {
                    kill_background_shell(ctx, &id);
                }
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } => {
                self.stop_output_reader();
                EventResult::ClosePanel
            }
            Input { key: Key::Left, .. } => {
                if self.view == BackgroundTaskView::List {
                    EventResult::ClosePanel
                } else {
                    self.stop_output_reader();
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
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
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

    fn status_bar_hints(&self, lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        match self.view {
            BackgroundTaskView::List => vec![
                ("↑↓".to_string(), lc.tr("key-move")),
                ("Enter".to_string(), lc.tr("key-detail")),
                ("Ctrl+B".to_string(), lc.tr("key-bg-command")),
                ("x".to_string(), lc.tr("key-kill")),
                ("Esc".to_string(), lc.tr("key-close")),
            ],
            BackgroundTaskView::Detail { .. } => vec![
                ("←".to_string(), lc.tr("key-back")),
                ("Ctrl+B".to_string(), lc.tr("key-bg-command")),
                ("x".to_string(), lc.tr("key-kill")),
                ("Esc".to_string(), lc.tr("key-close")),
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

fn task_items(session: &super::ChatSession) -> Vec<BackgroundTaskItem> {
    session
        .background_shells
        .iter()
        .map(|shell| BackgroundTaskItem::Shell(shell.id.clone()))
        .chain(
            session
                .agent_shells
                .iter()
                .filter(|slot| slot.is_backgrounded() || slot.is_foreground_running())
                .map(|slot| BackgroundTaskItem::Shell(slot.task_id.clone())),
        )
        .chain(
            session
                .background_agents
                .iter()
                .map(|agent| BackgroundTaskItem::Agent(agent.instance_id.clone())),
        )
        .collect()
}

fn render_list(f: &mut Frame, panel: &mut BackgroundTasksPanel, app: &mut super::App, area: Rect) {
    let lc = &app.services.lc;
    // 一次性收集数据，释放 app 借用
    let (shells, agents): (Vec<ShellRow>, Vec<AgentRow>) = {
        let session = app.session_mgr.current();
        panel.refresh_items(task_items(session));
        let shells = session
            .background_shells
            .iter()
            .map(|b| ShellRow {
                command: b.command.clone(),
                status: b.status,
                elapsed: b.elapsed(),
            })
            .chain(
                session
                    .agent_shells
                    .iter()
                    .filter(|slot| slot.is_backgrounded() || slot.is_foreground_running())
                    .map(|slot| ShellRow {
                        command: if slot.is_foreground_running() {
                            lc.tr_args(
                                "app-bg-foreground-tag",
                                &[("command".to_string(), slot.command.clone().into())],
                            )
                        } else {
                            slot.command.clone()
                        },
                        status: agent_shell_status(slot),
                        elapsed: slot.elapsed(),
                    }),
            )
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

    let selected_index = panel.selected_index();

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
            lc.tr("app-bg-empty"),
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
                    selected_index,
                    row.command.clone(),
                    row.status,
                    row.elapsed,
                    area.width as usize,
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
                    selected_index,
                    row.label.clone(),
                    ShellStatus::Running,
                    row.elapsed,
                    area.width as usize,
                ));
            }
        }
    }

    f.render_widget(Paragraph::new(lines), area);
}

/// 渲染单个任务行（marker + label + badge + elapsed，选中项整行高亮）。
///
/// `avail_width` 为列表可用列宽：命令 label 按「预留 badge + 耗时后」的剩余宽度
/// 单行截断，保证状态徽标与耗时始终可见（长命令不再挤掉它们）。
/// 完整命令请在详情视图查看（Enter 进入，那里折行展开）。
fn render_task_row(
    idx: usize,
    selected_index: Option<usize>,
    label: String,
    status: ShellStatus,
    elapsed: Duration,
    avail_width: usize,
) -> Line<'static> {
    let selected = Some(idx) == selected_index;
    let marker = if selected { "▸ " } else { "  " };
    let badge_text = format!(" {} ", status.badge());
    let elapsed_text = format_elapsed(elapsed);

    // 预留：marker + 空格 + badge + 空格 + elapsed
    let suffix_width = UnicodeWidthStr::width(marker)
        + 1
        + UnicodeWidthStr::width(badge_text.as_str())
        + 1
        + UnicodeWidthStr::width(elapsed_text.as_str());
    let label_budget = avail_width.saturating_sub(suffix_width);
    let label = super::super::ui::message_render::truncate_to_display_width(&label, label_budget);

    let mut spans = vec![
        Span::raw(marker.to_string()),
        Span::styled(label, Style::default().fg(theme::SELECTED_FG)),
        Span::raw(" "),
        Span::styled(badge_text, status_badge_style(status)),
        Span::raw(" "),
        Span::styled(elapsed_text, Style::default().fg(theme::MUTED)),
    ];
    if selected {
        for s in &mut spans {
            s.style = s.style.bg(theme::SELECTION_BG);
        }
    }
    Line::from(spans)
}

fn agent_shell_status(slot: &super::AgentShellSlot) -> ShellStatus {
    if !slot.ended {
        ShellStatus::Running
    } else {
        match slot.exit_signal.outcome() {
            Some(cc_agent::shell::ShellOutcome::Exited(0)) => ShellStatus::Completed,
            Some(cc_agent::shell::ShellOutcome::Cancelled) => ShellStatus::Killed,
            _ => ShellStatus::Failed,
        }
    }
}

fn render_detail(
    f: &mut Frame,
    panel: &mut BackgroundTasksPanel,
    app: &mut super::App,
    item_id: &str,
    area: Rect,
) {
    let bg_info = {
        let session = app.session_mgr.current();
        session
            .background_shells
            .iter()
            .find(|b| b.id == item_id)
            .map(|b| {
                (
                    b.status,
                    b.command.clone(),
                    b.elapsed(),
                    b.output_path.clone(),
                )
            })
            .or_else(|| {
                session
                    .agent_shells
                    .iter()
                    .find(|slot| slot.task_id == item_id)
                    .map(|slot| {
                        (
                            agent_shell_status(slot),
                            slot.command.clone(),
                            slot.elapsed(),
                            slot.output_path.clone(),
                        )
                    })
            })
    };
    let Some((status, command, elapsed, output_path)) = bg_info else {
        // 任务不存在（可能被 cleanup 淘汰）：abort 后台 output task + 切回 List，避免卡在"任务不存在"
        if let Some(t) = panel.output_task.take() {
            t.abort();
        }
        panel.output_rx = None;
        panel.output_cache_id = None;
        panel.view = BackgroundTaskView::List;
        f.render_widget(
            Paragraph::new(app.services.lc.tr("app-bg-task-missing")).alignment(Alignment::Center),
            area,
        );
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
                let tail = match cc_agent::task_output::DiskOutput::read_tail(
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

    // 布局：上半信息行（高度按命令折行行数自适应）+ 下半 Output 框
    //
    // 信息区含 3 行固定字段（Status / Runtime / Command）+ 命令折行后的续行。
    // 命令可能很长（如带循环的 shell 脚本），故不能再用固定 Length(4)，
    // 否则折行内容会被裁掉 —— 这里按 `command` 的展示宽度预先算出所需行数。
    let info_inner_width = area.width.saturating_sub(2).max(1) as usize; // 减去边框
    let command_prefix_width = UnicodeWidthStr::width("Command: ");
    let command_width = UnicodeWidthStr::width(command.as_str());
    let command_rows = if info_inner_width > command_prefix_width {
        let usable = info_inner_width - command_prefix_width;
        command_width.div_ceil(usable.max(1)).max(1)
    } else {
        1
    };
    let max_info_height = (area.height as usize).saturating_sub(8).max(1);
    let info_height = (2 + command_rows).min(max_info_height).max(3) as u16;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(info_height), Constraint::Min(8)])
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
    // 启用折行：长命令在详情页完整展开（列表页则截断，见 render_task_row）
    f.render_widget(
        Paragraph::new(info_lines).wrap(Wrap { trim: false }),
        chunks[0],
    );

    // Output 框（圆角边框，对齐效果图场景 4）
    let output_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled("Output", Style::default().fg(theme::MUTED)));
    let output_inner = output_block.inner(chunks[1]);
    f.render_widget(output_block, chunks[1]);

    // 显示末尾 N 行 + "Showing N lines of X.X KB"（N 根据实际可用高度动态计算）
    let total_kb = panel.output_cache.len() as f64 / 1024.0;
    let all_lines: Vec<&str> = panel.output_cache.lines().collect();
    // footer 占 2 行（空行 + 状态行），剩余空间全部用于显示内容
    let max_content_lines = output_inner.height.saturating_sub(2) as usize;
    let start = all_lines.len().saturating_sub(max_content_lines);
    let showing_lines = &all_lines[start..];
    let mut output_lines: Vec<Line> = showing_lines
        .iter()
        .map(|l| Line::from(l.to_string()))
        .collect();
    output_lines.push(Line::from(""));
    output_lines.push(Line::from(Span::styled(
        format!(
            "Showing {} lines of {:.1} KB",
            showing_lines.len(),
            total_kb
        ),
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
        ShellStatus::Killed => Style::default()
            .fg(theme::WARNING)
            .bg(Color::Rgb(52, 46, 26)),
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
        return;
    }
    if let Some(slot) = session
        .agent_shells
        .iter_mut()
        .find(|slot| slot.task_id == id)
    {
        slot.kill.abort();
        if let Some(watchdog) = slot.stall_watchdog.take() {
            watchdog.abort();
        }
        // 等执行器报告真实取消结果后由 poll_agent_shells 收口并通知 Agent。
    }
}

impl super::App {
    /// 打开后台任务面板（status bar 入口 Enter / Ctrl+B 入口）。
    pub fn open_background_tasks_panel(&mut self) {
        self.session_mgr
            .current_mut()
            .ui
            .background_tasks_bar_focused = false;
        let mut panel = BackgroundTasksPanel::new();
        panel.refresh_items(task_items(self.session_mgr.current()));
        self.open_panel(super::panel_manager::PanelState::BackgroundTasks(panel));
    }
}

#[cfg(test)]
#[path = "background_tasks_panel_test.rs"]
mod tests;
