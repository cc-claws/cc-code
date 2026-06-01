use std::collections::HashSet;

use peri_middlewares::prelude::SkillMetadata;

use crate::command::CommandRegistry;

/// 命令系统：命令注册表、帮助列表、Skills 元数据、Agent 命令集合。
///
/// `agent_commands` 存储从 ACP `AvailableCommandsUpdate` 学习到的命令名集合。
/// 当本地 UICommand 未匹配时，检查该集合——命中则通过 `session/prompt` 发给 Agent 执行。
pub struct CommandSystem {
    pub command_registry: CommandRegistry,
    pub command_help_list: Vec<(String, String, Vec<String>)>,
    pub skills: Vec<SkillMetadata>,
    /// 从 ACP AvailableCommandsUpdate 学习到的 Agent 命令名集合（不含 `/` 前缀）。
    pub agent_commands: HashSet<String>,
}

impl CommandSystem {
    pub fn new(
        command_registry: CommandRegistry,
        skills: Vec<SkillMetadata>,
        lc: &crate::i18n::LcRegistry,
    ) -> Self {
        let command_help_list = command_registry.list(lc);
        Self {
            command_registry,
            command_help_list,
            skills,
            agent_commands: HashSet::new(),
        }
    }

    /// 从 ACP `AvailableCommandsUpdate` 更新 agent 命令列表。
    pub fn update_agent_commands(&mut self, names: Vec<String>) {
        self.agent_commands = names.into_iter().collect();
    }
}
