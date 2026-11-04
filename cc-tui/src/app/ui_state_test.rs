use super::*;
use std::time::Duration;

fn make_ui() -> UiState {
    UiState::new(TextArea::default(), "", false, false, false)
}

#[test]
fn test_session_ui_state_toast_active_and_expiry() {
    let mut ui = make_ui();
    let lc = crate::i18n::LcRegistry::new(None);

    assert!(
        ui.active_toast(&lc).is_none(),
        "初始状态下没有 active toast"
    );

    ui.set_toast("Model switched to: haiku", Duration::from_millis(500));
    assert_eq!(
        ui.active_toast(&lc).as_deref(),
        Some("Model switched to: haiku"),
        "未过期时应返回设置的 toast 文本"
    );

    // 人工将过期时间设为过去
    if let Some((_, ref mut until)) = ui.toast {
        *until = std::time::Instant::now() - Duration::from_millis(10);
    }
    assert!(
        ui.active_toast(&lc).is_none(),
        "过期后 active_toast 应返回 None"
    );
}

#[test]
fn test_session_ui_state_toast_fallback_to_copy_message() {
    let mut ui = make_ui();
    let lc = crate::i18n::LcRegistry::new(None);

    ui.copy_char_count = 13;
    ui.copy_message_until = Some(std::time::Instant::now() + Duration::from_millis(500));

    let toast = ui.active_toast(&lc);
    assert!(toast.is_some(), "无显式 toast 时应回退到复制提示");
    assert!(toast.unwrap().contains("13"), "复制提示中应包含字符数 13");
}

#[test]
fn test_session_ui_state_toast_precedence_over_copy_message() {
    let mut ui = make_ui();
    let lc = crate::i18n::LcRegistry::new(None);

    ui.copy_char_count = 13;
    ui.copy_message_until = Some(std::time::Instant::now() + Duration::from_millis(500));
    ui.set_toast("Model switched to: fable", Duration::from_millis(500));

    assert_eq!(
        ui.active_toast(&lc).as_deref(),
        Some("Model switched to: fable"),
        "显式 toast 应优先于 copy_message 提示"
    );
}
