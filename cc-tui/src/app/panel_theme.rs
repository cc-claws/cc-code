use super::*;
use crate::app::message_pipeline::MessagePipeline;

/// 应用主题的完整流程：切换 / 持久化 / 推送提示 / 重建视图 / 触发重绘。
///
/// `/theme <name>` 命令（[`App::apply_theme`]）与 `/theme` 面板 Enter
/// （`ThemePanel::apply_and_close`）共用这一条路径——之前两处各写一遍，面板
/// 路径漏了重建与重绘，导致「提示不显示、已渲染 markdown 也不换色」。
///
/// 重建范围：
/// - 非流式：从规范消息全量重建 ViewModel，让 `rendered` 快照换成新配色
/// - 流式中：保留现有视图只追加提示（流式内容每帧都会用新主题重渲染）
pub(crate) fn apply_theme_impl(
    services: &mut ServiceRegistry,
    session_mgr: &mut SessionManager,
    name: crate::ui::theme::ThemeName,
) {
    crate::ui::theme::set_current(name);

    let save_result = {
        let cfg = services
            .peri_config
            .get_or_insert_with(PeriConfig::default);
        cfg.config.theme = Some(name.setting_value().to_string());
        App::save_config(cfg, services.config_path_override.as_deref())
    };

    let streaming = {
        let p = &session_mgr.current().messages.pipeline;
        p.has_streaming_content() || p.has_pending_tool_calls()
    };
    let mut view_msgs = if streaming {
        session_mgr.current().messages.view_messages.clone()
    } else {
        let messages = session_mgr
            .current()
            .messages
            .pipeline
            .completed_messages()
            .to_vec();
        let cwd = services.cwd.clone();
        MessagePipeline::messages_to_view_models(&messages, &cwd)
    };

    let note = services.lc.tr_args(
        "app-theme-set",
        &[("name".into(), name.setting_value().into())],
    );
    view_msgs.push(MessageViewModel::system(note));
    if let Err(e) = save_result {
        let note = services.lc.tr_args(
            "app-config-save-failed",
            &[("error".into(), e.to_string().into())],
        );
        view_msgs.push(MessageViewModel::system(note));
    }

    let session = session_mgr.current_mut();
    session.messages.round_start_vm_idx = 0;
    session.messages.view_messages = view_msgs.clone();
    // 与 `App::render_rebuild` 同口径：直接投递整屏重建，提示立即出现
    let _ = session
        .messages
        .render_tx
        .try_send(crate::ui::render_thread::RenderEvent::Rebuild(view_msgs));
}

impl App {
    /// 打开 /theme 面板
    pub fn open_theme_panel(&mut self) {
        self.services
            .peri_config
            .get_or_insert_with(PeriConfig::default);
        let panel = ThemePanel::new();
        self.open_panel(PanelState::Theme(panel));
    }

    /// 关闭 /theme 面板（不保存）
    pub fn close_theme_panel(&mut self) {
        self.session_mgr
            .current_mut()
            .session_panels
            .close_if(PanelKind::Theme);
    }

    /// 直接应用主题（/theme <name> 参数路径）
    pub fn apply_theme(&mut self, name: crate::ui::theme::ThemeName) {
        apply_theme_impl(&mut self.services, &mut self.session_mgr, name);
    }
}
