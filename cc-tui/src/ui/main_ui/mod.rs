mod attachment;
pub(crate) mod bg_agent_bar;
pub(crate) mod message_area;
pub(crate) mod panels;
mod popups;
mod queued_messages;
mod status_bar;
pub(crate) use status_bar::{format_token_count, truncate_spans_to_width};
mod sticky_header;
mod viewport_layout;
#[cfg(any(test, feature = "headless"))]
pub use viewport_layout::prepare_width;
pub use viewport_layout::WidthPreparation;
pub use viewport_layout::{prepare_viewport, required_rows, try_prepare_width};

pub(crate) use message_area::highlight_line_spans;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Clear, Padding, Paragraph},
    Frame,
};
use tui_textarea::{CursorMove, TextArea};

use crate::{app::App, ui::theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextareaShellMode {
    None,
    Command,
    Stdin,
}

pub fn render(f: &mut Frame, app: &mut App) {
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.queued_message_actions.clear();
    ui.queued_messages_area = None;
    // Setup 向导：全屏覆盖，优先于所有正常界面
    if app.global_ui.setup_wizard.is_some() {
        popups::setup_wizard::render_setup_wizard(f, app);
        return;
    }

    let area = f.area();
    if app.global_ui.agent_center.is_some() {
        crate::app::agent_center::render::render(f, app);
        return;
    }
    render_session_column(f, app, area);
}

/// 渲染单个 session 列（含垂直布局拆分）
fn render_session_column(f: &mut Frame, app: &mut App, area: Rect) {
    f.render_widget(Clear, area);

    let terminal_height = app.global_ui.viewport.terminal_size.height;
    let terminal_area = Rect::new(
        0,
        0,
        area.width,
        if terminal_height == 0 {
            area.height
        } else {
            terminal_height
        },
    );
    let heights = viewport_layout::BottomHeights::measure(app, terminal_area);
    let input_height = heights.input;
    let queued_height = heights.queued;
    let attachment_height = heights.attachment;
    let panel_height = heights.panel;

    // Sticky header 高度（已注释禁用：鸡肋功能，高度固定为 0 不渲染）
    let sticky_header_height: u16 = 0;
    // let sticky_header_height: u16 = app
    //     .session_mgr
    //     .current()
    //     .metadata
    //     .last_human_message
    //     .as_ref()
    //     .map(|msg| {
    //         let width = area.width.saturating_sub(2).max(1);
    //         let lines = sticky_header::estimate_header_lines(msg, width);
    //         lines as u16
    //     })
    //     .unwrap_or(0);

    let status_bar_height = heights.status;

    let bg_bar_height_val = heights.background;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(sticky_header_height),
            Constraint::Min(1),
            Constraint::Length(attachment_height),
            Constraint::Length(panel_height),
            Constraint::Length(queued_height),
            Constraint::Length(input_height),
            Constraint::Length(status_bar_height),
            Constraint::Length(bg_bar_height_val),
        ])
        .split(area);

    message_area::render_messages(f, app, chunks[0], chunks[1]);

    // 复制成功浮动提示：消息区右下角，紧贴输入框上方（对齐 CC）
    if let Some(until) = app.session_mgr.current().ui.copy_message_until {
        if std::time::Instant::now() < until {
            let lc = &app.services.lc;
            let count = app.session_mgr.current().ui.copy_char_count;
            let text = format!(
                " {} ",
                lc.tr_args(
                    "statusbar-copied",
                    &[("count".into(), (count as i64).into())]
                )
            );
            let text_width = unicode_width::UnicodeWidthStr::width(text.as_str());
            let msg_area = chunks[1];
            if msg_area.height > 0 && text_width > 0 {
                // scrollbar 占据最右 1 列，toast 需左移避让
                let has_scrollbar = app
                    .session_mgr
                    .current()
                    .ui
                    .message_scrollbar_area
                    .is_some();
                let scrollbar_col: u16 = if has_scrollbar { 1 } else { 0 };
                let y = msg_area.y + msg_area.height - 1;
                let avail_width = msg_area.width.saturating_sub(scrollbar_col);
                let x = (msg_area.x + avail_width).saturating_sub(text_width as u16);
                let label_area = Rect {
                    x,
                    y,
                    width: text_width.min(avail_width as usize) as u16,
                    height: 1,
                };
                f.render_widget(
                    Paragraph::new(text).style(
                        Style::default().fg(theme::cur().claude_blue_shimmer_for_system_spinner),
                    ),
                    label_area,
                );
            }
        }
    }

    render_back_to_bottom_button(f, app, chunks[1], chunks[5], panel_height);

    attachment::render_attachment_bar(f, app, chunks[2]);

    // 底部展开区
    if panel_height > 0 {
        let panel_area = chunks[3];
        match &app.session_mgr.current_mut().agent.interaction_prompt {
            Some(crate::app::InteractionPrompt::Approval(_)) => {
                popups::hitl::render_hitl_popup(f, app, panel_area);
            }
            Some(crate::app::InteractionPrompt::Questions(_)) => {
                popups::ask_user::render_ask_user_popup(f, app, panel_area);
            }
            Some(crate::app::InteractionPrompt::Rewind(_)) => {
                popups::rewind::render_rewind_popup(f, app, panel_area);
            }
            None => {}
        }
        if app.global_ui.oauth_prompt.is_some() {
            popups::oauth::render_oauth_popup(f, app, panel_area);
        }
        // PanelManager 统一渲染分发：session 面板优先，global 面板次之
        if app
            .session_mgr
            .current_mut()
            .agent
            .interaction_prompt
            .is_none()
            && app.global_ui.oauth_prompt.is_none()
        {
            if app.session_mgr.current_mut().session_panels.is_any_open() {
                let mut state = app
                    .session_mgr
                    .current_mut()
                    .session_panels
                    .take_active()
                    .expect("is_any_open was true");
                state.render(f, app, panel_area);
                app.session_mgr
                    .current_mut()
                    .session_panels
                    .put_active(state);
            } else if app.global_panels.is_any_open() {
                let mut state = app
                    .global_panels
                    .take_active()
                    .expect("is_any_open was true");
                state.render(f, app, panel_area);
                app.global_panels.put_active(state);
            }
        }
    } else {
        let ui = &mut app.session_mgr.current_mut().ui;
        ui.panel_area = None;
        ui.panel_scrollbar_metrics = None;
        ui.panel_scrollbar_dragging = false;
        ui.panel_selection.clear();
    }

    queued_messages::render(f, app, chunks[4]);

    // 输入框样式：Bar 焦点变暗 / 聚焦只读模式 / 正常模式
    let focused_id = app.session_mgr.current_mut().focused_instance_id.clone();

    // 抽屉导航态（主会话）刻意压暗输入框：**真机定稿**，用边框降级表达「按键已经
    // 交给抽屉，此刻打字先进抽屉」。唯一的例外是子视口内——那时输入框的青色边框
    // 是「现在在子 Agent 窗口还是主窗口」的唯一标识（PRD §4.11），按 ↓ 只是把按键
    // 交给抽屉、视口没变，不能连方位感一起抹掉，故由 `focused_id.is_none()` 放行。
    let input_dimmed = focused_id.is_none() && {
        let session = app.session_mgr.current();
        let task_bar_focused =
            session.ui.background_tasks_bar_focused && app.has_running_background_shell_tasks();
        session.drawer_nav_active() || task_bar_focused
    };

    let popup_active = app.global_ui.oauth_prompt.is_some()
        || app
            .session_mgr
            .current_mut()
            .agent
            .interaction_prompt
            .is_some();
    let shell_mode = textarea_shell_mode(app);

    if input_dimmed || popup_active {
        // Bar 焦点模式：输入框变暗
        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::TOP | ratatui::widgets::Borders::BOTTOM)
            .border_style(ratatui::style::Style::default().fg(theme::cur().prompt_border))
            .padding(Padding::new(2, 0, 0, 0));
        app.session_mgr.current_mut().ui.textarea.set_block(block);
    } else if let Some(ref id) = focused_id {
        let sub = app.session_mgr.current().subagent_sessions.get(id);
        let title = format!(
            "[ {} ]",
            sub.map(|sub| sub.task_preview.as_str()).unwrap_or("agent")
        );
        let placeholder = match sub {
            Some(sub) if sub.status.is_active() => format!("Message @{}...", sub.agent_type),
            _ => "Task ended (read-only)".into(),
        };
        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::TOP | ratatui::widgets::Borders::BOTTOM)
            .border_style(Style::default().fg(theme::cur().background))
            .padding(Padding::new(2, 0, 0, 0))
            .title_top(ratatui::text::Line::from(title).right_aligned());
        let textarea = &mut app.session_mgr.current_mut().ui.textarea;
        textarea.set_block(block);
        textarea.set_placeholder_text(placeholder);
        textarea.set_style(Style::default().fg(theme::cur().text));
    } else {
        app.session_mgr
            .current_mut()
            .ui
            .textarea
            .set_placeholder_text("");
        // 正常模式：恢复与 build_textarea 一致的边框样式
        let border_color = textarea_shell_border_color(shell_mode);
        let mut block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::TOP | ratatui::widgets::Borders::BOTTOM)
            .border_style(ratatui::style::Style::default().fg(border_color))
            .padding(Padding::new(2, 0, 0, 0));
        if let Some(hint) = textarea_shell_hint(shell_mode) {
            block = block.title(Span::styled(
                hint,
                Style::default().fg(textarea_shell_hint_color(shell_mode)),
            ));
        }
        app.session_mgr.current_mut().ui.textarea.set_block(block);
        app.session_mgr
            .current_mut()
            .ui
            .textarea
            .set_style(ratatui::style::Style::default().fg(theme::cur().text));
    }

    // 输入框渲染
    let cursor_visible = app.session_mgr.current().ui.cursor_visible;
    let textarea_ref = &app.session_mgr.current_mut().ui.textarea;
    render_textarea(
        f,
        textarea_ref,
        chunks[5],
        shell_mode,
        app.focused,
        cursor_visible,
    );
    app.session_mgr.current_mut().ui.textarea_area = Some(chunks[5]);

    // 输入提示符
    let prompt_x = chunks[5].x;
    let prompt_y = chunks[5].y + 1;
    let prompt_area = Rect {
        x: prompt_x,
        y: prompt_y,
        width: 2,
        height: 1,
    };
    let loading = app.session_mgr.current_mut().ui.loading;
    let (prompt, prompt_style) = textarea_prompt(shell_mode, loading);
    if chunks[5].height >= 2 {
        f.render_widget(Paragraph::new(prompt).style(prompt_style), prompt_area);
    }

    // 统一命令/Skills 提示条 或 @ 提及弹窗（互斥）
    if app.session_mgr.current_mut().ui.at_mention.active {
        crate::app::at_mention::popup::render_at_mention_popup(
            f,
            &app.session_mgr.current_mut().ui.at_mention,
            chunks[5],
            &app.services.lc,
        );
    } else {
        popups::hints::render_unified_hint(f, app, chunks[5]);
    }

    // 状态栏
    status_bar::render_status_bar(f, app, chunks[6]);
    if bg_bar_height_val > 0 {
        bg_agent_bar::render_bg_agent_bar(f, app, chunks[7]);
        app.session_mgr.current_mut().ui.bg_bar_area = Some(chunks[7]);
    } else {
        app.session_mgr.current_mut().ui.bg_bar_area = None;
    }
}

