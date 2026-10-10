use tui_textarea::{CursorMove, Input, Key};

use crate::app::{App, MessageViewModel};

use super::super::Action;

/// Normal mode key handling: main match block arm bodies
pub(super) fn handle_normal_keys(app: &mut App, input: Input) -> anyhow::Result<Option<Action>> {
    use super::{inject_at_mention_path, update_at_mention_detection};

    match input {
        // Ctrl+C: interrupt agent / double-tap to quit
        Input {
            key: Key::Char('c'),
            ctrl: true,
            ..
        } => {
            if let Some(action) = handle_ctrl_c(app) {
                return Ok(Some(action));
            }
        }

        // ESC: 中断 agent 运行（与 Ctrl+C 行为一致）
        Input { key: Key::Esc, .. } if app.session_mgr.current_mut().ui.loading => {
            app.interrupt();
        }

        // Esc: 关闭 @ 提及弹窗
        Input { key: Key::Esc, .. } if app.session_mgr.current_mut().ui.at_mention.active => {
            app.session_mgr.current_mut().ui.at_mention.close();
        }

        // Esc: 双击触发 rewind 选择器（仅空闲时）
        Input { key: Key::Esc, .. } if !app.session_mgr.current().ui.loading => {
            if let Some(since) = app.global_ui.rewind_pending_since {
                if since.elapsed() < std::time::Duration::from_secs(2) {
                    // 双击 ESC → 打开 rewind 选择器
                    app.global_ui.rewind_pending_since = None;
                    app.open_rewind_prompt();
                } else {
                    app.global_ui.rewind_pending_since = Some(std::time::Instant::now());
                }
            } else {
                app.global_ui.rewind_pending_since = Some(std::time::Instant::now());
            }
        }

        // Ctrl+Up / Ctrl+Down：textarea 光标移动 / 命令历史（原 textarea Up/Down 功能）。
        // 纯 Up/Down 让给消息区滚动（含滚轮经 ConPTY 变成的方向键），避免与 textarea 冲突。
        Input {
            key: Key::Up,
            ctrl: true,
            ..
        } => handle_ctrl_up(app),
        Input {
            key: Key::Down,
            ctrl: true,
            ..
        } => handle_ctrl_down(app),

        // Up：@ 提及导航 > hint 导航 > 滚动消息区
        Input { key: Key::Up, .. } => {
            if let Some(action) = handle_up(app) {
                return Ok(Some(action));
            }
        }

        // Down：有运行中的后台 shell 时聚焦 status bar 入口，否则 @ 提及导航 > hint 导航 > 滚动消息区
        Input { key: Key::Down, .. } => {
            if app.has_running_background_shell_tasks() {
                app.session_mgr
                    .current_mut()
                    .ui
                    .background_tasks_bar_focused = true;
                return Ok(Some(Action::Redraw));
            }
            if let Some(action) = handle_down(app) {
                return Ok(Some(action));
            }
        }

        // Ctrl+V / Alt+V: try pasting clipboard image first, fallback to text paste
        // Alt+V 用于规避现代终端（如 Windows Terminal / VS Code）对 Ctrl+V 的宿主拦截
        Input {
            key: Key::Char('v') | Key::Char('V'),
            ctrl: true,
            ..
        }
        | Input {
            key: Key::Char('v') | Key::Char('V'),
            alt: true,
            ..
        }
        | Input {
            key: Key::Char('√'), // macOS Option+V compose char
            ..
        } => handle_ctrl_v(app),

        // Ctrl+Enter / Alt+S / Alt+X：排队消息键盘操作，等价鼠标 [Send now] / [×]。
        // 主推荐快捷键为 Ctrl+Enter，并保持兼容 Alt+S 及 macOS ⌥S 组合字符 'ß'。
        //
        // 不用 Ctrl+S：Windows 控制台（conhost/ConPTY）把 Ctrl+S 当作暂停键（XOFF）
        // 在宿主层截走并丢弃，事件根本到不了应用层。
        // macOS Option 会组合出特殊字符（⌥S=ß、⌥X=≈，⌥V=√），一并匹配。
        //
        // 仅在存在可操作的队首消息时拦截，否则放行给 textarea。
        Input {
            key: Key::Enter,
            ctrl: true,
            ..
        }
        | Input {
            key: Key::Char('s') | Key::Char('S'),
            alt: true,
            ..
        }
        | Input {
            key: Key::Char('ß'), // macOS Option+S compose char
            ..
        } if app.first_actionable_queued_id().is_some() => {
            app.steer_first_queued_message();
        }

        Input {
            key: Key::Char('x') | Key::Char('X'),
            ctrl: true,
            ..
        }
        | Input {
            key: Key::Char('x') | Key::Char('X'),
            alt: true,
            ..
        }
        | Input {
            key: Key::Char('≈'), // macOS Option+X compose char
            ..
        } if app.first_actionable_queued_id().is_some() => {
            app.delete_first_queued_message();
        }

        // Tab: @ 提及补全 > hint overlay candidate navigation and completion
        Input {
            key: Key::Tab,
            shift: false,
            ..
        } if !app.session_mgr.current_mut().ui.loading => handle_tab(app),

        // Enter with @ mention active and candidates: inject selected path
        Input {
            key: Key::Enter, ..
        } if !app.session_mgr.current_mut().ui.loading
            && app.session_mgr.current_mut().ui.at_mention.active
            && !app
                .session_mgr
                .current_mut()
                .ui
                .at_mention
                .candidates
                .is_empty() =>
        {
            inject_at_mention_path(app);
        }

        // Enter with hints available: confirm selection (defaults to first if none selected)
        Input {
            key: Key::Enter, ..
        } if !app.session_mgr.current_mut().ui.loading && app.hint_candidates_count() > 0 => {
            if app.session_mgr.current_mut().ui.hint_cursor.is_none() {
                app.session_mgr.current_mut().ui.hint_cursor = Some(0);
            }
            app.hint_complete();
        }

        // Shift+Enter / Alt+Enter: insert newline (Shift works everywhere; Alt (Option) for macOS)
        Input {
            key: Key::Enter, ..
        } if input.shift || input.alt => {
            app.session_mgr.current_mut().ui.textarea.input(Input {
                key: Key::Enter,
                ctrl: false,
                alt: false,
                shift: false,
            });
        }

        // Enter: submit (non-loading) or buffer (loading)
        Input {
            key: Key::Enter, ..
        } => {
            // 关闭可能残留的 @ mention 弹窗
            if app.session_mgr.current_mut().ui.at_mention.active {
                app.session_mgr.current_mut().ui.at_mention.close();
            }
            let raw_text = app.session_mgr.current_mut().ui.textarea.lines().join("\n");
            if app.session_mgr.current_mut().ui.loading && app.is_shell_command_running() {
                app.send_shell_stdin_line(raw_text);
                return Ok(Some(Action::Redraw));
            }
            let text = raw_text.trim().to_string();
            if !text.is_empty() {
                if app.session_mgr.current_mut().ui.loading {
                    app.queue_user_message(text);
                } else if let Some(command) = text.strip_prefix('!') {
                    let command = command.trim().to_string();
                    app.session_mgr.current_mut().ui.textarea = crate::app::build_textarea(false);
                    if command.is_empty() {
                        app.session_mgr
                            .current_mut()
                            .messages
                            .view_messages
                            .push(MessageViewModel::system("请输入 shell 命令".to_string()));
                        app.render_rebuild();
                    } else {
                        return Ok(Some(Action::RunShellCommand(command)));
                    }
                } else if text.starts_with('/') {
                    app.session_mgr.current_mut().ui.textarea = crate::app::build_textarea(false);
                    // SAFETY: command_registry is nested inside App; dispatch needs &mut App
                    let registry = std::mem::take(
                        &mut app.session_mgr.current_mut().commands.command_registry,
                    );
                    let known = registry.dispatch(app, &text);
                    app.session_mgr.current_mut().commands.command_registry = registry;
                    if known {
                        // Command matched, done
                    } else {
                        // Command not matched, try Skill matching
                        let skill_name: String = text
                            .trim_start_matches('/')
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                            .collect();
                        if let Some(_skill) = app
                            .session_mgr
                            .current_mut()
                            .commands
                            .skills
                            .iter()
                            .find(|s| s.name == skill_name)
                        {
                            // Skill matched: submit full message to agent
                            return Ok(Some(Action::Submit(text)));
                        } else if app
                            .session_mgr
                            .current_mut()
                            .commands
                            .agent_commands
                            .contains(&skill_name)
                        {
                            // Agent command matched (from ACP AvailableCommandsUpdate): submit to agent
                            tracing::debug!(skill_name, "Matched agent command, submitting to ACP");
                            return Ok(Some(Action::Submit(text)));
                        } else {
                            tracing::debug!(
                                skill_name,
                                agent_commands = ?app.session_mgr.current_mut()
                                    .commands
                                    .agent_commands,
                                "Command not found in local registry, skills, or agent_commands"
                            );
                            // Distinguish "prefix ambiguity" from "completely unknown"
                            let prefix = text.trim_start_matches('/').to_string();
                            let cmd_matches = app
                                .session_mgr
                                .current_mut()
                                .commands
                                .command_registry
                                .match_prefix(&prefix, &app.services.lc);
                            let error_msg = if cmd_matches.len() > 1 {
                                let names: Vec<&str> =
                                    cmd_matches.iter().map(|(n, _)| n.as_str()).collect();
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
                                format!("未知命令或 Skill: {}  （输入 /help 查看可用命令）", text)
                            };
                            app.session_mgr
                                .current_mut()
                                .messages
                                .view_messages
                                .push(MessageViewModel::system(error_msg));
                        }
                    }
                } else {
                    app.session_mgr.current_mut().ui.textarea = crate::app::build_textarea(false);
                    return Ok(Some(Action::Submit(text)));
                }
            }
        }

        // VS Code terminal maps Option+Backspace to PageUp; perform word-delete when textarea has content
        Input {
            key: Key::PageUp, ..
        } if std::env::var("TERM_PROGRAM").as_deref() == Ok("vscode") => {
            let session = &mut app.session_mgr.current_mut();
            let has_content = session
                .ui
                .textarea
                .lines()
                .iter()
                .any(|line| !line.is_empty());
            if has_content {
                session.ui.textarea.delete_word();
            }
        }

        // Ctrl+D: close shell stdin when shell command is running
        Input {
            key: Key::Char('d'),
            ctrl: true,
            ..
        } if app.is_shell_command_running() => {
            app.close_shell_stdin();
        }

        // PageUp: half-page scroll (only when textarea is empty)
        Input {
            key: Key::PageUp, ..
        } => {
            let has_content = app
                .session_mgr
                .current_mut()
                .ui
                .textarea
                .lines()
                .iter()
                .any(|line| !line.is_empty());
            if !has_content {
                for _ in 0..20 {
                    app.scroll_up();
                }
            }
        }
        // PageDown: half-page scroll (only when textarea is empty)
        Input {
            key: Key::PageDown, ..
        } => {
            let has_content = app
                .session_mgr
                .current_mut()
                .ui
                .textarea
                .lines()
                .iter()
                .any(|line| !line.is_empty());
            if !has_content {
                for _ in 0..20 {
                    app.scroll_down();
                }
            }
        }
        // Home: textarea 有内容时光标移到行首，否则滚动到顶
        Input { key: Key::Home, .. } => {
            let has_content = app
                .session_mgr
                .current_mut()
                .ui
                .textarea
                .lines()
                .iter()
                .any(|line| !line.is_empty());
            if has_content {
                app.session_mgr
                    .current_mut()
                    .ui
                    .textarea
                    .move_cursor(CursorMove::Head);
                app.session_mgr.current_mut().ui.reset_cursor_blink();
            } else {
                app.scroll_to_top();
            }
        }
        // End: textarea 有内容时光标移到行尾，否则滚动到底
        Input { key: Key::End, .. } => {
            let has_content = app
                .session_mgr
                .current_mut()
                .ui
                .textarea
                .lines()
                .iter()
                .any(|line| !line.is_empty());
            if has_content {
                app.session_mgr
                    .current_mut()
                    .ui
                    .textarea
                    .move_cursor(CursorMove::End);
                app.session_mgr.current_mut().ui.reset_cursor_blink();
            } else {
                app.scroll_to_bottom();
            }
        }

        // Del: remove last pending attachment
        Input {
            key: Key::Delete, ..
        } if !app.session_mgr.current_mut().ui.loading
            && !app
                .session_mgr
                .current_mut()
                .metadata
                .pending_attachments
                .is_empty() =>
        {
            app.pop_pending_attachment();
        }

        // Intercept plain Enter to avoid textarea default newline; allow input during loading
        input if input.key != Key::Enter => {
            // Exit history browsing
            if app.session_mgr.current_mut().ui.history_index.is_some() {
                app.exit_history();
            }

            // 拦截 Backspace：若光标前是 [Image #N] 占位符，整体删除并联动附件。
            // 占位符作为 textarea 内原子元素，Backspace 不应只删一个字符破坏占位符。
            let intercepted = matches!(input.key, Key::Backspace)
                && !app.session_mgr.current_mut().ui.loading
                && try_delete_image_placeholder_backspace(app);

            if !intercepted {
                app.session_mgr.current_mut().ui.textarea.input(input);
            }
            app.session_mgr.current_mut().ui.reset_cursor_blink();
            // When input changes: reset cursor (don't pre-select; wait for user to press Tab/Up/Down)
            if !app.session_mgr.current_mut().ui.loading {
                app.session_mgr.current_mut().ui.hint_cursor = None;
                update_at_mention_detection(app);
            }
        }

        _ => {
            // Any other key cancels quit-pending state (Ctrl+C double-tap)
            app.global_ui.quit_pending_since = None;
            // Note: do NOT reset rewind_pending_since here. The fallback arm
            // captures keys like Key::Enter (with unmatched modifiers) and
            // terminal-generated sequences (e.g. focus events, unknown keys).
            // Resetting here would break the ESC double-tap detection because
            // spurious key events between two ESC presses would clear the state.
            // rewind_pending_since is naturally reset when the user types actual
            // content (the `input if input.key != Key::Enter` arm above).
        }
    }

    Ok(Some(Action::Redraw))
}

