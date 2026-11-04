
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

    #[test]
    fn test_looks_like_command_syntax_rules() {
        use crate::app::command_system::looks_like_command;
        // 合法语态
        assert!(looks_like_command("help"));
        assert!(looks_like_command("git-commit"));
        assert!(looks_like_command("my_custom_cmd"));
        assert!(looks_like_command("plugin:install"));
        assert!(looks_like_command("v2"));

        // 非合法语态（路径、URL、符号、空串、中文等）
        assert!(!looks_like_command(""));
        assert!(!looks_like_command("api/v1/items"));
        assert!(!looks_like_command("query_data?"));
        assert!(!looks_like_command("var/log/app.log"));
        assert!(!looks_like_command("test.py"));
        assert!(!looks_like_command("用户接口"));
        assert!(!looks_like_command("/"));
    }

    #[test]
    fn test_resolve_api_and_file_paths_fall_back_to_prompt() {
        let cs = make_system(&[], &[]);

        // 路径形态输入与自然语言提问（通用占位路径）
        let api_input = "/api/v1/resource/query_items?page=1 参数怎么传";
        assert_eq!(
            cs.resolve(api_input),
            InputResolution::Prompt,
            "API 地址与多级路径不应被拦截为未知命令，应降级为普通 Prompt"
        );

        // 纯 API 路径无空格
        assert_eq!(
            cs.resolve("/api/v1/resource/query_items"),
            InputResolution::Prompt,
            "纯多级路径应降级为普通 Prompt"
        );

        // 带查询参数的路径
        assert_eq!(
            cs.resolve("/search?q=rust&limit=10"),
            InputResolution::Prompt,
            "带 URL 查询参数的输入应降级为普通 Prompt"
        );

        // 绝对文件路径
        assert_eq!(
            cs.resolve("/var/log/syslog 帮我排查下错误"),
            InputResolution::Prompt,
            "文件绝对路径应降级为普通 Prompt"
        );

        // 单斜杠或代码注释
        assert_eq!(
            cs.resolve("// this is a comment"),
            InputResolution::Prompt,
            "双斜杠注释应降级为普通 Prompt"
        );
        assert_eq!(
            cs.resolve("/"),
            InputResolution::Prompt,
            "单斜杠应降级为普通 Prompt"
        );

        // 正常拼错的命令依然走 Unknown 报错提示
        let typo_cmd = cs.resolve("/hlp");
        assert!(
            matches!(typo_cmd, InputResolution::Unknown { .. }),
            "纯字母拼错的命令应被识别为 Unknown 并提示"
        );
    }