fn textarea_shell_mode(app: &App) -> TextareaShellMode {
    let text = app.session_mgr.current().ui.textarea.lines().join("\n");
    textarea_shell_mode_from_text(&text, app.is_shell_command_running())
}

fn textarea_shell_mode_from_text(text: &str, is_shell_running: bool) -> TextareaShellMode {
    if is_shell_running {
        TextareaShellMode::Stdin
    } else if text.starts_with('!') {
        TextareaShellMode::Command
    } else {
        TextareaShellMode::None
    }
}

fn textarea_shell_hint(mode: TextareaShellMode) -> Option<&'static str> {
    match mode {
        TextareaShellMode::Command => Some("Enter shell command..."),
        TextareaShellMode::Stdin => Some("stdin"),
        TextareaShellMode::None => None,
    }
}

fn textarea_shell_border_color(mode: TextareaShellMode) -> Color {
    match mode {
        TextareaShellMode::Command => theme::cur().error,
        TextareaShellMode::Stdin | TextareaShellMode::None => theme::cur().inactive,
    }
}

fn textarea_shell_hint_color(mode: TextareaShellMode) -> Color {
    match mode {
        TextareaShellMode::Command => theme::cur().error,
        TextareaShellMode::Stdin | TextareaShellMode::None => theme::cur().inactive,
    }
}

