use super::*;

impl App {
    /// 打开 /memory 面板
    pub fn open_memory_panel(&mut self) {
        let home_dir = dirs_next::home_dir();
        let mut panel = crate::app::memory_panel::MemoryPanel::new(
            &self.services.cwd,
            home_dir,
            &self.services.lc,
        );
        panel.refresh_exists();
        self.open_panel(PanelState::Memory(panel));
    }

    /// 关闭 /memory 面板
    pub fn close_memory_panel(&mut self) {
        self.global_panels.close_if(PanelKind::Memory);
    }

    /// 打开外部编辑器编辑选中的 memory 文件
    pub fn memory_panel_open_editor(&mut self) -> anyhow::Result<()> {
        let entry = self
            .global_panels
            .get::<crate::app::memory_panel::MemoryPanel>()
            .and_then(|p| p.entries.get(p.cursor()))
            .cloned();
        let Some(entry) = entry else {
            return Ok(());
        };

        // 文件不存在时创建空文件
        if !entry.path.exists() {
            if let Some(parent) = entry.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::File::create(&entry.path)?;
            // 刷新面板中的 exists 状态
            if let Some(ref mut panel) = self
                .global_panels
                .get_mut::<crate::app::memory_panel::MemoryPanel>()
            {
                panel.refresh_exists();
            }
        }

        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
        tracing::info!("Opening memory file with {}: {:?}", editor, entry.path);

        // 请求下一帧物理清屏 + 全量重绘（#587）：离开 alternate screen 后
        // 原屏内容已不可知（重进 alt screen 是空白屏），ratatui 的 buffer
        // 差值绘制会只 flush 变化 cell，导致大面积空白/残影。
        // 必须放在挂起之前：挂起步骤本身带 `?`（无 console 的 Windows
        // 环境下 disable_raw_mode 就会失败），置位若排在它们后面，提前
        // 返回会把它整段跳过——而挂起失败恰恰是屏幕状态最不可信的时刻。
        // 一旦决定去编辑器，任何部分挂起/恢复失败都要靠这次重绘兜底。
        self.session_mgr
            .current_mut()
            .ui
            .request_terminal_clear_redraw();

        // 挂起 TUI: 关闭鼠标捕获 + 离开 alternate screen + 恢复 raw mode，
        // 让外部编辑器独占终端。
        let _input_pause = crate::event::pause_input();
        let _ = crate::conpty::disable_mouse_tracking();
        let fullscreen = self.global_ui.viewport.fullscreen_latched;
        let status = (|| -> anyhow::Result<std::io::Result<std::process::ExitStatus>> {
            ratatui::crossterm::execute!(
                std::io::stdout(),
                ratatui::crossterm::event::DisableMouseCapture
            )?;
            if fullscreen {
                crate::terminal_backend::viewport::leave_alternate_screen(&mut std::io::stdout())?;
            } else {
                // 正常屏内容在编辑器期间保存在宿主缓冲区，临时 alt 不锁定应用视口。
                crate::terminal_backend::viewport::enter_alternate_screen(&mut std::io::stdout())?;
            }
            ratatui::crossterm::terminal::disable_raw_mode()?;

            // 启动编辑器
            // 文件路径经环境变量传递并在 shell 内引用，空格、&、$ 等不会变成命令。
            #[cfg(windows)]
            let editor_command = format!("{editor} \"%PERI_MEMORY_EDITOR_FILE%\"");
            #[cfg(not(windows))]
            let editor_command = format!("{editor} \"$PERI_MEMORY_EDITOR_FILE\"");
            let mut command = cc_middlewares::process::shell_command(&editor_command, &[]);
            command.env("PERI_MEMORY_EDITOR_FILE", &entry.path);
            // 编辑器共享当前交互控制台，不能继承后台 shell 的 CREATE_NO_WINDOW。
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.as_std_mut().creation_flags(0);
            }
            Ok(command.as_std_mut().status())
        })();

        // 即使挂起或 raw mode 恢复失败，也先尽力恢复屏幕与鼠标。
        let screen_result = if fullscreen {
            crate::terminal_backend::viewport::enter_alternate_screen(&mut std::io::stdout())
        } else {
            crate::terminal_backend::viewport::leave_alternate_screen(&mut std::io::stdout())
        };
        let raw_result = ratatui::crossterm::terminal::enable_raw_mode();
        #[cfg(windows)]
        let vt_result = crate::conpty::enable_vt_processing();
        let mouse_result = ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::event::EnableMouseCapture
        );
        // ConPTY fix: force mouse-tracking notify (same workaround as main.rs).
        if raw_result.is_ok() && mouse_result.is_ok() {
            let _ = crate::conpty::enable_mouse_tracking();
        }
        screen_result?;
        raw_result?;
        #[cfg(windows)]
        vt_result?;
        mouse_result?;

        match status? {
            Ok(s) if s.success() => {
                tracing::info!("Editor exited successfully");
            }
            Ok(s) => {
                tracing::warn!("Editor exited with status: {}", s);
            }
            Err(e) => {
                tracing::error!("Failed to launch editor: {}", e);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("panel_memory_test.rs");
}
