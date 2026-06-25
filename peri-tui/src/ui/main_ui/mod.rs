mod attachment;
pub(crate) mod bg_agent_bar;
pub(crate) mod message_area;
pub(crate) mod panels;
mod popups;
mod status_bar;
mod sticky_header;

pub(crate) use message_area::highlight_line_spans;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Padding, Paragraph},
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
    // Setup 向导：全屏覆盖，优先于所有正常界面
    if app.global_ui.setup_wizard.is_some() {
        popups::setup_wizard::render_setup_wizard(f, app);
        return;
    }

    let area = f.area();
    render_session_column(f, app, area);
}

/// 渲染单个 session 列（含垂直布局拆分）
fn render_session_column(f: &mut Frame, app: &mut App, area: Rect) {
    // 动态输入框高度
    let line_count = app.session_mgr.current_mut().ui.textarea.lines().len() as u16;
    let input_height = (line_count + 2).min(area.height * 2 / 5).max(3);

    // 缓冲消息高度（loading 时在输入框上方显示待发送消息）
    let pending_count = app
        .session_mgr
        .current_mut()
        .messages
        .pending_messages
        .len();
    let queued_height: u16 = if pending_count > 0 && app.session_mgr.current_mut().ui.loading {
        (pending_count as u16).min(3)
    } else {
        0
    };

    // 附件栏高度
    let attachment_height: u16 = if app
        .session_mgr
        .current_mut()
        .metadata
        .pending_attachments
        .is_empty()
    {
        0
    } else {
        3
    };

    // 底部展开区高度
    let panel_height = active_panel_height(app, area.height, area.width);

    // Sticky header 高度
    let sticky_header_height: u16 = app
        .session_mgr
        .current()
        .metadata
        .last_human_message
        .as_ref()
        .map(|msg| {
            let width = area.width.saturating_sub(2).max(1);
            let lines = sticky_header::estimate_header_lines(msg, width);
            lines as u16
        })
        .unwrap_or(0);

    let status_bar_height: u16 = 3;

    let bg_bar_height_val = bg_agent_bar::bg_bar_height(app);

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

    // 缓冲消息预览（loading 时在输入框上方显示待发送消息）
    if queued_height > 0 {
        let queued_area = chunks[4];
        let msgs = &app.session_mgr.current_mut().messages.pending_messages;
        let visible_count = (pending_count).min(queued_height as usize);
        let pending_style = Style::default().fg(theme::MUTED).bg(theme::USER_BG);
        for (i, msg) in msgs.iter().take(visible_count).enumerate() {
            let line_area = Rect {
                x: queued_area.x + 2,
                y: queued_area.y + i as u16,
                width: queued_area.width.saturating_sub(2),
                height: 1,
            };
            // 截断到可用宽度（字符级安全）
            let max_chars = line_area.width as usize;
            let display: String = msg.chars().take(max_chars.saturating_sub(3)).collect();
            let suffix = if msg.chars().count() > max_chars.saturating_sub(3) {
                "…"
            } else {
                ""
            };
            f.render_widget(
                Paragraph::new(format!("{}{}", display, suffix)).style(pending_style),
                line_area,
            );
        }
        if pending_count > visible_count {
            let more_area = Rect {
                x: queued_area.x + 2,
                y: queued_area.y + visible_count as u16,
                width: queued_area.width.saturating_sub(2),
                height: 1,
            };
            f.render_widget(
                Paragraph::new(format!("… +{} more", pending_count - visible_count))
                    .style(pending_style),
                more_area,
            );
        }
    }

    // 输入框样式：Bar 焦点变暗 / 聚焦只读模式 / 正常模式
    let bar_focused = app.session_mgr.current_mut().ui.bg_bar_cursor.is_some();
    let focused_id = app.session_mgr.current_mut().focused_instance_id.clone();

    let popup_active = app.global_ui.oauth_prompt.is_some()
        || app
            .session_mgr
            .current_mut()
            .agent
            .interaction_prompt
            .is_some();
    let shell_mode = textarea_shell_mode(app);

    if bar_focused || popup_active {
        // Bar 焦点模式：输入框变暗
        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::TOP | ratatui::widgets::Borders::BOTTOM)
            .border_style(ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray))
            .padding(Padding::new(2, 0, 0, 0));
        app.session_mgr.current_mut().ui.textarea.set_block(block);
    } else if let Some(ref id) = focused_id {
        // 聚焦只读模式：彩色边框 + agent 名称标签 + 暗色文本
        let agents = &app.session_mgr.current_mut().background_agents;
        let color_idx = agents.iter().position(|a| a.instance_id == *id);
        let color = color_idx
            .map(bg_agent_bar::agent_color)
            .unwrap_or(ratatui::style::Color::Cyan);
        let agent_name = agents
            .iter()
            .find(|a| a.instance_id == *id)
            .map(|a| a.agent_name.as_str())
            .unwrap_or("agent");
        let title = format!("[{}]", agent_name);
        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::TOP | ratatui::widgets::Borders::BOTTOM)
            .border_style(ratatui::style::Style::default().fg(color))
            .padding(Padding::new(2, 0, 0, 0))
            .title(title);
        app.session_mgr.current_mut().ui.textarea.set_block(block);
        app.session_mgr
            .current_mut()
            .ui
            .textarea
            .set_style(ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray));
    } else {
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
            .set_style(ratatui::style::Style::default().fg(theme::TEXT));
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
    f.render_widget(Paragraph::new(prompt).style(prompt_style), prompt_area);

    // 统一命令/Skills 提示条 或 @ 提及弹窗（互斥）
    if app.session_mgr.current_mut().ui.at_mention.active {
        crate::app::at_mention::popup::render_at_mention_popup(
            f,
            &app.session_mgr.current_mut().ui.at_mention,
            chunks[5],
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
        TextareaShellMode::Command => theme::ERROR,
        TextareaShellMode::Stdin | TextareaShellMode::None => theme::MUTED,
    }
}

fn textarea_shell_hint_color(mode: TextareaShellMode) -> Color {
    match mode {
        TextareaShellMode::Command => theme::ERROR,
        TextareaShellMode::Stdin | TextareaShellMode::None => theme::MUTED,
    }
}

fn textarea_prompt(mode: TextareaShellMode, _loading: bool) -> (&'static str, Style) {
    match mode {
        TextareaShellMode::Command => (
            "!",
            Style::default()
                .fg(theme::ERROR)
                .add_modifier(Modifier::BOLD),
        ),
        TextareaShellMode::Stdin | TextareaShellMode::None => {
            ("❯", Style::default().fg(theme::MUTED))
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
    let mut display_textarea = textarea.clone();
    if shell_mode == TextareaShellMode::Command {
        hide_shell_prefix_for_display(&mut display_textarea);
    }

    // textarea 自带块光标始终设为与文本同色（视觉不可见），由 draw_bar_cursor 画 │
    display_textarea.set_cursor_style(Style::default().fg(theme::TEXT));
    f.render_widget(&display_textarea, area);

    // 聚焦 + 非 Command 模式：始终右移 cells 防止字符抖动，仅亮相时画 │
    if focused && shell_mode != TextareaShellMode::Command {
        draw_bar_cursor(f.buffer_mut(), textarea, area, cursor_visible);
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
            cell.set_style(Style::default().fg(theme::MUTED));
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
    } else if let Some(crate::app::InteractionPrompt::Approval(p)) =
        &app.session_mgr.current().agent.interaction_prompt
    {
        (p.items.len() as u16 * 2 + 5).max(5)
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