fn textarea_prompt(mode: TextareaShellMode, _loading: bool) -> (&'static str, Style) {
    match mode {
        TextareaShellMode::Command => (
            "!",
            Style::default()
                .fg(theme::cur().error)
                .add_modifier(Modifier::BOLD),
        ),
        TextareaShellMode::Stdin | TextareaShellMode::None => {
            ("❯", Style::default().fg(theme::cur().inactive))
        }
    }
}

fn render_textarea(
    f: &mut Frame,
    textarea: &TextArea<'static>,
    area: Rect,
    shell_mode: TextareaShellMode,
    focused: bool,
    cursor_visible: bool,
) {
    f.render_widget(Clear, area);
    let mut display_textarea = textarea.clone();
    if shell_mode == TextareaShellMode::Command {
        hide_shell_prefix_for_display(&mut display_textarea);
    }
    // shell 命令态下 `/xxx` 是路径或参数，不做命令着色
    if shell_mode == TextareaShellMode::None {
        highlight_slash_commands(&mut display_textarea);
    }

    // textarea 自带块光标始终设为与文本同色（视觉不可见），由 draw_bar_cursor 画 │
    display_textarea.set_cursor_style(Style::default().fg(theme::cur().text));
    f.render_widget(&display_textarea, area);

    // 聚焦时画细线光标 │。Command 模式下用 display_textarea（! 前缀已移除、
    // 光标位置已调整），其他模式用原始 textarea
    if focused {
        let cursor_source = if shell_mode == TextareaShellMode::Command {
            &display_textarea
        } else {
            textarea
        };
        draw_bar_cursor(f.buffer_mut(), cursor_source, area, cursor_visible);
    }
}

