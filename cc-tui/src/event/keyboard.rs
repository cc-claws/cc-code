use anyhow::Result;
use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use tui_textarea::Input;

use crate::app::App;

use super::Action;

// ── Submodule declarations ─────────────────────────────────────────────────
mod bar_focus;
mod normal_keys;
mod panels;
mod popups;
mod setup_wizard;
mod shortcuts;

// ---------------------------------------------------------------------------
// macOS key-binding compatibility layer
// ---------------------------------------------------------------------------
// On macOS, the Option (Alt) key acts as a character compose modifier.
// Terminals emit a composed Unicode character *without* any modifier flags.
// We maintain a central mapping table so each shortcut only needs to be
// defined once, keeping the macOS workaround auditable.
// ---------------------------------------------------------------------------

/// A cross-platform key-binding definition that accounts for macOS Option-key
/// character composition.
pub(super) struct KeyBinding {
    /// Human-readable label (kept for debugging / future status-bar use).
    #[allow(dead_code)]
    label: &'static str,
    /// Character produced on macOS when Option (+ optional Shift) is held.
    macos_char: Option<char>,
    /// Required modifiers on non-macOS terminals (Linux/Windows).
    modifiers: KeyModifiers,
    /// The primary key code (ignoring macOS compose).
    key: KeyCode,
}

impl KeyBinding {
    /// Returns `true` if `key_event` matches this binding on *any* platform.
    pub(super) fn matches(&self, key_event: &ratatui::crossterm::event::KeyEvent) -> bool {
        // macOS path: terminal emits a composed char with no modifiers.
        if let Some(ch) = self.macos_char {
            if matches!(key_event.code, KeyCode::Char(c) if c == ch) {
                return true;
            }
        }
        // Standard path: check modifiers + key code.
        let mods_ok = key_event.modifiers.contains(self.resolved_modifiers());
        let key_ok = match (&self.key, &key_event.code) {
            (KeyCode::Char(a), KeyCode::Char(b)) => a.eq_ignore_ascii_case(b),
            (a, b) => a == b,
        };
        mods_ok && key_ok
    }

    /// Resolve the actual modifiers needed. bitflags `|` is not const,
    /// so multi-flag bindings store `KeyModifiers::empty()` and reconstruct here.
    fn resolved_modifiers(&self) -> KeyModifiers {
        self.modifiers
    }
}

/// Central shortcut registry.  Add new shortcuts here — the `matches()` call
/// in each handler block is the only site that needs updating.
pub(super) static SHORTCUT_BG_BAR: KeyBinding = KeyBinding {
    label: "Ctrl+B",
    macos_char: None,
    modifiers: KeyModifiers::CONTROL,
    key: KeyCode::Char('b'),
};

pub(super) static SHORTCUT_COMMAND_PALETTE: KeyBinding = KeyBinding {
    label: "Ctrl+P",
    macos_char: None,
    modifiers: KeyModifiers::CONTROL,
    key: KeyCode::Char('p'),
};

pub(super) static SHORTCUT_COMMAND_PALETTE_ALT: KeyBinding = KeyBinding {
    label: "Alt+P",
    macos_char: Some('π'),
    modifiers: KeyModifiers::ALT,
    key: KeyCode::Char('p'),
};

/// Returns the label for the command palette shortcut.
pub fn command_palette_label() -> &'static str {
    "Ctrl+P / Alt+P"
}