// ── Per-arm helper functions ──────────────────────────────────────────────

fn handle_ctrl_c(app: &mut App) -> Option<Action> {
    // Agent 运行中 → 中断 agent
    if app.session_mgr.current_mut().ui.loading {
        app.interrupt();
        app.global_ui.quit_pending_since = None;
        return None;
    }

    // quit-pending: 2 秒内连按两次退出
    if let Some(since) = app.global_ui.quit_pending_since {
        // 防抖：100ms 内的重复事件视为同一次按键，忽略。
        // Windows Terminal (ConPTY) 下 ctrl_handler 注入的 KeyDown 与原生
        // KeyDown 间隔约 0-1ms，不加防抖会误触发退出。
        if since.elapsed() < std::time::Duration::from_millis(100) {
            return None;
        }
        if since.elapsed() < std::time::Duration::from_secs(2) {
            return Some(Action::Quit);
        } else {
            app.global_ui.quit_pending_since = Some(std::time::Instant::now());
        }
    } else {
        app.global_ui.quit_pending_since = Some(std::time::Instant::now());
    }
    None
}

/// 纯 Up：@ 提及导航 > hint 导航 > 滚动消息区（textarea 光标/历史已移至 Ctrl+Up）。
fn handle_up(app: &mut App) -> Option<Action> {
    let hint_count = app.hint_candidates_count();
    if app.session_mgr.current_mut().ui.at_mention.active
        && !app.session_mgr.current_mut().ui.loading
    {
        app.session_mgr.current_mut().ui.at_mention.move_up();
        None
    } else if hint_count > 0 && !app.session_mgr.current_mut().ui.loading {
        let cur = app.session_mgr.current_mut().ui.hint_cursor.unwrap_or(0);
        app.session_mgr.current_mut().ui.hint_cursor = if cur == 0 {
            Some(hint_count - 1)
        } else {
            Some(cur - 1)
        };
        None
    } else {
        app.scroll_up();
        Some(Action::Redraw)
    }
}