/// 在 buffer 中绘制 │ 细线光标
/// 始终右移 cells（防止抖动），仅 cursor_visible 时画 │，否则留空格
fn draw_bar_cursor(
    buf: &mut Buffer,
    textarea: &TextArea<'static>,
    area: Rect,
    cursor_visible: bool,
) {
    let (data_row, data_col) = textarea.cursor();

    let inner_x = area.x + 2;
    let inner_y = area.y + 1;
    let inner_w = area.width.saturating_sub(2);
    let inner_h = area.height.saturating_sub(2);

    if inner_w == 0 || inner_h == 0 {
        return;
    }

    let line = textarea
        .lines()
        .get(data_row)
        .map(|s| s.as_str())
        .unwrap_or("");
    // data_col 是字符索引，需转换为显示列（累加 unicode-width）
    let display_col: usize = line
        .chars()
        .take(data_col)
        .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
        .sum();

    let cell_x = inner_x + display_col as u16;
    let cell_y = inner_y + data_row as u16;

    if cell_x >= inner_x + inner_w || cell_y >= inner_y + inner_h {
        return;
    }

    let at_end_of_line = data_col >= line.chars().count();

    if !at_end_of_line && display_col > 0 {
        // 光标在行内字符上（非最左）：右移后续文本，腾出 1 格给光标
        let line_display_width: usize = line
            .chars()
            .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
            .sum();
        let last_text_cell = (inner_x + line_display_width as u16).saturating_sub(1);
        let last_visible = inner_x + inner_w - 1;
        let shift_end = last_text_cell.min(last_visible.saturating_sub(1));

        if shift_end >= cell_x {
            for x in (cell_x..=shift_end).rev() {
                let (sym, sty) = buf
                    .cell((x, cell_y))
                    .map(|c| (c.symbol().to_string(), c.style()))
                    .unwrap_or_default();
                if let Some(dst) = buf.cell_mut((x + 1, cell_y)) {
                    dst.set_symbol(&sym);
                    dst.set_style(sty);
                }
            }
        }
        write_cursor_cell(buf, cell_x, cell_y, cursor_visible);
    } else if !at_end_of_line && display_col == 0 {
        // 光标在最左边：│ 画在 padding 区域，不右移不占文本位置
        write_cursor_cell(buf, inner_x.saturating_sub(1), cell_y, cursor_visible);
    } else {
        // 行尾：直接覆盖光标位空格
        write_cursor_cell(buf, cell_x, cell_y, cursor_visible);
    }
}

/// 在指定 cell 写入 │（亮相）或空格（灭相）
fn write_cursor_cell(buf: &mut Buffer, x: u16, y: u16, visible: bool) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        if visible {
            cell.set_symbol("│");
            cell.set_style(Style::default().fg(theme::cur().inactive));
        } else {
            cell.set_symbol(" ");
            cell.set_style(Style::default());
        }
    }
}

