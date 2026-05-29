use tui_textarea::{Input, Key};

use crate::app::{App, MessageViewModel, PendingAttachment};

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

        // ESC: no longer quits main window; only clears buffer while loading
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

        // Esc: 关闭 @ 提及弹窗
        Input { key: Key::Esc, .. }
            if app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .at_mention
                .active =>
        {
            app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .at_mention
                .close();
        }

        // Up: @ 提及导航 > hint navigation > history browse (only first row) > textarea cursor
        Input { key: Key::Up, .. } => handle_up(app),

        // Down: @ 提及导航 > hint navigation > history restore (only last row) > textarea cursor
        Input { key: Key::Down, .. } => handle_down(app),

        // Ctrl+V: try pasting clipboard image first, fallback to text paste
        Input {
            key: Key::Char('v'),
            ctrl: true,
            ..
        } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading => handle_ctrl_v(app),

        // Tab: @ 提及补全 > hint overlay candidate navigation and completion
        Input {
            key: Key::Tab,
            shift: false,
            ..
        } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading => handle_tab(app),

        // Enter with @ mention active and candidates: inject selected path
        Input {
            key: Key::Enter, ..
        } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading
            && app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .at_mention
                .active
            && !app.session_mgr.sessions[app.session_mgr.active]
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

        // Shift+Enter / Alt+Enter: insert newline (Shift works everywhere; Alt (Option) for macOS)
        Input {
            key: Key::Enter, ..
        } if input.shift || input.alt => {
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

        // Enter: submit (non-loading) or buffer (loading)
        Input {
            key: Key::Enter, ..
        } => {
            // 关闭可能残留的 @ mention 弹窗
            if app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .at_mention
                .active
            {
                app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .at_mention
                    .close();
            }
            let text = app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .textarea
                .lines()
                .join("\n");
            let text = text.trim().to_string();
            if !text.is_empty() {
                if app.session_mgr.sessions[app.session_mgr.active].ui.loading {
                    // Loading state: buffer message
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
                    // SAFETY: command_registry is nested inside App; dispatch needs &mut App
                    // NOTE: session index must be saved before take because dispatch
                    // (e.g. /split) may change app.session_mgr.active
                    let session_idx = app.session_mgr.active;
                    let registry = std::mem::take(
                        &mut app.session_mgr.sessions[session_idx]
                            .commands
                            .command_registry,
                    );
                    let known = registry.dispatch(app, &text);
                    app.session_mgr.sessions[session_idx]
                        .commands
                        .command_registry = registry;
                    if known {
                        // Command matched, done
                    } else {
                        // Command not matched, try Skill matching
                        let skill_name: String = text
                            .trim_start_matches('/')
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                            .collect();
                        if let Some(_skill) = app.session_mgr.sessions[app.session_mgr.active]
                            .commands
                            .skills
                            .iter()
                            .find(|s| s.name == skill_name)
                        {
                            // Skill matched: submit full message to agent
                            return Ok(Some(Action::Submit(text)));
                        } else if app.session_mgr.sessions[app.session_mgr.active]
                            .commands
                            .agent_commands
                            .contains(&skill_name)
                        {
                            // Agent command matched (from ACP AvailableCommandsUpdate): submit to agent
                            tracing::info!(skill_name, "Matched agent command, submitting to ACP");
                            return Ok(Some(Action::Submit(text)));
                        } else {
                            tracing::info!(
                                skill_name,
                                agent_commands = ?app.session_mgr.sessions[app.session_mgr.active]
                                    .commands
                                    .agent_commands,
                                "Command not found in local registry, skills, or agent_commands"
                            );
                            // Distinguish "prefix ambiguity" from "completely unknown"
                            let prefix = text.trim_start_matches('/').to_string();
                            let cmd_matches = app.session_mgr.sessions[app.session_mgr.active]
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

        // VS Code terminal maps Option+Backspace to PageUp; perform word-delete when textarea has content
        Input {
            key: Key::PageUp, ..
        } if std::env::var("TERM_PROGRAM").as_deref() == Ok("vscode") => {
            let session = &mut app.session_mgr.sessions[app.session_mgr.active];
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

        // Ctrl+U / Ctrl+D: half-page scroll
        Input {
            key: Key::Char('u'),
            ctrl: true,
            ..
        } => {
            let session = &app.session_mgr.sessions[app.session_mgr.active];
            let has_content = session
                .ui
                .textarea
                .lines()
                .iter()
                .any(|line| !line.is_empty());
            if has_content {
                app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea
                    .delete_line_by_head();
            } else {
                for _ in 0..20 {
                    app.scroll_up();
                }
            }
        }
        Input {
            key: Key::Char('d'),
            ctrl: true,
            ..
        } => {
            for _ in 0..20 {
                app.scroll_down();
            }
        }

        // Del: remove last pending attachment
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

        // Ctrl+N/P: cycle session focus
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

        // Ctrl+W: close current session
        input @ Input {
            key: Key::Char('w'),
            ctrl: true,
            ..
        } => {
            if app.close_session().is_some() {
                // Session closed, stop processing
            } else {
                // Only one session, fallback to textarea
                app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea
                    .input(input);
            }
        }

        // Intercept plain Enter to avoid textarea default newline; allow input during loading
        input if input.key != Key::Enter => {
            // Exit history browsing
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
            // When input changes: reset cursor (don't pre-select; wait for user to press Tab/Up/Down)
            if !app.session_mgr.sessions[app.session_mgr.active].ui.loading {
                app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .hint_cursor = None;
                update_at_mention_detection(app);
            }
        }

        _ => {
            // Any other key cancels quit-pending state
            app.global_ui.quit_pending_since = None;
        }
    }

    Ok(Some(Action::Redraw))
}

// ── Per-arm helper functions ──────────────────────────────────────────────

fn handle_ctrl_c(app: &mut App) -> Option<Action> {
    if app.session_mgr.sessions[app.session_mgr.active].ui.loading {
        // Agent running: interrupt first, clear quit-pending state
        app.interrupt();
        app.global_ui.quit_pending_since = None;
    } else if let Some(since) = app.global_ui.quit_pending_since {
        // Not loading, second Ctrl+C within 2s → quit
        if since.elapsed() < std::time::Duration::from_secs(2) {
            return Some(Action::Quit);
        } else {
            // Timeout expired, restart timer
            app.global_ui.quit_pending_since = Some(std::time::Instant::now());
        }
    } else {
        // First Ctrl+C, enter quit-pending state
        app.global_ui.quit_pending_since = Some(std::time::Instant::now());
    }
    None
}

fn handle_up(app: &mut App) {
    let hint_count = app.hint_candidates_count();
    if app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .at_mention
        .active
        && !app.session_mgr.sessions[app.session_mgr.active].ui.loading
    {
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .at_mention
            .move_up();
    } else if hint_count > 0 && !app.session_mgr.sessions[app.session_mgr.active].ui.loading {
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

fn handle_down(app: &mut App) {
    let hint_count = app.hint_candidates_count();
    if app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .at_mention
        .active
        && !app.session_mgr.sessions[app.session_mgr.active].ui.loading
    {
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .at_mention
            .move_down();
    } else if hint_count > 0 && !app.session_mgr.sessions[app.session_mgr.active].ui.loading {
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

fn handle_ctrl_v(app: &mut App) {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if let Ok(img) = clipboard.get_image() {
            let (w, h) = (img.width as u32, img.height as u32);
            if let Ok((b64, sz)) = super::super::mouse::rgba_to_png_base64(w, h, &img.bytes) {
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

fn handle_tab(app: &mut App) {
    use super::inject_at_mention_path;

    if app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .at_mention
        .active
    {
        inject_at_mention_path(app);
    } else {
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
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .hint_cursor = Some(0);
                }
                None => {
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .hint_cursor = Some(0);
                }
            }
        }
    }
}
