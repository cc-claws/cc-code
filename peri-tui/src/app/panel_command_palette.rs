use super::*;
use super::command_palette_panel::CommandPalettePanel;

impl App {
    /// 打开 CommandPalette 面板
    pub fn open_command_palette(&mut self) {
        let cfg = self
            .services
            .peri_config
            .get_or_insert_with(PeriConfig::default);
        let panel = CommandPalettePanel::from_config(cfg);
        self.open_panel(PanelState::CommandPalette(panel));
    }
}