fn hide_shell_prefix_for_display(textarea: &mut TextArea<'static>) {
    if !textarea
        .lines()
        .first()
        .is_some_and(|line| line.starts_with('!'))
    {
        return;
    }
    let (cursor_row, cursor_col) = textarea.cursor();
    textarea.move_cursor(CursorMove::Jump(0, 1));
    textarea.delete_char();
    let display_col = if cursor_row == 0 {
        cursor_col.saturating_sub(1)
    } else {
        cursor_col
    };
    textarea.move_cursor(CursorMove::Jump(cursor_row as u16, display_col as u16));
}

/// `/command` token 的续接字符集（`/` 后首字符必须为字母，随后可含这些字符）
fn is_slash_command_tail(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_')
}

/// 扫描一行内所有 `/command` token 的**字节**区间。
///
/// 规则对齐 CC `findSlashCommandPositions`：`/` 前是行首或空白，`/` 后首字符为
/// ASCII 字母（因此 `/theme`、`/gougou-auth` 命中，`/2x`、`a/theme` 不命中）。
/// 注意该规则不区分「已知命令」与路径——行首的 `/usr/bin` 同样命中 `/usr`，与 CC 行为一致。
fn slash_command_spans(line: &str) -> Vec<(usize, usize)> {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (byte, ch) = chars[i];
        let at_word_start = i == 0 || chars[i - 1].1.is_whitespace();
        if ch != '/' || !at_word_start {
            i += 1;
            continue;
        }
        if !chars
            .get(i + 1)
            .is_some_and(|(_, c)| c.is_ascii_alphabetic())
        {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len() && is_slash_command_tail(chars[j].1) {
            j += 1;
        }
        spans.push((byte, chars.get(j).map_or(line.len(), |(b, _)| *b)));
        i = j;
    }
    spans
}

/// 将输入框内的 `/command` token 渲染为主题 `suggestion` 蓝色（对齐 CC 输入框）。
///
/// 入参为**显示态** textarea（`!` 前缀可能已被移除），自定义高亮只作用于克隆体，
/// 不污染真实输入状态；优先级 5 低于光标(30)/选区(10)，不会盖住它们。
fn highlight_slash_commands(textarea: &mut TextArea<'static>) {
    textarea.clear_custom_highlight();
    let style = Style::default().fg(theme::cur().suggestion);
    let ranges: Vec<((usize, usize), (usize, usize))> = textarea
        .lines()
        .iter()
        .enumerate()
        .flat_map(|(row, line)| {
            slash_command_spans(line)
                .into_iter()
                .map(move |(start, end)| ((row, start), (row, end)))
        })
        .collect();
    for (start, end) in ranges {
        textarea.custom_highlight((start, end), style, 5);
    }
}