/// Ctrl+Up：textarea 光标上移，首行时浏览上一条命令历史。
fn handle_ctrl_up(app: &mut App) {
    let (row, _col) = app.session_mgr.current_mut().ui.textarea.cursor();
    if row == 0 {
        app.history_up();
    } else {
        app.session_mgr.current_mut().ui.textarea.input(Input {
            key: Key::Up,
            ctrl: false,
            alt: false,
            shift: false,
        });
    }
}

/// 纯 Down：@ 提及导航 > hint 导航 > 滚动消息区（textarea 光标/历史已移至 Ctrl+Down）。
fn handle_down(app: &mut App) -> Option<Action> {
    let hint_count = app.hint_candidates_count();
    if app.session_mgr.current_mut().ui.at_mention.active
        && !app.session_mgr.current_mut().ui.loading
    {
        app.session_mgr.current_mut().ui.at_mention.move_down();
        None
    } else if hint_count > 0 && !app.session_mgr.current_mut().ui.loading {
        let cur = app
            .session_mgr
            .current_mut()
            .ui
            .hint_cursor
            .unwrap_or(hint_count - 1);
        app.session_mgr.current_mut().ui.hint_cursor = if cur + 1 >= hint_count {
            Some(0)
        } else {
            Some(cur + 1)
        };
        None
    } else {
        app.scroll_down();
        Some(Action::Redraw)
    }
}

