use anyhow::Result;
use base64::Engine as _;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use std::time::Duration;
use tui_textarea::{Input, Key};

use crate::app::panel_manager::{EventResult, PanelContext, PanelKind};
use crate::app::plugin_panel::PluginPanel;
use crate::app::{App, MessageViewModel, PendingAttachment};
use crate::ui::render_thread::RenderEvent;
use rust_create_agent::messages::BaseMessage;

/// 将 RGBA 像素数据编码为 PNG，再返回 base64 字符串和 PNG 字节数
fn rgba_to_png_base64(width: u32, height: u32, rgba_bytes: &[u8]) -> Result<(String, usize)> {
    let mut png_bytes: Vec<u8> = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(rgba_bytes)?;
    }
    let size = png_bytes.len();
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    Ok((b64, size))
}

pub enum Action {
    Quit,
    Submit(String),
    Redraw,
}

/// 将选区文本复制到系统剪贴板并更新 UI 提示。返回 true 表示成功复制。
fn copy_selection_to_clipboard(app: &mut App) -> bool {
    if let Some(text) = app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .text_selection
        .selected_text
        .take()
    {
        let char_count = text.chars().count();
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(&text);
        }
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .copy_char_count = char_count;
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .copy_message_until =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(2000));
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .text_selection
            .clear();
        return true;
    }
    false
}

/// 将面板选区文本复制到系统剪贴板。返回 true 表示成功复制。
fn copy_panel_selection_to_clipboard(app: &mut App) -> bool {
    if let Some(text) = app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .panel_selection
        .selected_text
        .take()
    {
        let char_count = text.chars().count();
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(&text);
        }
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .copy_char_count = char_count;
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .copy_message_until =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(2000));
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .panel_selection
            .clear();
        return true;
    }
    false
}

