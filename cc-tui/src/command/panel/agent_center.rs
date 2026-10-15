use crate::{app::App, command::Command};

pub struct AgentCenterCommand;
impl Command for AgentCenterCommand {
    fn name(&self) -> &str {
        "agents"
    }
    fn description(&self, lc: &crate::i18n::LcRegistry) -> String {
        lc.tr("command-agent-center-description")
    }
    fn execute(&self, app: &mut App, _args: &str) {
        app.open_agent_center();
    }
}