/// Ctrl+Down：textarea 光标下移，末行或浏览历史时恢复下一条命令历史。
fn handle_ctrl_down(app: &mut App) {
    if app.session_mgr.current_mut().ui.history_index.is_some() {
        app.history_down();
        return;
    }
    let (row, _col) = app.session_mgr.current_mut().ui.textarea.cursor();
    let last_row = app
        .session_mgr
        .current_mut()
        .ui
        .textarea
        .lines()
        .len()
        .saturating_sub(1);
    if row >= last_row {
        app.history_down();
    } else {
        app.session_mgr.current_mut().ui.textarea.input(Input {
            key: Key::Down,
            ctrl: false,
            alt: false,
            shift: false,
        });
    }
}

/// 若光标正前方（向左方向）是一个完整的 `[Image #N]` 占位符，整体删除占位符 +
/// 从 `pending_attachments` 中移除对应附件，返回 `true`；否则返回 `false` 不拦截。
fn try_delete_image_placeholder_backspace(app: &mut App) -> bool {
    use crate::clipboard::image_placeholder::parse_single_placeholder;

    let (row, col) = app.session_mgr.current().ui.textarea.cursor();
    let lines = app.session_mgr.current().ui.textarea.lines();
    let Some(line) = lines.get(row) else {
        return false;
    };

    // 取光标前的字符序列（按字符，非字节）
    let prefix_chars: Vec<char> = line.chars().take(col).collect();
    if prefix_chars.is_empty() {
        return false;
    }

    // 末尾必须是 ']'
    if prefix_chars.last() != Some(&']') {
        return false;
    }

    // 从 ']' 向前查找最近的 '['，把候选段切出来验证
    let close_idx = prefix_chars.len() - 1;
    let Some(open_idx) = prefix_chars[..close_idx].iter().rposition(|c| *c == '[') else {
        return false;
    };

    let candidate: String = prefix_chars[open_idx..=close_idx].iter().collect();
    let Some((image_id, placeholder_len)) = parse_single_placeholder(&candidate) else {
        return false;
    };

    // 整体删除：先把光标移到占位符开头，再删 placeholder_len 个字符
    let metadata = &mut app.session_mgr.current_mut().metadata;
    let before_len = metadata.pending_attachments.len();
    metadata
        .pending_attachments
        .retain(|a| a.image_id != image_id);
    let attachment_removed = metadata.pending_attachments.len() < before_len;

    let textarea = &mut app.session_mgr.current_mut().ui.textarea;
    textarea.move_cursor(CursorMove::Jump(
        row.try_into().unwrap_or(u16::MAX),
        open_idx.try_into().unwrap_or(u16::MAX),
    ));
    let deleted = textarea.delete_str(placeholder_len);

    if !attachment_removed && !deleted {
        tracing::debug!("Backspace 拦截占位符 image_id={image_id} 但未找到对应附件/未删除文本");
    }
    true
}

