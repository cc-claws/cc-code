use crate::app::App;

impl App {
    pub(super) fn flush_tool_output_refresh(&mut self) -> bool {
        if !self
            .session_mgr
            .current()
            .agent
            .tool_output_refresh_due(std::time::Instant::now())
        {
            return false;
        }
        self.request_rebuild();
        true
    }
}

#[cfg(test)]
#[path = "output_refresh_test.rs"]
mod tests;