pub async fn next_event(app: &mut App) -> Result<Option<Action>> {
    // 退出待确认状态 2 秒后自动过期，快捷键栏恢复正常
    // 退出待确认状态 2 秒后自动过期，触发重绘让快捷键栏恢复正常
    if let Some(since) = app.services.quit_pending_since {
        if since.elapsed() >= std::time::Duration::from_secs(1) {
            app.services.quit_pending_since = None;
            return Ok(Some(Action::Redraw));
        }
    }

    if !event::poll(Duration::from_millis(50))? {
        return Ok(None);
    }

    let ev = event::read()?;

    match ev {
        Event::Resize(w, _) => {
            let _ = app.session_mgr.sessions[app.session_mgr.active]
                .messages
                .render_tx
                .send(RenderEvent::Resize(w));
            app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .text_selection
                .clear();
        }
        Event::Key(key_event) => {
            // 只处理 Press 事件，忽略 Release（防止按键重复触发）
            if key_event.kind == KeyEventKind::Release {
                return Ok(Some(Action::Redraw));
            }

            // Shift+Tab 在 crossterm 中报告为 BackTab，
            // ratatui-textarea 的 Key 枚举不处理 BackTab（映射为 Null），
            // 因此在这里提前拦截，直接处理权限模式切换。
            if matches!(key_event.code, ratatui::crossterm::event::KeyCode::BackTab) {
                let _new_mode = app.services.permission_mode.cycle();
                app.services.mode_highlight_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
                return Ok(Some(Action::Redraw));
            }

            // Alt+M 循环切换模型别名（opus → sonnet → haiku → opus）
            // macOS 默认 Alt 作为字符修饰键，Alt+M 发送 'µ' 且不带 ALT 修饰符
            if matches!(key_event.code, KeyCode::Char('µ'))
                || (key_event.modifiers.contains(KeyModifiers::ALT)
                    && matches!(key_event.code, KeyCode::Char('m')))
            {
                if let Some(cfg) = app.services.peri_config.as_mut() {
                    let aliases = ["opus", "sonnet", "haiku"];
                    let current = cfg.config.active_alias.as_str();
                    let idx = aliases.iter().position(|&a| a == current).unwrap_or(0);
                    let next = aliases[(idx + 1) % aliases.len()];
                    cfg.config.active_alias = next.to_string();
                    if let Err(e) =
                        App::save_config(cfg, app.services.config_path_override.as_deref())
                    {
                        app.session_mgr.sessions[app.session_mgr.active]
                            .messages
                            .view_messages
                            .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
                    }
                    if let Some(p) = crate::app::agent::LlmProvider::from_config(cfg) {
                        app.services.provider_name = p.display_name().to_string();
                        app.services.model_name = p.model_name().to_string();
                    }
                    app.services.model_highlight_until =
                        Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
                }
                return Ok(Some(Action::Redraw));
            }

            // macOS: Cmd+C (Super+C) 复制选区文本（遵循系统剪贴板快捷键惯例）
            // tui_textarea::Input 没有 super 字段，需在转换前从原始 key_event 检测。
            if key_event.code == KeyCode::Char('c')
                && key_event.modifiers.contains(KeyModifiers::SUPER)
                && copy_selection_to_clipboard(app)
            {
                return Ok(Some(Action::Redraw));
            }

            // 全局复制：有选区时 Ctrl+C 优先复制，不被任何面板拦截
            if key_event.code == KeyCode::Char('c')
                && key_event.modifiers.contains(KeyModifiers::CONTROL)
                && !key_event.modifiers.contains(KeyModifiers::SHIFT)
            {
                // 优先级：消息区选区 > 面板选区 > textarea 选区
                if copy_selection_to_clipboard(app) {
                    return Ok(Some(Action::Redraw));
                }
                if copy_panel_selection_to_clipboard(app) {
                    return Ok(Some(Action::Redraw));
                }
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea
                    .is_selecting()
                {
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea
                        .copy();
                    let text = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea
                        .yank_text();
                    if !text.is_empty() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(&text);
                        }
                        let char_count = text.chars().count();
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .copy_char_count = char_count;
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .copy_message_until = Some(
                            std::time::Instant::now() + std::time::Duration::from_millis(2000),
                        );
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .cancel_selection();
                        return Ok(Some(Action::Redraw));
                    }
                }
            }

            let input = Input::from(ev);

            // Setup 向导：优先拦截所有按键事件
            if app.services.setup_wizard.is_some() {
                let input_clone = input.clone();
                if let Some(ref mut wizard) = app.services.setup_wizard {
                    if let Some(action) =
                        crate::app::setup_wizard::handle_setup_wizard_key(wizard, input_clone)
                    {
                        match action {
                            crate::app::setup_wizard::SetupWizardAction::SaveAndClose => {
                                let wizard = app
                                    .services
                                    .setup_wizard
                                    .take()
                                    .expect("setup_wizard must be Some (checked above)");
                                match crate::app::setup_wizard::save_setup(&wizard) {
                                    Ok(cfg) => app.refresh_after_setup(cfg),
                                    Err(e) => {
                                        let msg = MessageViewModel::from_base_message(
                                            &BaseMessage::system(format!("配置保存失败: {}", e)),
                                            &[],
                                        );
                                        let _ = app.session_mgr.sessions[app.session_mgr.active]
                                            .messages
                                            .render_tx
                                            .send(RenderEvent::AddMessage(msg));
                                    }
                                }
                            }
                            crate::app::setup_wizard::SetupWizardAction::Skip => {
                                app.services.setup_wizard = None;
                                return Ok(Some(Action::Quit));
                            }
                            crate::app::setup_wizard::SetupWizardAction::Redraw => {}
                        }
                    }
                }
                return Ok(Some(Action::Redraw));
            }

            // ─── PanelManager 分发 ─────────────────────────────────────────────
            {
                // Session 面板：Model, Agent, Hooks, Login, Config, ThreadBrowser
                let session_kind = app.session_mgr.sessions[app.session_mgr.active]
                    .session_panels
                    .active_kind();
                if matches!(
                    session_kind,
                    Some(PanelKind::Model)
                        | Some(PanelKind::Agent)
                        | Some(PanelKind::Hooks)
                        | Some(PanelKind::Login)
                        | Some(PanelKind::Config)
                        | Some(PanelKind::ThreadBrowser)
                ) {
                    let active_idx = app.session_mgr.active;
                    // SAFETY: 临时取出 PanelManager 避免借用冲突。
                    // dispatch_key(&self, &mut PanelContext) 需要 &self + &mut App 内的字段，
                    // Rust 借用检查器无法证明两者不重叠。take 后归还，语义等价。
                    // TODO: 若重构 dispatch 签名为独立 &mut 参数可消除此 workaround。
                    let mut pm =
                        std::mem::take(&mut app.session_mgr.sessions[active_idx].session_panels);
                    let mut ctx = PanelContext {
                        services: &mut app.services,
                        session_mgr: &mut app.session_mgr,
                    };
                    let result = pm.dispatch_key(input, &mut ctx);
                    match result {
                        EventResult::ClosePanel => {
                            pm.close();
                            app.session_mgr.sessions[active_idx]
                                .ui
                                .panel_selection
                                .clear();
                            app.session_mgr.sessions[active_idx].ui.panel_area = None;
                        }
                        EventResult::OpenThread(thread_id) => {
                            pm.close();
                            app.session_mgr.sessions[active_idx]
                                .ui
                                .panel_selection
                                .clear();
                            app.session_mgr.sessions[active_idx].ui.panel_area = None;
                            app.session_mgr.sessions[active_idx].session_panels = pm;
                            app.open_thread_with_feedback(thread_id);
                            return Ok(Some(Action::Redraw));
                        }
                        _ => {}
                    }
                    // 放回 PanelManager
                    app.session_mgr.sessions[active_idx].session_panels = pm;
                    return Ok(Some(Action::Redraw));
                }

                // Global 面板：Status, Memory, Mcp, Cron, Plugin
                let global_kind = app.global_panels.active_kind();
                if matches!(
                    global_kind,
                    Some(PanelKind::Status)
                        | Some(PanelKind::Memory)
                        | Some(PanelKind::Mcp)
                        | Some(PanelKind::Cron)
                        | Some(PanelKind::Plugin)
                ) {
                    let active_idx = app.session_mgr.active;
                    // SAFETY: 同上，global_panels 嵌套在 App 内，dispatch_key 需 &mut App 字段
                    let mut pm = std::mem::take(&mut app.global_panels);
                    let mut ctx = PanelContext {
                        services: &mut app.services,
                        session_mgr: &mut app.session_mgr,
                    };
                    let result = pm.dispatch_key(input, &mut ctx);
                    match result {
                        EventResult::ClosePanel => {
                            pm.close();
                            app.session_mgr.sessions[active_idx]
                                .ui
                                .panel_selection
                                .clear();
                            app.session_mgr.sessions[active_idx].ui.panel_area = None;
                        }
                        EventResult::OpenPanel(PanelKind::Memory) => {
                            app.global_panels = pm;
                            if let Err(e) = app.memory_panel_open_editor() {
                                tracing::error!("Failed to open editor: {}", e);
                            }
                            return Ok(Some(Action::Redraw));
                        }
                        _ => {}
                    }
                    // 放回 PanelManager
                    app.global_panels = pm;
                    return Ok(Some(Action::Redraw));
                }
            }

            // OAuth 弹窗优先处理
            if app.services.oauth_prompt.is_some() {
                handle_oauth_prompt(app, input);
                return Ok(Some(Action::Redraw));
            }

            // AskUser 批量弹窗
            if matches!(
                &app.session_mgr.sessions[app.session_mgr.active]
                    .agent
                    .interaction_prompt,
                Some(crate::app::InteractionPrompt::Questions(_))
            ) {
                match input {
                    Input {
                        key: Key::Char('c'),
                        ctrl: true,
                        ..
                    } => return Ok(Some(Action::Quit)),
                    // Tab / Shift+Tab 切换问题
                    Input {
                        key: Key::Tab,
                        shift: false,
                        ..
                    } => app.ask_user_next_tab(),
                    Input {
                        key: Key::Tab,
                        shift: true,
                        ..
                    } => app.ask_user_prev_tab(),
                    // Enter 提交所有答案
                    Input {
                        key: Key::Enter, ..
                    } => app.ask_user_confirm(),
                    // 上下移动当前问题内的选项光标
                    Input { key: Key::Up, .. } => app.ask_user_move(-1),
                    Input { key: Key::Down, .. } => app.ask_user_move(1),
                    // Space 切换选中
                    Input {
                        key: Key::Char(' '),
                        ..
                    } => app.ask_user_toggle(),
                    // 文字输入（自定义输入模式下）— 使用公共编辑函数
                    _ => {
                        app.ask_user_edit_key(input);
                    }
                }
                return Ok(Some(Action::Redraw));
            }

            // HITL 批量弹窗激活时，优先处理弹窗按键
            if matches!(
                &app.session_mgr.sessions[app.session_mgr.active]
                    .agent
                    .interaction_prompt,
                Some(crate::app::InteractionPrompt::Approval(_))
            ) {
                match input {
                    Input {
                        key: Key::Char('c'),
                        ctrl: true,
                        ..
                    } => return Ok(Some(Action::Quit)),

                    // 上下移动光标
                    Input { key: Key::Up, .. } => app.hitl_move(-1),
                    Input { key: Key::Down, .. } => app.hitl_move(1),

                    // Space：切换当前项
                    Input {
                        key: Key::Char(' '),
                        ..
                    } => app.hitl_toggle(),

                    // Enter：按当前各项选择确认
                    Input {
                        key: Key::Enter, ..
                    } => app.hitl_confirm(),

                    _ => {}
                }
                return Ok(Some(Action::Redraw));
            }

            match input {
                // Ctrl+C：有选区时复制优先（已全局拦截），无选区时中断/双击退出
                Input {
                    key: Key::Char('c'),
                    ctrl: true,
                    ..
                } => {
                    if app.session_mgr.sessions[app.session_mgr.active].ui.loading {
                        // agent 运行中：优先中断，清除退出待确认状态
                        app.interrupt();
                        app.services.quit_pending_since = None;
                    } else if let Some(since) = app.services.quit_pending_since {
                        // 非 loading，2 秒内再次 Ctrl+C → 退出
                        if since.elapsed() < std::time::Duration::from_secs(2) {
                            return Ok(Some(Action::Quit));
                        } else {
                            // 超时，重新开始计时
                            app.services.quit_pending_since = Some(std::time::Instant::now());
                        }
                    } else {
                        // 第一次 Ctrl+C，进入退出待确认状态
                        app.services.quit_pending_since = Some(std::time::Instant::now());
                    }
                }

                // ESC：主界面不再退出，仅用于 loading 时清除缓冲
                Input { key: Key::Esc, .. }
                    if app.session_mgr.sessions[app.session_mgr.active].ui.loading =>
                {
                    if !app.session_mgr.sessions[app.session_mgr.active]
                        .messages
                        .pending_messages
                        .is_empty()
                    {
                        app.session_mgr.sessions[app.session_mgr.active]
                            .messages
                            .pending_messages
                            .clear();
                    }
                }

                // Up：浮层导航 > 历史恢复（仅首行）> textarea 光标
                Input { key: Key::Up, .. }
                    if !app.session_mgr.sessions[app.session_mgr.active].ui.loading =>
                {
                    let hint_count = app.hint_candidates_count();
                    if hint_count > 0 {
                        let cur = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor
                            .unwrap_or(0);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor = if cur == 0 {
                            Some(hint_count - 1)
                        } else {
                            Some(cur - 1)
                        };
                    } else {
                        let (row, _col) = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .cursor();
                        if row == 0 {
                            app.history_up();
                        } else {
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .textarea
                                .input(Input {
                                    key: Key::Up,
                                    ctrl: false,
                                    alt: false,
                                    shift: false,
                                });
                        }
                    }
                }

                // Down：浮层导航 > 历史恢复（仅末行）> textarea 光标
                Input { key: Key::Down, .. }
                    if !app.session_mgr.sessions[app.session_mgr.active].ui.loading =>
                {
                    let hint_count = app.hint_candidates_count();
                    if hint_count > 0 {
                        let cur = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor
                            .unwrap_or(hint_count - 1);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor = if cur + 1 >= hint_count {
                            Some(0)
                        } else {
                            Some(cur + 1)
                        };
                    } else if app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .history_index
                        .is_some()
                    {
                        app.history_down();
                    } else {
                        let (row, _col) = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .cursor();
                        let last_row = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .lines()
                            .len()
                            .saturating_sub(1);
                        if row >= last_row {
                            app.history_down();
                        } else {
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .textarea
                                .input(Input {
                                    key: Key::Down,
                                    ctrl: false,
                                    alt: false,
                                    shift: false,
                                });
                        }
                    }
                }

                // Ctrl+V：优先尝试粘贴剪贴板图片，失败则回退到粘贴文字
                Input {
                    key: Key::Char('v'),
                    ctrl: true,
                    ..
                } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading => {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        if let Ok(img) = clipboard.get_image() {
                            let (w, h) = (img.width as u32, img.height as u32);
                            if let Ok((b64, sz)) = rgba_to_png_base64(w, h, &img.bytes) {
                                let n = app.session_mgr.sessions[app.session_mgr.active]
                                    .metadata
                                    .pending_attachments
                                    .len()
                                    + 1;
                                app.add_pending_attachment(PendingAttachment {
                                    label: format!("clipboard_{}.png", n),
                                    media_type: "image/png".to_string(),
                                    base64_data: b64,
                                    size_bytes: sz,
                                });
                            }
                        } else if let Ok(text) = clipboard.get_text() {
                            let text = text.replace('\r', "\n");
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .textarea
                                .insert_str(&text);
                        }
                    }
                }

                // Tab：提示浮层候选导航与补全
                Input {
                    key: Key::Tab,
                    shift: false,
                    ..
                } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading => {
                    let count = app.hint_candidates_count();
                    if count > 0 {
                        match app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor
                        {
                            Some(cur) if cur + 1 < count => {
                                app.session_mgr.sessions[app.session_mgr.active]
                                    .ui
                                    .hint_cursor = Some(cur + 1);
                            }
                            Some(_) => {
                                // 已在最后一个，循环到第一个
                                app.session_mgr.sessions[app.session_mgr.active]
                                    .ui
                                    .hint_cursor = Some(0);
                            }
                            None => {
                                // 首次按 Tab，选中第一个
                                app.session_mgr.sessions[app.session_mgr.active]
                                    .ui
                                    .hint_cursor = Some(0);
                            }
                        }
                    }
                }

                // Enter 在有候选项时：确认选中（无选中则默认第一项）
                Input {
                    key: Key::Enter, ..
                } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading
                    && app.hint_candidates_count() > 0 =>
                {
                    if app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .hint_cursor
                        .is_none()
                    {
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor = Some(0);
                    }
                    app.hint_complete();
                }

                // Alt+Enter：插入换行
                Input {
                    key: Key::Enter,
                    alt: true,
                    ..
                } => {
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea
                        .input(Input {
                            key: Key::Enter,
                            ctrl: false,
                            alt: false,
                            shift: false,
                        });
                }

                // Enter：提交（非 loading）或缓冲（loading）
                Input {
                    key: Key::Enter, ..
                } => {
                    let text = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea
                        .lines()
                        .join("\n");
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        if app.session_mgr.sessions[app.session_mgr.active].ui.loading {
                            // Loading 状态：缓冲消息
                            app.session_mgr.sessions[app.session_mgr.active]
                                .messages
                                .pending_messages
                                .push(text);
                            app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
                                crate::app::build_textarea(false);
                            app.update_textarea_hint();
                        } else if text.starts_with('/') {
                            app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
                                crate::app::build_textarea(false);
                            // SAFETY: 同上，command_registry 嵌套在 App 内，dispatch 需 &mut App
                            let registry = std::mem::take(
                                &mut app.session_mgr.sessions[app.session_mgr.active]
                                    .commands
                                    .command_registry,
                            );
                            let known = registry.dispatch(app, &text);
                            app.session_mgr.sessions[app.session_mgr.active]
                                .commands
                                .command_registry = registry;
                            if known {
                                // 命令命中，结束
                            } else {
                                // 命令未命中，尝试 Skill 匹配
                                let skill_name: String = text
                                    .trim_start_matches('/')
                                    .chars()
                                    .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                                    .collect();
                                if let Some(_skill) = app.session_mgr.sessions
                                    [app.session_mgr.active]
                                    .commands
                                    .skills
                                    .iter()
                                    .find(|s| s.name == skill_name)
                                {
                                    // Skill 命中：将整条消息提交给 agent
                                    return Ok(Some(Action::Submit(text)));
                                } else {
                                    // 区分"前缀歧义"和"完全未知"
                                    let prefix = text.trim_start_matches('/').to_string();
                                    let cmd_matches = app.session_mgr.sessions
                                        [app.session_mgr.active]
                                        .commands
                                        .command_registry
                                        .match_prefix(&prefix);
                                    let error_msg = if cmd_matches.len() > 1 {
                                        let names: Vec<&str> =
                                            cmd_matches.iter().map(|(n, _)| *n).collect();
                                        format!(
                                            "命令 '{}' 匹配多个: {}  （请输入完整命令名）",
                                            text,
                                            names
                                                .iter()
                                                .map(|n| format!("/{}", n))
                                                .collect::<Vec<_>>()
                                                .join(", ")
                                        )
                                    } else {
                                        format!(
                                            "未知命令或 Skill: {}  （输入 /help 查看可用命令）",
                                            text
                                        )
                                    };
                                    app.session_mgr.sessions[app.session_mgr.active]
                                        .messages
                                        .view_messages
                                        .push(MessageViewModel::system(error_msg));
                                }
                            }
                        } else {
                            app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
                                crate::app::build_textarea(false);
                            return Ok(Some(Action::Submit(text)));
                        }
                    }
                }

                Input {
                    key: Key::PageUp, ..
                } => {
                    for _ in 0..10 {
                        app.scroll_up();
                    }
                }
                Input {
                    key: Key::PageDown, ..
                } => {
                    for _ in 0..10 {
                        app.scroll_down();
                    }
                }

                // Del：删除最后一个待发送附件（有附件时优先消费 Del）
                Input {
                    key: Key::Delete, ..
                } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading
                    && !app.session_mgr.sessions[app.session_mgr.active]
                        .metadata
                        .pending_attachments
                        .is_empty() =>
                {
                    app.pop_pending_attachment();
                }

                // Ctrl+N/P：切换 session 焦点
                Input {
                    key: Key::Char('n'),
                    ctrl: true,
                    ..
                } => {
                    app.switch_next_session();
                }
                Input {
                    key: Key::Char('p'),
                    ctrl: true,
                    ..
                } => {
                    app.switch_prev_session();
                }

                // Ctrl+W：关闭当前 session
                input @ Input {
                    key: Key::Char('w'),
                    ctrl: true,
                    ..
                } => {
                    if app.close_session().is_some() {
                        // session 已关闭，不继续处理
                    } else {
                        // 只有一个 session，fallback 到 textarea
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .input(input);
                    }
                }

                // 拦截普通 Enter，避免 textarea 默认换行；允许 loading 时输入
                input if input.key != Key::Enter => {
                    // 退出历史浏览
                    if app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .history_index
                        .is_some()
                    {
                        app.exit_history();
                    }
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea
                        .input(input);
                    // 输入内容变化时：重置光标（不预选，等用户按 Tab/上下键激活）
                    if !app.session_mgr.sessions[app.session_mgr.active].ui.loading {
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor = None;
                    }
                }

                _ => {
                    // 任何其他按键取消退出待确认状态
                    app.services.quit_pending_since = None;
                }
            }
        }
        Event::Paste(text) => {
            // 粘贴文本处理
            // 某些终端（如 VSCode）在 bracketed paste 中使用 \r 而非 \n 作为换行符
            let text = text.replace('\r', "\n");

            // setup_wizard 打开时粘贴到当前字段
            if let Some(wizard) = &mut app.services.setup_wizard {
                wizard.paste_text(&text);
                return Ok(Some(Action::Redraw));
            }

            // ─── PanelManager 粘贴分发（已迁移的面板）────────────────
            {
                // Session 面板：Model, Agent, Hooks, Login, Config, ThreadBrowser
                let session_kind = app.session_mgr.sessions[app.session_mgr.active]
                    .session_panels
                    .active_kind();
                if matches!(
                    session_kind,
                    Some(PanelKind::Model)
                        | Some(PanelKind::Agent)
                        | Some(PanelKind::Hooks)
                        | Some(PanelKind::Login)
                        | Some(PanelKind::Config)
                        | Some(PanelKind::ThreadBrowser)
                ) {
                    let active_idx = app.session_mgr.active;
                    // SAFETY: 同上，session_panels 嵌套在 App 内，dispatch_paste 需 &mut App 字段
                    let mut pm =
                        std::mem::take(&mut app.session_mgr.sessions[active_idx].session_panels);
                    let mut ctx = PanelContext {
                        services: &mut app.services,
                        session_mgr: &mut app.session_mgr,
                    };
                    pm.dispatch_paste(&text, &mut ctx);
                    app.session_mgr.sessions[active_idx].session_panels = pm;
                    return Ok(Some(Action::Redraw));
                }

                // Global 面板：Status, Memory, Mcp, Cron, Plugin
                let global_kind = app.global_panels.active_kind();
                if matches!(
                    global_kind,
                    Some(PanelKind::Status)
                        | Some(PanelKind::Memory)
                        | Some(PanelKind::Mcp)
                        | Some(PanelKind::Cron)
                        | Some(PanelKind::Plugin)
                ) {
                    // SAFETY: 同上，global_panels 嵌套在 App 内，dispatch_paste 需 &mut App 字段
                    let mut pm = std::mem::take(&mut app.global_panels);
                    let mut ctx = PanelContext {
                        services: &mut app.services,
                        session_mgr: &mut app.session_mgr,
                    };
                    pm.dispatch_paste(&text, &mut ctx);
                    app.global_panels = pm;
                    return Ok(Some(Action::Redraw));
                }
            }

            // 其他情况粘贴到 textarea
            app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .textarea
                .insert_str(&text);
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => {
                // MCP 面板区域滚轮滚动面板，否则滚动消息区
                if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_area
                {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                        && app.global_panels.is_active(PanelKind::Mcp)
                    {
                        app.mcp_panel_scroll_up(3);
                        return Ok(Some(Action::Redraw));
                    }
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                        && app.global_panels.is_active(PanelKind::Plugin)
                    {
                        if let Some(panel) = &mut app.global_panels.get_mut::<PluginPanel>() {
                            panel.scroll_offset = panel.scroll_offset.saturating_sub(3);
                        }
                        return Ok(Some(Action::Redraw));
                    }
                }
                app.scroll_up();
            }
            MouseEventKind::ScrollDown => {
                if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_area
                {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                        && app.global_panels.is_active(PanelKind::Mcp)
                    {
                        app.mcp_panel_scroll_down(3);
                        return Ok(Some(Action::Redraw));
                    }
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                        && app.global_panels.is_active(PanelKind::Plugin)
                    {
                        if let Some(panel) = &mut app.global_panels.get_mut::<PluginPanel>() {
                            let max = panel.current_list_len() as u16;
                            panel.scroll_offset = (panel.scroll_offset + 3).min(max);
                        }
                        return Ok(Some(Action::Redraw));
                    }
                }
                app.scroll_down();
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // 多 session：点击非 active session 列区域时切换焦点
                if app.session_mgr.sessions.len() > 1 {
                    for (i, area) in app.session_mgr.session_areas.iter().enumerate() {
                        if mouse.column >= area.x
                            && mouse.column < area.x + area.width
                            && mouse.row >= area.y
                            && mouse.row < area.y + area.height
                            && i != app.session_mgr.active
                        {
                            app.session_mgr.active = i;
                            return Ok(Some(Action::Redraw));
                        }
                    }
                }
                // 面板区域：开始面板选区
                if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_area
                {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let content_row = mouse.row - area.y
                            + app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .panel_scroll_offset;
                        let col = mouse.column - area.x;
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .panel_selection
                            .start_drag(content_row, col);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .text_selection
                            .clear();
                        // 不再处理其他区域的选区
                        return Ok(Some(Action::Redraw));
                    }
                }
                if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .messages_area
                {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let visual_row = mouse.row - area.y
                            + app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .scroll_offset;
                        let visual_col = mouse.column - area.x;
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .text_selection
                            .start_drag(visual_row, visual_col);
                    }
                }
                // 输入框区域：开始 textarea 选区
                if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea_area
                {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let row = (mouse.row - area.y).saturating_sub(1) as usize; // 跳过顶部边框
                        let col = mouse.column.saturating_sub(area.x) as usize;
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .move_cursor(tui_textarea::CursorMove::Jump(row as u16, col as u16));
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .start_selection();
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // 面板选区拖拽
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_selection
                    .dragging
                {
                    if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .panel_area
                    {
                        let content_row = mouse.row.saturating_sub(area.y).saturating_add(
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .panel_scroll_offset,
                        );
                        let col = mouse.column.saturating_sub(area.x);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .panel_selection
                            .update_drag(content_row, col);
                    }
                }
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .text_selection
                    .dragging
                {
                    if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .messages_area
                    {
                        let visual_row = mouse.row.saturating_sub(area.y).saturating_add(
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .scroll_offset,
                        );
                        let visual_col = mouse.column.saturating_sub(area.x);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .text_selection
                            .update_drag(visual_row, visual_col);
                    }
                }
                // 输入框区域：扩展 textarea 选区
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea
                    .is_selecting()
                {
                    if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea_area
                    {
                        if mouse.row >= area.y && mouse.row < area.y + area.height {
                            let row = (mouse.row - area.y).saturating_sub(1) as usize;
                            let col = mouse.column.saturating_sub(area.x) as usize;
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .textarea
                                .move_cursor(tui_textarea::CursorMove::Jump(
                                    row as u16, col as u16,
                                ));
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // 面板选区松开
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_selection
                    .dragging
                {
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .panel_selection
                        .end_drag();
                    let sel = &app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .panel_selection;
                    if let (Some(start), Some(end)) = (sel.start, sel.end) {
                        let text = crate::app::text_selection::extract_panel_text(
                            start,
                            end,
                            &app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .panel_plain_lines,
                        );
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .panel_selection
                            .set_selected_text(text);
                    }
                }
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .text_selection
                    .dragging
                {
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .text_selection
                        .end_drag();
                    let ts = &app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .text_selection;
                    if let (Some(start), Some(end)) = (ts.start, ts.end) {
                        let usable_width = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .messages_area
                            .map(|a| a.width.saturating_sub(1))
                            .unwrap_or(0);
                        let cache = app.session_mgr.sessions[app.session_mgr.active]
                            .messages
                            .render_cache
                            .read();
                        let text = crate::app::text_selection::extract_selected_text(
                            start,
                            end,
                            &cache.wrap_map,
                            usable_width,
                        );
                        drop(cache);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .text_selection
                            .set_selected_text(text);
                    }
                }
                // textarea 选区在 mouse up 时不做额外处理，保持 tui_textarea 的选区状态
            }
            _ => {}
        },
        _ => {}
    }

    Ok(Some(Action::Redraw))
}

fn handle_oauth_prompt(app: &mut App, input: Input) {
    use crate::app::handle_edit_key;
    let prompt = match app.services.oauth_prompt.as_mut() {
        Some(p) => p,
        None => return,
    };
    match input {
        Input {
            key: Key::Enter, ..
        } => {
            if prompt.submit() {
                app.services.oauth_prompt = None;
            }
        }
        Input {
            key: Key::Char('o'),
            ctrl: true,
            ..
        } => {
            let url = prompt.authorization_url.clone();
            #[cfg(unix)]
            let _ = std::process::Command::new("open").arg(&url).spawn();
            #[cfg(windows)]
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", &url])
                .spawn();
        }
        Input { key: Key::Esc, .. } => {
            app.services.oauth_prompt = None;
        }
        Input {
            key: Key::Char('c'),
            ctrl: true,
            ..
        } => {
            // Ctrl+C 在弹窗中不退出，忽略
        }
        _ => {
            prompt.error_message = None;
            handle_edit_key(&mut prompt.input, &mut prompt.cursor, input);
        }
    }
}