/// 绘制"回到最新"浮层按钮：离开底部且内容溢出时，在**输入框上方白条**的横向
/// 范围内居中，覆盖在消息区最后一行。
///
/// 命中区域写入 `ui.scroll_to_bottom_area`，鼠标点击回到底部（`App::scroll_to_bottom`）。
/// 覆盖式浮层——会遮住该行约 1 行文本，与既有 copy toast 同策略。
fn render_back_to_bottom_button(
    f: &mut Frame,
    app: &mut App,
    msg_area: Rect,
    input_area: Rect,
    panel_height: u16,
) {
    // 基线刷新：跟随底部时持续对齐基线并清除未读；离开底部后内容增长即标记未读。
    // 对齐 codex unseen_activity 的两条信号：committed 消息数（view_messages.len）
    // + 内容行数（render_cache.total_lines），任一增长即视为有新活动。
    let (following, total_lines, msg_len) = {
        let s = app.session_mgr.current();
        (
            s.ui.scroll_follow,
            s.messages.render_cache.read().total_lines,
            s.messages.view_messages.len(),
        )
    };
    {
        let ui = &mut app.session_mgr.current_mut().ui;
        if following {
            ui.scroll_baseline_lines = total_lines;
            ui.scroll_baseline_len = msg_len;
            ui.scroll_to_bottom_unseen = false;
        } else if total_lines > ui.scroll_baseline_lines || msg_len > ui.scroll_baseline_len {
            ui.scroll_to_bottom_unseen = true;
        }
    }
    let (scrolled_away, hover, unseen) = {
        let s = app.session_mgr.current();
        (
            !s.ui.scroll_follow && s.ui.scrollbar_max_offset > 0,
            s.ui.scroll_to_bottom_hover,
            s.ui.scroll_to_bottom_unseen,
        )
    };
    // 面板/弹窗激活、消息区不可用时不显示，并清除上一帧登记的热区
    if !scrolled_away
        || panel_height > 0
        || app.is_interaction_popup_active()
        || msg_area.height == 0
        || msg_area.width == 0
    {
        app.session_mgr.current_mut().ui.scroll_to_bottom_area = None;
        return;
    }

    let key = if unseen {
        "scroll-back-to-bottom-new"
    } else {
        "scroll-back-to-bottom"
    };
    let text = format!(" {} ", app.services.lc.tr(key));
    let text_width = unicode_width::UnicodeWidthStr::width(text.as_str());
    // 居中基准 = 输入框（含其上边框白条）的横向范围。不再按"消息区宽度减去最右
    // 1 列滚动条"居中——那会让按钮整体左偏 1 列，与白条不齐；居中且较短的按钮
    // 右缘也永远够不到滚动条列。
    let avail = input_area.width;
    if text_width == 0 || text_width as u16 > avail {
        app.session_mgr.current_mut().ui.scroll_to_bottom_area = None;
        return;
    }

    let width = text_width as u16;
    let y = msg_area.y + msg_area.height - 1;
    let x = input_area.x + (avail - width) / 2;
    let area = Rect {
        x,
        y,
        width,
        height: 1,
    };
    let style = if hover {
        Style::default()
            .fg(theme::cur().claude)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::cur().claude)
    };
    f.render_widget(Paragraph::new(text).style(style), area);
    app.session_mgr.current_mut().ui.scroll_to_bottom_area = Some(area);
}

/// 计算底部展开区所需高度（无激活面板时返回 0）
fn active_panel_height(app: &App, screen_height: u16, screen_width: u16) -> u16 {
    // plugin 面板可以占 70%，AskUser 弹窗允许 75%（选项多/文字长需要更多空间），其他最多 60%
    let is_plugin_panel = app.global_panels.is_active(crate::app::PanelKind::Plugin);
    let has_ask_user = matches!(
        &app.session_mgr.current().agent.interaction_prompt,
        Some(crate::app::InteractionPrompt::Questions(_))
    );
    let max_h = if is_plugin_panel {
        screen_height * 70 / 100
    } else if has_ask_user {
        screen_height * 3 / 4
    } else {
        screen_height * 3 / 5
    };
    let raw = if let Some(h) = app
        .session_mgr
        .current()
        .session_panels
        .dispatch_desired_height(screen_height, screen_width)
    {
        h
    } else if let Some(h) = app
        .global_panels
        .dispatch_desired_height(screen_height, screen_width)
    {
        h
    } else if let Some(crate::app::InteractionPrompt::Approval(_)) =
        &app.session_mgr.current().agent.interaction_prompt
    {
        popups::hitl::hitl_popup_height(app, screen_width, max_h)
    } else if app.global_ui.oauth_prompt.is_some() {
        9 // 标题1 + 提示1 + URL1 + 空行1 + 输入框1 + 错误1 + 快捷键1 + 边框2
    } else if let Some(crate::app::InteractionPrompt::Questions(p)) =
        &app.session_mgr.current().agent.interaction_prompt
    {
        let cur = &p.questions[p.active_tab];
        // BorderedPanel 无左右边框，内容区宽度 = screen_width；滚动条占 1 列
        let panel_width = screen_width.saturating_sub(1) as usize;
        popups::ask_user_height::ask_user_content_height(&cur.data, panel_width).max(8)
    } else if let Some(crate::app::InteractionPrompt::Rewind(p)) =
        &app.session_mgr.current().agent.interaction_prompt
    {
        let base = p.items.len() as u16 + 3;
        let confirm_extra = if p.mode == crate::app::RewindMode::ConfirmRevert {
            let selected = &p.items[p.cursor];
            (selected.file_changes.len() as u16 + 3).min(10)
        } else {
            0
        };
        (base + confirm_extra).max(5)
    } else {
        0
    };
    raw.min(max_h)
}

#[cfg(test)]
#[path = "main_ui_test.rs"]
mod main_ui_test;
