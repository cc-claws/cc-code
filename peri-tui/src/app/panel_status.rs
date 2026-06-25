use super::*;

impl App {
    /// 打开状态面板并激活指定 Tab
    pub fn open_status_panel(&mut self, tab: usize) {
        let panel = status_panel::StatusPanel::new(tab);
        self.open_panel(PanelState::Status(panel));
    }

    /// 关闭状态面板
    pub fn close_status_panel(&mut self) {
        self.global_panels.close_if(PanelKind::Status);
    }
}
