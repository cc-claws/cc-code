    fn write_skill(dir: &std::path::Path, name: &str, desc: &str) {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let content = format!(
            "---\nname: '{}'\ndescription: '{}'\n---\n\n# {}\n",
            name, desc, name
        );
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    #[tokio::test]
    async fn test_no_skills_no_op() {
        // 使用临时目录作为所有 skills 目录来源，确保测试隔离
        let empty_dir = tempdir().unwrap();
        let empty_path = empty_dir.path().to_path_buf();

        let mw = SkillsMiddleware::new()
            .with_user_dir(empty_path.clone())
            .with_project_dir(empty_path);
        let mut state = AgentState::new("/nonexistent/path");
        let result = mw.before_agent(&mut state).await;
        assert!(result.is_ok());
        assert_eq!(state.messages().len(), 0);
    }

    #[tokio::test]
    async fn test_injects_summary() {
        let dir = tempdir().unwrap();
        let skills_dir = dir.path().join(".claude").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        write_skill(&skills_dir, "tui-dev", "构建 TUI 应用");
        write_skill(&skills_dir, "codebase-exploration", "深度代码搜索");

        let mw = SkillsMiddleware::new();
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1);
        let msg = &state.messages()[0];
        assert!(msg.is_system());
        let content = msg.content();
        assert!(content.contains("tui-dev"));
        assert!(content.contains("codebase-exploration"));
        assert!(content.contains("Skills"));
    }

    #[tokio::test]
    async fn test_custom_project_dir() {
        let dir = tempdir().unwrap();
        write_skill(dir.path(), "custom-skill", "自定义技能");

        let mw = SkillsMiddleware::new().with_project_dir(dir.path().to_path_buf());
        let mut state = AgentState::new("/any/cwd");
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1);
        assert!(state.messages()[0].content().contains("custom-skill"));
    }

    #[tokio::test]
    async fn test_build_summary_contains_slash_prefix() {
        let dir = tempdir().unwrap();
        let skills_dir = dir.path().join(".claude").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        write_skill(&skills_dir, "test-skill", "test description");

        let mw = SkillsMiddleware::new();
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        let content = state.messages()[0].content();
        assert!(
            content.contains("'/skill-name'"),
            "提示词应包含 '/skill-name' 格式，实际: {}",
            content
        );
    }

    #[tokio::test]
    async fn test_build_summary_does_not_contain_hash_prefix() {
        let dir = tempdir().unwrap();
        let skills_dir = dir.path().join(".claude").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        write_skill(&skills_dir, "test-skill", "test description");

        let mw = SkillsMiddleware::new();
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        let content = state.messages()[0].content();
        assert!(
            !content.contains("#skill_name"),
            "提示词不应包含旧 #skill_name 格式，实际: {}",
            content
        );
    }

    #[tokio::test]
    async fn test_extra_dirs_injected() {
        let dir = tempdir().unwrap();
        let extra1 = dir.path().join("extra1");
        let extra2 = dir.path().join("extra2");
        std::fs::create_dir_all(&extra1).unwrap();
        std::fs::create_dir_all(&extra2).unwrap();
        write_skill(&extra1, "extra-skill-1", "from extra 1");
        write_skill(&extra2, "extra-skill-2", "from extra 2");

        let mw = SkillsMiddleware::new()
            .with_user_dir(dir.path().to_path_buf())
            .with_project_dir(dir.path().to_path_buf())
            .with_extra_dirs(vec![extra1.clone(), extra2.clone()]);

        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        let content = state.messages()[0].content();
        assert!(
            content.contains("extra-skill-1"),
            "Should include skill from extra dir 1"
        );
        assert!(
            content.contains("extra-skill-2"),
            "Should include skill from extra dir 2"
        );
    }

    #[tokio::test]
    async fn test_extra_dirs_nonexistent_skipped() {
        let dir = tempdir().unwrap();
        let mw = SkillsMiddleware::new()
            .with_user_dir(dir.path().to_path_buf())
            .with_project_dir(dir.path().to_path_buf())
            .with_extra_dirs(vec![dir.path().join("nonexistent")]);

        let mut state = AgentState::new(dir.path().to_str().unwrap());
        let result = mw.before_agent(&mut state).await;
        assert!(result.is_ok());
        assert_eq!(state.messages().len(), 0, "No skills should be injected");
    }

    #[tokio::test]
    async fn test_extra_dirs_priority_after_project() {
        let dir = tempdir().unwrap();
        // project skills directory (acts as cwd/.claude/skills)
        let project_skills = dir.path().join("project-skills");
        std::fs::create_dir_all(&project_skills).unwrap();
        write_skill(&project_skills, "project-skill", "from project");

        let extra_dir = dir.path().join("extra");
        std::fs::create_dir_all(&extra_dir).unwrap();
        write_skill(&extra_dir, "extra-skill", "from extra");

        let mw = SkillsMiddleware::new()
            .with_user_dir(dir.path().to_path_buf())
            .with_project_dir(project_skills)
            .with_extra_dirs(vec![extra_dir]);

        let mut state = AgentState::new("/nonexistent");
        mw.before_agent(&mut state).await.unwrap();

        let content = state.messages()[0].content();
        assert!(content.contains("project-skill"));
        assert!(content.contains("extra-skill"));
    }
