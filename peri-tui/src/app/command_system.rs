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
    /// 过滤已存在于 `skills` 列表的名字，避免 Hints 浮层重复显示。
    pub fn update_agent_commands(&mut self, names: Vec<String>) {
        let skill_names: std::collections::HashSet<&str> =
            self.skills.iter().map(|s| s.name.as_str()).collect();
        self.agent_commands = names
            .into_iter()
            .filter(|n| !skill_names.contains(n.as_str()))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_metadata(name: &str, desc: &str) -> SkillMetadata {
        SkillMetadata {
            name: name.to_string(),
            description: desc.to_string(),
            path: PathBuf::from(format!("/fake/{name}/SKILL.md")),
        }
    }

    #[test]
    fn test_update_agent_commands_过滤重复的_skill_名() {
        // skills 列表已有 caveman
        let skills = vec![make_metadata("caveman", "desc")];
        let mut cs = CommandSystem::new(
            crate::command::default_registry(),
            skills,
            &crate::i18n::LcRegistry::default(),
        );
        // ACP 发来的命令列表包含 skill 名和普通命令名
        cs.update_agent_commands(vec!["compact".into(), "caveman".into(), "help".into()]);
        // caveman 应被过滤掉（已存在于 skills）
        assert!(
            !cs.agent_commands.contains("caveman"),
            "skill 名不应出现在 agent_commands 中"
        );
        // 普通命令应保留
        assert!(cs.agent_commands.contains("compact"));
        assert!(cs.agent_commands.contains("help"));
    }

    #[test]
    fn test_update_agent_commands_无_skills_时不过滤() {
        let mut cs = CommandSystem::new(
            crate::command::default_registry(),
            vec![],
            &crate::i18n::LcRegistry::default(),
        );
        cs.update_agent_commands(vec!["compact".into(), "caveman".into()]);
        assert!(cs.agent_commands.contains("compact"));
        assert!(cs.agent_commands.contains("caveman"));
    }
}