/// Handles a single key event, dispatching to panels, prompts, textarea, or
/// application-level shortcuts. Returns an `Action` when a redraw or quit is
/// needed.
pub fn handle_key_event(
    app: &mut App,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> Result<Option<Action>> {
    // Only process Press events; ignore Release (prevents double-fires)
    if key_event.kind == KeyEventKind::Release {
        return Ok(Some(Action::Redraw));
    }

    // 按住 Ctrl+C 产生的键盘重复事件不计为新的按键：一次物理按下只触发一次
    // handle_ctrl_c，否则长按会被误判为"双击退出"直接退出 TUI。
    // （Release 已在上面过滤；100ms 防抖只覆盖 ConPTY 0-1ms 的重复下发，
    // 挡不住真正的按键重复。2026-10-01 修）
    if key_event.kind == KeyEventKind::Repeat
        && key_event.code == KeyCode::Char('c')
        && key_event.modifiers.contains(KeyModifiers::CONTROL)
    {
        return Ok(Some(Action::Redraw));
    }

    // Stage 1-2: Bar focus / focused-only mode
    if let Some(action) = bar_focus::handle_bar_focus(app, &key_event) {
        return Ok(Some(action));
    }
    if let Some(action) = bar_focus::handle_focused_only(app, &key_event) {
        return Ok(Some(action));
    }

    // Stage 3-6: Shortcuts (BackTab, Ctrl+B, Ctrl+P, Alt+P, Ctrl+O)
    if let Some(action) = shortcuts::handle_shortcuts(app, &key_event) {
        return Ok(Some(action));
    }

    let input = Input::from(key_event);

    // Stage 7: Setup wizard
    if let Some(action) = setup_wizard::handle_setup_wizard(app, &input) {
        return Ok(Some(action));
    }

    // Stage 8-9: Panels
    if let Some(action) = panels::handle_panels(app, &input) {
        return Ok(Some(action));
    }

    // Stage 10-12: Popups (OAuth > AskUser > HITL)
    if let Some(action) = popups::handle_popups(app, &input) {
        return Ok(Some(action));
    }

    // Stage 13: Normal key handling (main match block)
    normal_keys::handle_normal_keys(app, input)
}

/// 检测 textarea 中 @ 提及模式，更新状态并触发异步搜索
/// 缓存命中时立即更新，否则 spawn 后台任务避免阻塞 UI 线程
pub(super) fn update_at_mention_detection(app: &mut App) {
    let textarea = &app.session_mgr.current_mut().ui.textarea;
    let text = textarea.lines().join("\n");
    let (row, col) = textarea.cursor();
    // 将 (row, col) 转为字节偏移
    let mut pos = 0usize;
    for (i, line) in textarea.lines().iter().enumerate() {
        if i == row {
            pos += line.chars().take(col).map(|c| c.len_utf8()).sum::<usize>();
            break;
        }
        pos += line.len() + 1; // +1 for \n
    }

    let at = &mut app.session_mgr.current_mut().ui.at_mention;

    at.ensure_cwd(app.services.cwd.clone());

    if let Some((query, start)) = crate::app::AtMentionState::detect(&text, pos) {
        if at.active && at.query == query {
            return; // 未变化
        }
        at.activate(query.clone(), start);

        // 尝试从缓存获取结果（零 IO，立即更新）
        if let Some(cached) = at.try_filter_from_cache(&query) {
            at.update_candidates(cached);
            return;
        }

        // 节流：距离上次搜索不到 200ms 时，保留旧结果不搜索
        if !at.should_search_now() && !at.candidates.is_empty() {
            return;
        }

        // 搜索线程处理，不阻塞 UI
        at.start_search(query);
    } else if at.active {
        at.close();
    }
}

/// 将选中的 @ 提及路径注入 textarea
pub(super) fn inject_at_mention_path(app: &mut App) {
    let at = &app.session_mgr.current_mut().ui.at_mention;
    let candidate = match at.selected_candidate() {
        Some(c) => c.clone(),
        None => return,
    };
    let query_start = at.query_start;
    let query_len = at.query.len();

    let textarea = &app.session_mgr.current_mut().ui.textarea;
    let full_text: String = textarea.lines().join("\n");

    let needs_quotes = candidate.path.contains(' ');
    let replacement = if needs_quotes {
        format!("@\"{}\"", candidate.path)
    } else {
        format!("@{}", candidate.path)
    };

    // 替换从 query_start 到 query_start + 1(@) + query_len
    let mut new_text = String::with_capacity(full_text.len() + replacement.len());
    new_text.push_str(&full_text[..query_start]);
    new_text.push_str(&replacement);
    let after_end = query_start + 1 + query_len;
    if after_end < full_text.len() {
        new_text.push_str(&full_text[after_end..]);
    }

    let is_dir = candidate.is_dir;

    let mut new_ta = crate::app::build_textarea(false);
    new_ta.insert_str(&new_text);
    app.session_mgr.current_mut().ui.textarea = new_ta;

    if is_dir {
        app.session_mgr.current_mut().ui.textarea.insert_str("/");
        update_at_mention_detection(app);
    } else {
        app.session_mgr.current_mut().ui.textarea.insert_str(" ");
        app.session_mgr.current_mut().ui.at_mention.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::panel_manager::PanelKind;
    use ratatui::crossterm::event::{KeyCode, KeyEvent};

    #[tokio::test]
    async fn test_ctrl_c_closes_panel_first_then_double_tap_quits() {
        // 端到端验证新链路：ModelPanel 打开时第一次 Ctrl+C 只关面板
        // （不退出、不进入 quit-pending）；面板关闭后，双击 Ctrl+C 才真正退出。
        // 覆盖 handle_key_event 全 Stage 分发。
        // （2026-10-01 改：此前面板开着时双击/长按 Ctrl+C 直接退出整个 TUI）
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.open_model_panel();
        assert!(
            app.session_mgr
                .current()
                .session_panels
                .is_active(PanelKind::Model),
            "前置条件：ModelPanel 应已打开"
        );

        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);

        // 第一次 Ctrl+C（面板开着）→ 只关面板，不 Quit，不 arm
        let r1 = handle_key_event(&mut app, ctrl_c).unwrap();
        assert!(
            !app.session_mgr.current().session_panels.is_any_open(),
            "第一次 Ctrl+C 应关闭面板"
        );
        assert!(!matches!(r1, Some(Action::Quit)), "关面板时不应 Quit");
        assert!(
            app.global_ui.quit_pending_since.is_none(),
            "关面板这次不应计入双击退出"
        );

        // 面板已关：再按一次 → 进入 quit-pending，不退出
        let r2 = handle_key_event(&mut app, ctrl_c).unwrap();
        assert!(
            app.global_ui.quit_pending_since.is_some(),
            "面板关闭后 Ctrl+C 应进入 quit-pending"
        );
        assert!(!matches!(r2, Some(Action::Quit)), "第一次不应 Quit");

        // 等待超过防抖窗口（100ms），模拟真实双击
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        // 2 秒内第二次 → 真正退出
        let r3 = handle_key_event(&mut app, ctrl_c).unwrap();
        assert!(
            matches!(r3, Some(Action::Quit)),
            "双击 Ctrl+C 应返回 Quit"
        );
    }

    #[tokio::test]
    async fn test_ctrl_c_repeat_kind_does_not_arm_or_quit() {
        // 按住 Ctrl+C 的键盘重复事件不应被计为新的按键：不 arm、不退出。
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        let repeat = KeyEvent {
            kind: KeyEventKind::Repeat,
            ..KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
        };
        let r = handle_key_event(&mut app, repeat).unwrap();
        assert!(!matches!(r, Some(Action::Quit)), "Repeat 不应 Quit");
        assert!(
            app.global_ui.quit_pending_since.is_none(),
            "Repeat 不应进入 quit-pending"
        );
    }

    #[tokio::test]
    async fn test_alt_v_intercepted_by_paste_handler() {
        // 验证 Alt+V / Option+V 不会被 textarea 作为普通字面值字符输入，
        // 而是被粘贴处理逻辑拦截（即便剪贴板无内容也不会把 '√' 等字面值打入 textarea）。
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;

        for key in [
            KeyEvent::new(KeyCode::Char('v'), KeyModifiers::ALT),
            KeyEvent::new(KeyCode::Char('V'), KeyModifiers::ALT),
            KeyEvent::new(KeyCode::Char('√'), KeyModifiers::NONE),
        ] {
            let res = handle_key_event(&mut app, key).unwrap();
            assert!(matches!(res, Some(Action::Redraw)));
            let text = app.session_mgr.current().ui.textarea.lines().join("");
            assert!(
                !text.contains('√'),
                "按键 {:?} 应被粘贴逻辑拦截，不应把字面字符 '√' 打入 textarea，实际为: {text}",
                key
            );
        }
    }
}