fn handle_ctrl_v(app: &mut App) {
    // 优先尝试图片粘贴（file_list / get_image / WSL PowerShell fallback），
    // 失败时再退回文本粘贴。
    match crate::clipboard::paste::paste_image_as_png_base64() {
        Ok((b64, sz, _w, _h)) => {
            app.attach_pasted_image(b64, sz);
        }
        Err(img_err) => {
            tracing::debug!("paste_image_as_png_base64 failed: {img_err}; fallback to text");
            // 不是图片或剪贴板不可用，退回文本粘贴；macOS 下抑制 stderr 污染
            let _guard = crate::clipboard::SuppressStderr::new();
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                if let Ok(text) = clipboard.get_text() {
                    app.paste_text_into_textarea(&text);
                }
            }
        }
    }
}

fn handle_tab(app: &mut App) {
    use super::inject_at_mention_path;

    if app.session_mgr.current_mut().ui.at_mention.active {
        inject_at_mention_path(app);
    } else {
        let count = app.hint_candidates_count();
        if count > 0 {
            match app.session_mgr.current_mut().ui.hint_cursor {
                Some(cur) if cur + 1 < count => {
                    app.session_mgr.current_mut().ui.hint_cursor = Some(cur + 1);
                }
                Some(_) => {
                    app.session_mgr.current_mut().ui.hint_cursor = Some(0);
                }
                None => {
                    app.session_mgr.current_mut().ui.hint_cursor = Some(0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{
        build_textarea, AgentShellRegistration, AgentShellSlot, BackgroundShell, ShellStatus,
    };
    use crate::event::Action;
    use crate::shell_exec::CommandOutput;
    use cc_agent::shell::{ExitSignal, ShellAbortHandle, ShellHandoff};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::oneshot;

    async fn make_app() -> App {
        let (app, _) = App::new_headless(80, 24).await;
        app
    }

    fn make_background_shell(status: ShellStatus) -> BackgroundShell {
        let (_tx, rx) = oneshot::channel::<anyhow::Result<CommandOutput>>();
        let mut shell = BackgroundShell::new(
            "bg-test".to_string(),
            "python wuhan_weather.py".to_string(),
            PathBuf::from("."),
            PathBuf::from("target/peri-test-bg.output"),
            rx,
            ShellAbortHandle::noop(),
            std::time::Instant::now(),
        );
        shell.status = status;
        shell
    }

    fn make_backgrounded_agent_shell() -> AgentShellSlot {
        AgentShellSlot::from_registration(AgentShellRegistration {
            task_id: "agent-bg-test".to_string(),
            owner_session_id: None,
            tool_call_id: None,
            source_agent_id: None,
            execution_timeout_ms: 600_000,
            command: "python wuhan_weather.py".to_string(),
            cwd: ".".to_string(),
            output_path: PathBuf::from("target/peri-test-agent-bg.output"),
            exit_signal: Arc::new(ExitSignal::new()),
            handoff: Arc::new(ShellHandoff::new(true, true)),
            kill: ShellAbortHandle::noop(),
            started_instant: std::time::Instant::now(),
            direct_background: true,
        })
    }

    fn make_foreground_agent_shell() -> (AgentShellSlot, Arc<ShellHandoff>) {
        let handoff = Arc::new(ShellHandoff::new(true, false));
        let slot = AgentShellSlot::from_registration(AgentShellRegistration {
            task_id: "agent-fg-test".to_string(),
            owner_session_id: None,
            tool_call_id: None,
            source_agent_id: None,
            execution_timeout_ms: 600_000,
            command: "python wuhan_weather.py".to_string(),
            cwd: ".".to_string(),
            output_path: PathBuf::from("target/peri-test-agent-fg.output"),
            exit_signal: Arc::new(ExitSignal::new()),
            handoff: handoff.clone(),
            kill: ShellAbortHandle::noop(),
            started_instant: std::time::Instant::now(),
            direct_background: false,
        });
        (slot, handoff)
    }

    #[tokio::test]
    async fn test_ctrl_c_ignores_textarea_content_enters_quit_pending() {
        let mut app = make_app().await;
        app.session_mgr.current_mut().ui.textarea = build_textarea(false);
        app.session_mgr
            .current_mut()
            .ui
            .textarea
            .insert_str("hello world");

        let result = handle_ctrl_c(&mut app);

        assert!(result.is_none(), "第一次 Ctrl+C 不应返回 Quit");
        // 输入框内容不影响 quit-pending
        assert!(
            app.global_ui.quit_pending_since.is_some(),
            "有内容时也应进入 quit-pending"
        );
        // 输入框内容不被清空
        assert!(
            !app.session_mgr.current_mut().ui.textarea.lines()[0].is_empty(),
            "输入框内容不应被清空"
        );
    }

    #[tokio::test]
    async fn test_ctrl_c_interrupts_agent_when_textarea_empty() {
        let mut app = make_app().await;
        app.set_loading(true);

        let result = handle_ctrl_c(&mut app);

        assert!(result.is_none(), "中断 agent 不应返回 Quit");
        assert!(
            app.global_ui.quit_pending_since.is_none(),
            "中断 agent 不应进入 quit-pending"
        );
    }

    #[tokio::test]
    async fn test_ctrl_c_enters_quit_pending_when_idle_and_empty() {
        let mut app = make_app().await;

        let result = handle_ctrl_c(&mut app);

        assert!(result.is_none(), "第一次 Ctrl+C 不应返回 Quit");
        assert!(
            app.global_ui.quit_pending_since.is_some(),
            "空闲时应进入 quit-pending"
        );

        // 等待超过防抖窗口（100ms），模拟真实双击
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let result = handle_ctrl_c(&mut app);
        assert!(
            matches!(result, Some(Action::Quit)),
            "2 秒内第二次 Ctrl+C 应返回 Quit"
        );
    }

    #[tokio::test]
    async fn test_ctrl_c_quits_even_when_textarea_has_content() {
        let mut app = make_app().await;
        let _ = handle_ctrl_c(&mut app);
        assert!(app.global_ui.quit_pending_since.is_some());

        // 等待超过防抖窗口（100ms），模拟真实双击
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        // 输入框有内容，第二次 Ctrl+C 仍应退出
        app.session_mgr
            .current_mut()
            .ui
            .textarea
            .insert_str("some text");
        let result = handle_ctrl_c(&mut app);

        assert!(
            matches!(result, Some(Action::Quit)),
            "有内容时第二次 Ctrl+C 应退出"
        );
    }

    #[tokio::test]
    async fn test_enter_shell_command_returns_run_shell_action() {
        let mut app = make_app().await;
        app.session_mgr
            .current_mut()
            .ui
            .textarea
            .insert_str("!git status");

        let result = handle_normal_keys(
            &mut app,
            Input {
                key: Key::Enter,
                ctrl: false,
                alt: false,
                shift: false,
            },
        )
        .unwrap();

        assert!(
            matches!(result, Some(Action::RunShellCommand(cmd)) if cmd == "git status"),
            "! 前缀输入应剥离前缀后返回 RunShellCommand"
        );
    }

    #[tokio::test]
    async fn test_enter_shell_command_trims_prefix_and_spaces() {
        let mut app = make_app().await;
        app.session_mgr
            .current_mut()
            .ui
            .textarea
            .insert_str("!  cargo build  ");

        let result = handle_normal_keys(
            &mut app,
            Input {
                key: Key::Enter,
                ctrl: false,
                alt: false,
                shift: false,
            },
        )
        .unwrap();

        assert!(
            matches!(result, Some(Action::RunShellCommand(cmd)) if cmd == "cargo build"),
            "shell 命令应去掉 ! 前缀和首尾空格"
        );
    }

    #[tokio::test]
    async fn test_down_completed_background_shell_does_not_focus_task_bar() {
        let mut app = make_app().await;
        app.session_mgr
            .current_mut()
            .background_shells
            .push(make_background_shell(ShellStatus::Completed));

        let result = handle_normal_keys(
            &mut app,
            Input {
                key: Key::Down,
                ctrl: false,
                alt: false,
                shift: false,
            },
        )
        .unwrap();

        assert!(
            matches!(result, Some(Action::Redraw)),
            "完成的后台 shell 不应拦截 Down，应继续走普通滚动"
        );
        assert!(
            !app.session_mgr.current().ui.background_tasks_bar_focused,
            "完成的后台 shell 不应让后台任务入口聚焦"
        );
    }

    #[tokio::test]
    async fn test_down_running_background_shell_focuses_task_bar() {
        let mut app = make_app().await;
        app.session_mgr
            .current_mut()
            .background_shells
            .push(make_background_shell(ShellStatus::Running));

        let result = handle_normal_keys(
            &mut app,
            Input {
                key: Key::Down,
                ctrl: false,
                alt: false,
                shift: false,
            },
        )
        .unwrap();

        assert!(matches!(result, Some(Action::Redraw)));
        assert!(
            app.session_mgr.current().ui.background_tasks_bar_focused,
            "运行中的后台 shell 应让 Down 聚焦 status bar 入口"
        );
    }

    #[tokio::test]
    async fn test_down_backgrounded_agent_shell_focuses_task_bar() {
        let mut app = make_app().await;
        app.session_mgr
            .current_mut()
            .agent_shells
            .push(make_backgrounded_agent_shell());

        let result = handle_normal_keys(
            &mut app,
            Input {
                key: Key::Down,
                ctrl: false,
                alt: false,
                shift: false,
            },
        )
        .unwrap();

        assert!(matches!(result, Some(Action::Redraw)));
        assert!(
            app.session_mgr.current().ui.background_tasks_bar_focused,
            "后台化的 agent Bash 也应让 Down 聚焦 status bar 入口"
        );
    }

    #[tokio::test]
    async fn test_enter_focused_background_task_bar_opens_panel() {
        use crate::app::panel_manager::PanelKind;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.session_mgr
            .current_mut()
            .background_shells
            .push(make_background_shell(ShellStatus::Running));
        app.session_mgr
            .current_mut()
            .ui
            .background_tasks_bar_focused = true;

        let result = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        )
        .unwrap();

        assert!(matches!(result, Some(Action::Redraw)));
        assert!(
            app.global_panels.is_active(PanelKind::BackgroundTasks),
            "后台任务入口聚焦后 Enter 应打开 BackgroundTasksPanel"
        );
        assert!(
            !app.session_mgr.current().ui.background_tasks_bar_focused,
            "打开面板后应退出 status bar 入口聚焦"
        );
    }

    #[tokio::test]
    async fn test_ctrl_b_backgrounded_agent_shell_focuses_task_bar() {
        use crate::app::panel_manager::PanelKind;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.session_mgr
            .current_mut()
            .agent_shells
            .push(make_backgrounded_agent_shell());

        let result = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
        )
        .unwrap();

        assert!(matches!(result, Some(Action::Redraw)));
        assert!(
            !app.global_panels.is_active(PanelKind::BackgroundTasks),
            "后台化的 agent Bash 存在时 Ctrl+B 不应直接打开 BackgroundTasksPanel"
        );
        assert!(
            app.session_mgr.current().ui.background_tasks_bar_focused,
            "后台化的 agent Bash 存在时 Ctrl+B 应先聚焦底部 shell 入口"
        );

        let enter = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        )
        .unwrap();

        assert!(matches!(enter, Some(Action::Redraw)));
        assert!(
            app.global_panels.is_active(PanelKind::BackgroundTasks),
            "底部 shell 入口聚焦后 Enter 应打开 BackgroundTasksPanel"
        );
    }

    #[tokio::test]
    async fn test_ctrl_b_foreground_agent_shell_backgrounds_and_focuses_task_bar() {
        use crate::app::panel_manager::PanelKind;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        let (slot, handoff) = make_foreground_agent_shell();
        app.session_mgr.current_mut().agent_shells.push(slot);

        let result = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
        )
        .unwrap();

        assert!(matches!(result, Some(Action::Redraw)));
        assert!(handoff.is_backgrounded(), "后台化必须已提交到共享归属状态");
        assert!(
            app.session_mgr.current().agent_shells[0].is_backgrounded(),
            "Ctrl+B 应先把前台 agent Bash 切到后台"
        );
        assert!(
            !app.global_panels.is_active(PanelKind::BackgroundTasks),
            "前台 agent Bash 后台化时不应直接打开 BackgroundTasksPanel"
        );
        assert!(
            app.session_mgr.current().ui.background_tasks_bar_focused,
            "前台 agent Bash 后台化后应聚焦底部 shell 入口"
        );
    }

    #[tokio::test]
    async fn test_esc_closes_background_tasks_panel_from_detail() {
        use crate::app::panel_manager::PanelKind;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.session_mgr
            .current_mut()
            .background_shells
            .push(make_background_shell(ShellStatus::Running));
        app.open_background_tasks_panel();

        let enter = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        )
        .unwrap();
        assert!(matches!(enter, Some(Action::Redraw)));
        assert!(app.global_panels.is_active(PanelKind::BackgroundTasks));
        app.session_mgr
            .current_mut()
            .ui
            .background_tasks_bar_focused = true;

        let esc = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        )
        .unwrap();

        assert!(matches!(esc, Some(Action::Redraw)));
        assert!(
            !app.global_panels.is_any_open(),
            "BackgroundTasksPanel 详情页按 Esc 应真实关闭面板并释放输入区域"
        );
        assert!(
            !app.session_mgr.current().ui.background_tasks_bar_focused,
            "BackgroundTasksPanel 关闭后不应保留 status bar 入口聚焦态"
        );
    }

    #[tokio::test]
    async fn test_alt_x_deletes_head_queued_message() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.session_mgr.current_mut().messages.pending_messages =
            vec!["第一条".to_string().into(), "第二条".to_string().into()];

        let result = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT),
        )
        .unwrap();

        assert!(matches!(result, Some(Action::Redraw)));
        let messages = &app.session_mgr.current().messages.pending_messages;
        assert_eq!(messages.len(), 1, "Alt+X 一次只删除队首一条");
        assert_eq!(messages[0].text, "第二条");
        assert_eq!(
            app.session_mgr.current().ui.textarea.lines().join(""),
            "",
            "队首删除按键不能作为字面字符写入输入框"
        );
    }

    #[tokio::test]
    async fn test_ctrl_x_deletes_head_queued_message() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.session_mgr.current_mut().messages.pending_messages =
            vec!["第一条".to_string().into(), "第二条".to_string().into()];

        let result = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
        )
        .unwrap();

        assert!(matches!(result, Some(Action::Redraw)));
        let messages = &app.session_mgr.current().messages.pending_messages;
        assert_eq!(messages.len(), 1, "Ctrl+X 一次只删除队首一条");
        assert_eq!(messages[0].text, "第二条");
        assert_eq!(
            app.session_mgr.current().ui.textarea.lines().join(""),
            "",
            "队首删除按键不能作为字面字符写入输入框"
        );
    }

    #[tokio::test]
    async fn test_ctrl_enter_is_consumed_by_queued_message_shortcut() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.session_mgr.current_mut().ui.loading = true;
        app.session_mgr
            .current_mut()
            .messages
            .pending_messages
            .push("补充信息".to_string().into());

        let result = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
        )
        .unwrap();

        assert!(matches!(result, Some(Action::Redraw)));
        assert_eq!(
            app.session_mgr.current().ui.textarea.lines().join(""),
            "",
            "队首发送按键不能作为字面字符写入输入框"
        );
        assert_eq!(
            app.session_mgr.current().messages.pending_messages.len(),
            1,
            "无 ACP 客户端时消息必须留在队列中"
        );
    }

    #[tokio::test]
    async fn test_alt_s_is_consumed_by_queued_message_shortcut() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.session_mgr.current_mut().ui.loading = true;
        app.session_mgr
            .current_mut()
            .messages
            .pending_messages
            .push("补充信息".to_string().into());

        let result = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT),
        )
        .unwrap();

        assert!(matches!(result, Some(Action::Redraw)));
        assert_eq!(
            app.session_mgr.current().ui.textarea.lines().join(""),
            "",
            "队首发送按键不能作为字面字符写入输入框"
        );
        assert_eq!(
            app.session_mgr.current().messages.pending_messages.len(),
            1,
            "无 ACP 客户端时消息必须留在队列中"
        );
    }

    #[tokio::test]
    async fn test_macos_option_compose_chars_trigger_queue_shortcuts() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.session_mgr.current_mut().messages.pending_messages =
            vec!["第一条".to_string().into(), "第二条".to_string().into()];
        // ⌥X 组合出 '≈'，无修饰符
        let _ = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('≈'), KeyModifiers::NONE),
        )
        .unwrap();
        assert_eq!(
            app.session_mgr.current().messages.pending_messages.len(),
            1,
            "macOS Option+X 组合字符 '≈' 应等价 Alt+X 删除队首"
        );
        assert_eq!(
            app.session_mgr.current().ui.textarea.lines().join(""),
            "",
            "组合字符不应被当作字面值写入输入框"
        );

        // ⌥S 组合出 'ß'，无修饰符
        let _ = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('ß'), KeyModifiers::NONE),
        )
        .unwrap();
        assert_eq!(
            app.session_mgr.current().ui.textarea.lines().join(""),
            "",
            "组合字符 'ß' 不应被当作字面值写入输入框"
        );
    }

    #[tokio::test]
    async fn test_left_returns_background_tasks_panel_to_list_from_detail() {
        use crate::app::panel_manager::PanelKind;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.session_mgr
            .current_mut()
            .background_shells
            .push(make_background_shell(ShellStatus::Running));
        app.open_background_tasks_panel();

        let enter = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        )
        .unwrap();
        assert!(matches!(enter, Some(Action::Redraw)));
        assert!(app.global_panels.is_active(PanelKind::BackgroundTasks));
        app.session_mgr
            .current_mut()
            .ui
            .background_tasks_bar_focused = true;

        let left = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
        )
        .unwrap();

        assert!(matches!(left, Some(Action::Redraw)));
        assert!(
            app.global_panels.is_active(PanelKind::BackgroundTasks),
            "BackgroundTasksPanel 详情页按 ← 应返回列表，不应真实关闭面板"
        );
        let hints = app.global_panels.status_bar_hints(&app.services.lc);
        // 断言回到「列表视图」：列表视图含 Enter/详情 提示，详情视图含 ←/返回。
        // 不硬编码文案 —— 走 lc 解析，避免语言环境不同导致断言失败。
        assert!(
            hints.contains(&("Enter".to_string(), app.services.lc.tr("key-detail"))),
            "BackgroundTasksPanel 详情页按 ← 应回到列表视图，实际 hints: {hints:?}"
        );
    }

    #[tokio::test]
    async fn test_left_closes_background_tasks_panel_from_list() {
        use crate::app::panel_manager::PanelKind;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.session_mgr
            .current_mut()
            .background_shells
            .push(make_background_shell(ShellStatus::Running));
        app.open_background_tasks_panel();
        assert!(app.global_panels.is_active(PanelKind::BackgroundTasks));
        app.session_mgr
            .current_mut()
            .ui
            .background_tasks_bar_focused = true;

        let left = crate::event::keyboard::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
        )
        .unwrap();

        assert!(matches!(left, Some(Action::Redraw)));
        assert!(
            !app.global_panels.is_any_open(),
            "BackgroundTasksPanel 列表页按 ← 没有上一步，应真实关闭面板"
        );
        assert!(
            !app.session_mgr.current().ui.background_tasks_bar_focused,
            "BackgroundTasksPanel 关闭后不应保留 status bar 入口聚焦态"
        );
    }
}
