
    use crate::app::command_system::{InputResolution, NameKind};

    fn make_system(agent_commands: &[&str], skills: &[&str]) -> CommandSystem {
        let skills = skills
            .iter()
            .map(|name| make_metadata(name, "desc"))
            .collect();
        let mut cs = CommandSystem::new(
            crate::command::default_registry(),
            skills,
            &crate::i18n::LcRegistry::default(),
        );
        cs.update_agent_commands(agent_commands.iter().map(|s| s.to_string()).collect());
        cs
    }

    #[test]
    fn test_classify_name_distinguishes_local_skill_and_agent() {
        let cs = make_system(&["compact"], &["caveman"]);
        assert_eq!(cs.classify_name("theme"), NameKind::Local);
        // 别名也算本地命令
        assert_eq!(cs.classify_name("undo"), NameKind::Local);
        assert_eq!(cs.classify_name("caveman"), NameKind::Skill);
        assert_eq!(cs.classify_name("compact"), NameKind::Agent);
        assert_eq!(cs.classify_name("no-such-thing"), NameKind::Unknown);
    }

    #[test]
    fn test_resolve_plain_text_is_prompt() {
        let cs = make_system(&[], &[]);
        let resolution = cs.resolve("帮我看下这个函数");
        assert_eq!(resolution, InputResolution::Prompt);
        assert!(!resolution.is_command(), "普通消息不是命令");
    }

    #[test]
    fn test_resolve_ui_command_is_not_deferrable() {
        let cs = make_system(&[], &[]);
        // /theme 要开面板，运行中不可延后
        let resolution = cs.resolve("/theme dark");
        assert_eq!(
            resolution,
            InputResolution::LocalCommand {
                deferrable: false,
                immediate_while_busy: false,
            }
        );
        assert!(resolution.is_command());
        assert!(!resolution.is_deferrable());
        assert!(!resolution.is_immediate_while_busy());
    }

    #[test]
    fn test_resolve_immediate_while_busy_command() {
        let cs = make_system(&[], &[]);
        // /ps 是纯只读面板命令，运行中允许立即打开，不进队列也不拒绝
        let resolution = cs.resolve("/ps");
        assert_eq!(
            resolution,
            InputResolution::LocalCommand {
                deferrable: false,
                immediate_while_busy: true,
            }
        );
        assert!(resolution.is_command());
        assert!(!resolution.is_deferrable());
        assert!(resolution.is_immediate_while_busy());
    }

    #[test]
    fn test_resolve_forwarding_command_is_deferrable() {
        let cs = make_system(&[], &[]);
        // /recap 的 execute 只是 submit_message，运行中可入队延后
        for text in ["/recap", "/recap 补充参数", "/commit", "/review 123", "/init"] {
            let resolution = cs.resolve(text);
            assert_eq!(
                resolution,
                InputResolution::LocalCommand {
                    deferrable: true,
                    immediate_while_busy: false,
                },
                "{text} 应判定为可延后的本地命令"
            );
            assert!(resolution.is_deferrable(), "{text} 应可入队延后");
            assert!(!resolution.is_immediate_while_busy());
        }
    }

    #[test]
    fn test_resolve_agent_command_is_deferrable() {
        let cs = make_system(&["compact"], &[]);
        let resolution = cs.resolve("/compact");
        assert_eq!(resolution, InputResolution::AgentCommand);
        assert!(resolution.is_command());
        assert!(resolution.is_deferrable());
    }

    #[test]
    fn test_resolve_skill_is_prompt() {
        let cs = make_system(&[], &["caveman"]);
        // Skill 就是一段 prompt，不是命令（允许插入运行中的轮次）
        let resolution = cs.resolve("/caveman 说人话");
        assert_eq!(resolution, InputResolution::Prompt);
        assert!(!resolution.is_command());
    }

    #[test]
    fn test_resolve_alias_and_unique_prefix_hit_local_command() {
        let cs = make_system(&[], &[]);
        assert_eq!(
            cs.resolve("/undo"),
            InputResolution::LocalCommand {
                deferrable: false,
                immediate_while_busy: false,
            },
            "别名应命中本地命令"
        );
        assert_eq!(
            cs.resolve("/strea"),
            InputResolution::LocalCommand {
                deferrable: true,
                immediate_while_busy: false,
            },
            "唯一前缀应命中本地命令"
        );
    }

    #[test]
    fn test_resolve_unknown_reports_prefix_matches() {
        let cs = make_system(&[], &[]);
        assert_eq!(
            cs.resolve("/no-such-thing"),
            InputResolution::Unknown { matches: vec![] }
        );
        // /re 命中多个命令（recap/rename/review/rewind）→ 歧义
        let InputResolution::Unknown { matches } = cs.resolve("/re") else {
            panic!("/re 应判定为未知（歧义）");
        };
        assert!(
            matches.len() > 1,
            "歧义应列出多个候选，实际: {matches:?}"
        );
        assert!(
            !cs.resolve("/re").is_deferrable(),
            "未知/歧义命令不得入队延后"
        );
    }

    #[test]
    fn test_resolve_ignores_arguments_when_matching_name() {
        let cs = make_system(&[], &[]);
        // 带参数不应影响命令名匹配
        assert_eq!(
            cs.resolve("/theme some arg with spaces"),
            InputResolution::LocalCommand {
                deferrable: false,
                immediate_while_busy: false,
            }
        );
    }
