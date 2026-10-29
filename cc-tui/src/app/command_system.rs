use std::collections::HashSet;

use cc_middlewares::prelude::SkillMetadata;

use crate::command::CommandRegistry;

/// 一个不含 `/` 的命令名归属。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    /// 本地 TUI 命令（含别名、前缀唯一匹配）
    Local,
    /// Skill
    Skill,
    /// ACP 侧命令（来自 `AvailableCommandsUpdate`）
    Agent,
    /// 谁都不认
    Unknown,
}

/// 一段用户输入的归属判定结果。
///
/// 判定只此一处：空闲派发、运行中入队/拒绝、排队消费、以及「立即插入当前轮次」
/// 共用同一份规则，避免文本入队后身份丢失、出队又被重新解释成普通消息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputResolution {
    /// 本地 TUI 命令，需要 `&mut App` 立即执行。
    /// `deferrable` 表示运行中可否入队延后（见 `Command::blocked_while_busy`）。
    /// `immediate_while_busy` 表示运行中能否立即执行（见 `Command::execute_while_busy`）。
    LocalCommand {
        deferrable: bool,
        immediate_while_busy: bool,
    },
    /// ACP 侧命令：可入队延后，最终由 ACP 的命令路径执行。
    AgentCommand,
    /// Skill 或普通消息：作为 prompt 交给 agent。
    Prompt,
    /// 未知命令或前缀歧义。`matches` 为前缀命中的命令名。
    Unknown { matches: Vec<String> },
}

impl InputResolution {
    /// 运行中能否立即执行。
    pub fn is_immediate_while_busy(&self) -> bool {
        matches!(
            self,
            InputResolution::LocalCommand {
                immediate_while_busy: true,
                ..
            }
        )
    }

    /// 运行中能否安全入队、留到本轮结束后再执行。
    pub fn is_deferrable(&self) -> bool {
        match self {
            InputResolution::LocalCommand { deferrable, .. } => *deferrable,
            InputResolution::AgentCommand | InputResolution::Prompt => true,
            InputResolution::Unknown { .. } => false,
        }
    }

    /// 是否是「命令」（而非普通消息/Skill）。命令不允许插入正在执行的轮次。
    pub fn is_command(&self) -> bool {
        matches!(
            self,
            InputResolution::LocalCommand { .. } | InputResolution::AgentCommand
        )
    }
}

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
    /// 过滤已存在于本地 `command_registry` 或 `skills` 列表的名字，避免 Hints 浮层重复显示。
    pub fn update_agent_commands(&mut self, names: Vec<String>) {
        let local_names: HashSet<&str> = self
            .command_help_list
            .iter()
            .flat_map(|(n, _, aliases)| {
                // 同时包含别名，防止别名也重复
                std::iter::once(n.as_str()).chain(aliases.iter().map(|s| s.as_str()))
            })
            .collect();
        let skill_names: HashSet<&str> = self.skills.iter().map(|s| s.name.as_str()).collect();
        self.agent_commands = names
            .into_iter()
            .filter(|n| !local_names.contains(n.as_str()) && !skill_names.contains(n.as_str()))
            .collect();
    }

    /// 判定一个命令名（不含 `/`，不含参数）的归属。
    /// 优先级与 `CommandRegistry::dispatch` 一致：本地命令 > Skill > Agent 命令 > 未知。
    pub fn classify_name(&self, name: &str) -> NameKind {
        if self.command_registry.find(name).is_some() {
            return NameKind::Local;
        }
        if self.skills.iter().any(|s| s.name == name) {
            return NameKind::Skill;
        }
        if self.agent_commands.contains(name) {
            return NameKind::Agent;
        }
        NameKind::Unknown
    }

    /// 解析一段用户输入，判定它该本地立即执行、转发给 ACP、还是拒绝。
    ///
    /// 不以 `/` 开头的输入一律是普通消息。参数按第一个空格切分（与 `dispatch` 一致）。
    pub fn resolve(&self, text: &str) -> InputResolution {
        let Some(rest) = text.strip_prefix('/') else {
            return InputResolution::Prompt;
        };
        let name = match rest.split_once(' ') {
            Some((n, _)) => n.trim(),
            None => rest.trim(),
        };
        match self.classify_name(name) {
            NameKind::Skill => InputResolution::Prompt,
            NameKind::Agent => InputResolution::AgentCommand,
            NameKind::Local => {
                let (deferrable, immediate_while_busy) = self
                    .command_registry
                    .find(name)
                    .map(|cmd| (!cmd.blocked_while_busy(), cmd.execute_while_busy()))
                    .unwrap_or((false, false));
                InputResolution::LocalCommand {
                    deferrable,
                    immediate_while_busy,
                }
            }
            NameKind::Unknown => InputResolution::Unknown {
                matches: self.command_registry.prefix_names(name),
            },
        }
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
    fn test_update_agent_commands_filters_duplicate_skill_and_local_command_names() {
        // skills 列表已有 caveman
        let skills = vec![make_metadata("caveman", "desc")];
        let mut cs = CommandSystem::new(
            crate::command::default_registry(),
            skills,
            &crate::i18n::LcRegistry::default(),
        );
        // ACP 发来的命令列表包含 skill 名、本地已有命令名、和仅 agent 端的命令
        cs.update_agent_commands(vec!["compact".into(), "caveman".into(), "help".into()]);
        // caveman 应被过滤掉（已存在于 skills）
        assert!(
            !cs.agent_commands.contains("caveman"),
            "skill 名不应出现在 agent_commands 中"
        );
        // help 应被过滤掉（已存在于本地 command_registry）
        assert!(
            !cs.agent_commands.contains("help"),
            "本地已有命令不应出现在 agent_commands 中"
        );
        // compact 仅在 agent 端，应保留
        assert!(cs.agent_commands.contains("compact"));
    }

    #[test]
    fn test_update_agent_commands_skips_filtering_without_skills() {
        let mut cs = CommandSystem::new(
            crate::command::default_registry(),
            vec![],
            &crate::i18n::LcRegistry::default(),
        );
        cs.update_agent_commands(vec!["compact".into(), "caveman".into()]);
        assert!(cs.agent_commands.contains("compact"));
        assert!(cs.agent_commands.contains("caveman"));
    }

    include!("command_system_test.rs");
}
