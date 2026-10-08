    // 测试辅助：把用户全局文件指向临时目录内不存在的路径，
    // 避免开发机 `~/.cc-code/AGENTS.md` 污染断言。
    fn cfg_in(dir: &std::path::Path) -> AgentsMdConfig {
        AgentsMdConfig {
            user_global_file: dir.join("__no_such_global__.md"),
            ..Default::default()
        }
    }

    fn mw_in(dir: &std::path::Path) -> AgentsMdMiddleware {
        AgentsMdMiddleware::new().with_config(cfg_in(dir))
    }

    /// 临时目录 + 内嵌 `.git`：把 `find_project_root` **钉死**在 tempdir。
    ///
    /// 否则目录链会一直向上走到文件系统根——若 `TMPDIR`/`TEMP` 恰好落在某个
    /// git 仓库内（例如 CI 把 TMPDIR 指到 workspace），祖先目录的 AGENTS.md /
    /// CLAUDE.md 会被一并加载，测试结果就依赖环境了。
    fn repo_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        dir
    }

    #[tokio::test]
    async fn test_no_file_no_op() {
        let dir = repo_dir();
        let mw = mw_in(dir.path());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        let result = mw.before_agent(&mut state).await;
        assert!(result.is_ok());
        assert_eq!(state.messages().len(), 0);
    }

    #[tokio::test]
    async fn test_no_file_no_op_nonexistent_path() {
        let dir = std::path::Path::new("/nonexistent/path");
        let mw = mw_in(dir);
        let mut state = AgentState::new("/nonexistent/path");
        let result = mw.before_agent(&mut state).await;
        assert!(result.is_ok());
        assert_eq!(state.messages().len(), 0);
    }

    #[tokio::test]
    async fn test_with_file() {
        let dir = repo_dir();
        let agents_md = dir.path().join("AGENTS.md");
        std::fs::write(&agents_md, "# Project Guide\nDo things correctly.").unwrap();

        let mw = mw_in(dir.path());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1);
        assert!(state.messages()[0].is_system());
        assert!(state.messages()[0].content().contains("Project Guide"));
    }

    // ── dsh 加载模型：同目录全加载 + 去重 ──────────────────────────────────

    #[tokio::test]
    async fn test_same_dir_agents_and_claude_both_loaded() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("AGENTS.md"), "agents content").unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "claude content").unwrap();

        let mw = mw_in(dir.path());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        // 单条 System 消息，但两个文件的内容都在里面（不再是「先命中者独占」）
        assert_eq!(state.messages().len(), 1);
        let content = state.messages()[0].content();
        assert!(content.contains("agents content"), "{content}");
        assert!(content.contains("claude content"), "{content}");
        // AGENTS.md 在前（基础层有序）
        assert!(
            content.find("agents content").unwrap() < content.find("claude content").unwrap(),
            "{content}"
        );
        assert!(content.contains("## AGENTS.md"), "{content}");
        assert!(content.contains("## CLAUDE.md"), "{content}");
    }

    #[tokio::test]
    async fn test_same_dir_identical_content_deduped() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("AGENTS.md"), "shared rules\n").unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "  shared rules  \n").unwrap();

        let mw = mw_in(dir.path());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        let content = state.messages()[0].content();
        assert_eq!(content.matches("shared rules").count(), 1, "{content}");
        assert!(content.contains("## AGENTS.md"), "{content}");
        assert!(!content.contains("## CLAUDE.md"), "{content}");
    }

    #[tokio::test]
    async fn test_local_overlay_after_base() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("AGENTS.md"), "base rules").unwrap();
        std::fs::write(dir.path().join("AGENTS.local.md"), "local overlay").unwrap();

        let mw = mw_in(dir.path());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        let content = state.messages()[0].content();
        assert!(
            content.find("base rules").unwrap() < content.find("local overlay").unwrap(),
            "基础层应排在 .local 覆盖层之前: {content}"
        );
    }

    #[tokio::test]
    async fn test_empty_file_does_not_shadow() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("AGENTS.md"), "   \n\n  ").unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "claude real content").unwrap();

        let mw = mw_in(dir.path());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        // 回归：空 AGENTS.md 不再遮蔽后续候选
        assert_eq!(state.messages().len(), 1);
        assert!(state.messages()[0].content().contains("claude real content"));
    }

    // ── dsh 加载模型：跨目录拼接 ──────────────────────────────────────────

    #[tokio::test]
    async fn test_directory_chain_root_to_cwd_order() {
        let dir = repo_dir();
        let root = dir.path();
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();

        std::fs::write(root.join("AGENTS.md"), "root rules").unwrap();
        std::fs::write(sub.join("AGENTS.md"), "sub rules").unwrap();

        let mw = mw_in(root);
        let mut state = AgentState::new(sub.to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        let content = state.messages()[0].content();
        assert!(content.contains("## AGENTS.md"), "{content}");
        assert!(content.contains("## sub/AGENTS.md"), "{content}");
        assert!(
            content.find("root rules").unwrap() < content.find("sub rules").unwrap(),
            "越靠后越具体: {content}"
        );
    }

    #[tokio::test]
    async fn test_non_git_only_cwd() {
        let dir = repo_dir();
        let outer = dir.path();
        let inner = outer.join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        // outer 没有标记、inner 自己是根：root = inner，**不向上**取 outer/AGENTS.md。
        // （inner 里嵌 .git 以把链钉死，否则 TMP 落在别的仓库里时结果会变）
        std::fs::create_dir_all(inner.join(".git")).unwrap();
        std::fs::write(outer.join("AGENTS.md"), "outer rules").unwrap();

        let mw = mw_in(&inner);
        let mut state = AgentState::new(inner.to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 0, "root = cwd 时只查 cwd");
    }

    #[test]
    fn test_find_project_root_without_marker_falls_back_to_cwd() {
        // 找不到任何标记 → root = cwd（不扫祖先目录的指引文件）
        let deep = std::path::Path::new("/definitely/no/marker/here/sub");
        let found = find_project_root(deep, &["__no_such_marker__".to_string()]);
        assert_eq!(found, deep);
    }

    #[test]
    fn test_ancestor_chain_cwd_outside_root() {
        // cwd 不在 root 下 → 链退化为单目录
        let chain = ancestor_chain(
            std::path::Path::new("/some/root"),
            std::path::Path::new("/elsewhere/x"),
        );
        assert_eq!(chain, vec![std::path::PathBuf::from("/elsewhere/x")]);
    }

    #[test]
    fn test_ancestor_chain_root_equals_cwd() {
        let chain = ancestor_chain(std::path::Path::new("/a/b"), std::path::Path::new("/a/b"));
        assert_eq!(chain, vec![std::path::PathBuf::from("/a/b")]);
    }

    #[tokio::test]
    async fn test_find_project_root_walks_up() {
        let dir = repo_dir();
        let root = dir.path().join("repo");
        let sub = root.join("a").join("b");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();

        let found = find_project_root(&sub, &[".git".to_string()]);
        assert_eq!(
            std::fs::canonicalize(found).unwrap(),
            std::fs::canonicalize(&root).unwrap()
        );
    }

    #[tokio::test]
    async fn test_global_file_prepended() {
        let dir = repo_dir();
        let global = dir.path().join("global-agents.md");
        std::fs::write(&global, "global rules").unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "project rules").unwrap();

        let cfg = AgentsMdConfig {
            user_global_file: global,
            ..Default::default()
        };
        let mw = AgentsMdMiddleware::new().with_config(cfg);
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        let content = state.messages()[0].content();
        assert!(content.contains("global rules"), "{content}");
        assert!(
            content.find("global rules").unwrap() < content.find("project rules").unwrap(),
            "全局层在最宽处（链首）: {content}"
        );
    }

    // ── 冻结路径 ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_frozen_instructions_single_message() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("AGENTS.md"), "frozen rules").unwrap();

        let frozen = load_instructions(dir.path(), &cfg_in(dir.path())).unwrap();
        let mw = AgentsMdMiddleware::new().with_frozen_instructions(frozen);
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        state.add_message(BaseMessage::human("hi"));
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 2);
        assert!(state.messages()[0].is_system());
        assert!(state.messages()[0].content().contains("frozen rules"));
    }

    // ── 限额与截断 ─────────────────────────────────────────────────────────

    #[test]
    fn test_truncation_head_tail() {
        let max = 4096usize;
        let content = "A".repeat(20_000);
        let out = truncate_per_file(&content, max, "AGENTS.md");
        assert!(out.len() <= max, "{}", out.len());
        assert!(out.contains("[...truncated AGENTS.md"), "{out}");
        assert!(out.contains("Use file tools"), "{out}");
        // 头尾都保留
        assert!(out.starts_with('A'));
        assert!(out.ends_with('A'));
    }

    #[test]
    fn test_utf8_boundaries_multibyte() {
        // 中文 + emoji：任意字节级切分都不能 panic，且不超上限
        let content = "规则🚀".repeat(500);
        for max in [4usize, 8, 16, 33, 100, 4096, 100_000] {
            let out = truncate_per_file(&content, max, "AGENTS.md");
            assert!(out.len() <= max, "max={max} len={}", out.len());
            // 能成功构造 String 即未切坏 char boundary
            assert!(std::str::from_utf8(out.as_bytes()).is_ok());
        }
    }

    #[test]
    fn test_truncation_tiny_budget_degrades() {
        // 上限小到放不下标记：退化为头部截断，绝不 panic / 绝不超限
        let content = "abcdefghij".repeat(100);
        let out = truncate_bytes_head_tail(&content, 8, "AGENTS.md");
        assert!(out.len() <= 8, "{out}");
        assert!(out.starts_with("abcdefgh"), "{out}");
    }

    #[test]
    fn test_total_max_bytes_stops() {
        let cfg = AgentsMdConfig {
            max_bytes: 600,
            ..Default::default()
        };
        let files: Vec<InstructionFile> = (0..5)
            .map(|i| InstructionFile {
                source_path: PathBuf::from(format!("/tmp/f{i}.md")),
                abs_path: PathBuf::from(format!("/tmp/f{i}.md")),
                display: format!("f{i}.md"),
                content: format!("content-{i}").repeat(20),
            })
            .collect();

        let out = render_instruction_set(&files, &cfg);
        assert!(out.len() <= cfg.max_bytes, "总量不得超上限: {}", out.len());
        assert!(out.contains("## f0.md"), "{out}");
        assert!(out.contains("## f1.md"), "{out}");
        assert!(!out.contains("## f2.md"), "{out}");
        assert!(!out.contains("## f4.md"), "{out}");
        assert!(out.contains("instruction files omitted"), "{out}");
    }

    #[test]
    fn test_total_limit_exact_when_marker_cannot_fit() {
        // 额度小到连省略标记都放不下：总量仍不得越界（宁可标记残缺）
        let cfg = AgentsMdConfig {
            max_bytes: 40,
            ..Default::default()
        };
        let files: Vec<InstructionFile> = (0..3)
            .map(|i| InstructionFile {
                source_path: PathBuf::from(format!("/tmp/g{i}.md")),
                abs_path: PathBuf::from(format!("/tmp/g{i}.md")),
                display: format!("g{i}.md"),
                content: "z".repeat(500),
            })
            .collect();

        let out = render_instruction_set(&files, &cfg);
        assert!(out.len() <= 40, "{}", out.len());
        assert!(!out.is_empty());
    }

    #[tokio::test]
    async fn test_first_file_over_total_limit_still_included() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("AGENTS.md"), "y".repeat(5000)).unwrap();

        let cfg = AgentsMdConfig {
            max_bytes: 256,
            ..cfg_in(dir.path())
        };
        let out = load_instructions(dir.path(), &cfg).unwrap();
        assert!(out.len() <= 256, "{}", out.len());
        assert!(out.contains("## AGENTS.md"), "{out}");
    }

    #[tokio::test]
    async fn test_crlf_normalized_and_deduped() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("AGENTS.md"), "rule one\r\nrule two\r\n").unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "rule one\nrule two\n").unwrap();

        let mw = mw_in(dir.path());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        let content = state.messages()[0].content();
        assert!(!content.contains('\r'), "CRLF 应归一为 LF");
        assert_eq!(content.matches("rule one").count(), 1, "{content}");
    }

    #[tokio::test]
    async fn test_prepends_before_existing_messages() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("AGENTS.md"), "system instructions").unwrap();

        let mw = mw_in(dir.path());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        state.add_message(BaseMessage::human("user question"));
        mw.before_agent(&mut state).await.unwrap();

        // 系统消息应在 human 消息之前
        assert_eq!(state.messages().len(), 2);
        assert!(state.messages()[0].is_system());
        assert!(matches!(state.messages()[1], BaseMessage::Human { .. }));
    }

    #[tokio::test]
    async fn test_excludes_matching_file_skipped() {
        let dir = repo_dir();
        let claude_md = dir.path().join("CLAUDE.md");
        std::fs::write(&claude_md, "should be excluded").unwrap();

        let mw = mw_in(dir.path()).with_excludes(vec![format!("{}", claude_md.display())]);
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(
            state.messages().len(),
            0,
            "excluded file should not be loaded"
        );
    }

    #[tokio::test]
    async fn test_excludes_non_matching_file_loaded() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("CLAUDE.md"), "should be loaded").unwrap();

        let mw = mw_in(dir.path()).with_excludes(vec!["**/node_modules/**".to_string()]);
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1);
        assert!(state.messages()[0].content().contains("should be loaded"));
    }

    // ── CLAUDE.local.md tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_local_md_only() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("CLAUDE.local.md"), "local only content").unwrap();

        let mw = mw_in(dir.path());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1);
        assert!(state.messages()[0].content().contains("local only content"));
    }

    #[tokio::test]
    async fn test_claude_md_and_local_merged() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("CLAUDE.md"), "main content").unwrap();
        std::fs::write(dir.path().join("CLAUDE.local.md"), "local content").unwrap();

        let mw = mw_in(dir.path());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1);
        let content = state.messages()[0].content();
        assert!(content.contains("main content"));
        assert!(content.contains("local content"));
    }

    #[tokio::test]
    async fn test_local_md_empty_not_appended() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("CLAUDE.md"), "main content").unwrap();
        std::fs::write(dir.path().join("CLAUDE.local.md"), "   \n  ").unwrap();

        let mw = mw_in(dir.path());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1);
        let content = state.messages()[0].content();
        assert!(content.contains("main content"));
        assert!(!content.contains("local"));
    }

    // ── @import tests ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_import_simple() {
        let dir = repo_dir();
        let imported = dir.path().join("rules.md");
        std::fs::write(&imported, "imported rules").unwrap();
        std::fs::write(
            dir.path().join("CLAUDE.md"),
            "header\n<!-- @import rules.md -->\nfooter",
        )
        .unwrap();

        let mw = mw_in(dir.path());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        let content = state.messages()[0].content();
        assert!(content.contains("header"));
        assert!(content.contains("imported rules"));
        assert!(content.contains("footer"));
        assert!(!content.contains("@import"));
    }

    #[tokio::test]
    async fn test_import_in_agents_md_too() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("extra.md"), "extra rules").unwrap();
        std::fs::write(
            dir.path().join("AGENTS.md"),
            "top\n<!-- @import extra.md -->\nbottom",
        )
        .unwrap();

        let mw = mw_in(dir.path());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        // AGENTS.md 现在也解析 @import（此前只对 CLAUDE* 生效）
        let content = state.messages()[0].content();
        assert!(content.contains("extra rules"), "{content}");
        assert!(!content.contains("@import"), "{content}");
    }

    #[tokio::test]
    async fn test_import_nested() {
        let dir = repo_dir();
        let sub_dir = dir.path().join("sub");
        std::fs::create_dir_all(&sub_dir).unwrap();
        let inner = sub_dir.join("inner.md");
        std::fs::write(&inner, "inner content").unwrap();
        let outer = dir.path().join("outer.md");
        std::fs::write(&outer, "outer <!-- @import sub/inner.md --> end").unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "<!-- @import outer.md -->").unwrap();

        let mw = mw_in(dir.path());
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        let content = state.messages()[0].content();
        assert!(content.contains("inner content"));
    }

    #[test]
    fn test_import_max_depth() {
        let dir = repo_dir();
        let imported = dir.path().join("deep.md");
        std::fs::write(&imported, "deep content").unwrap();
        let content = "<!-- @import deep.md -->".to_string();
        let mut visited = HashSet::new();
        // depth 0 should return original content
        let result = resolve_imports(&content, dir.path(), 0, &mut visited);
        assert!(result.contains("@import"));
    }

    #[test]
    fn test_import_cycle_detection() {
        let dir = repo_dir();
        let a = dir.path().join("a.md");
        let b = dir.path().join("b.md");
        std::fs::write(&a, "<!-- @import b.md -->").unwrap();
        std::fs::write(&b, "<!-- @import a.md -->").unwrap();

        let main = dir.path().join("main.md");
        std::fs::write(&main, "<!-- @import a.md -->").unwrap();

        let mut visited = HashSet::new();
        visited.insert(main.clone());
        // Should not panic or infinite loop
        let result = resolve_imports(
            &std::fs::read_to_string(&main).unwrap(),
            dir.path(),
            3,
            &mut visited,
        );
        // a.md's @import b.md should be resolved, but b.md's @import a.md should be kept as-is (cycle)
        assert!(!result.is_empty());
    }

    #[test]
    fn test_import_nonexistent_file() {
        let content = "<!-- @import nonexistent.md -->";
        let mut visited = HashSet::new();
        let result = resolve_imports(content, Path::new("/tmp"), 3, &mut visited);
        assert!(
            result.contains("@import"),
            "nonexistent file should keep original placeholder"
        );
    }

    #[test]
    fn test_import_invalid_format() {
        let content = "<!-- @import no closing tag";
        let mut visited = HashSet::new();
        let result = resolve_imports(content, Path::new("/tmp"), 3, &mut visited);
        assert!(
            result.contains("@import"),
            "invalid format should preserve original text"
        );
    }

    #[tokio::test]
    async fn test_import_in_local_and_global_too() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("from-local.md"), "local import body").unwrap();
        std::fs::write(dir.path().join("from-global.md"), "global import body").unwrap();
        std::fs::write(
            dir.path().join("CLAUDE.local.md"),
            "L <!-- @import from-local.md -->",
        )
        .unwrap();
        let global = dir.path().join("global-agents.md");
        std::fs::write(&global, "G <!-- @import from-global.md -->").unwrap();

        let cfg = AgentsMdConfig {
            user_global_file: global,
            ..AgentsMdConfig::default()
        };
        let mw = AgentsMdMiddleware::new().with_config(cfg);
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        // spec §2.6：@import 对所有候选生效（含 .local 与用户全局文件）
        let content = state.messages()[0].content();
        assert!(content.contains("local import body"), "{content}");
        assert!(content.contains("global import body"), "{content}");
        assert!(!content.contains("@import"), "{content}");
    }

    #[tokio::test]
    async fn test_excludes_applied_before_dedup() {
        let dir = repo_dir();
        // 两候选内容相同；排除 AGENTS.md 后，CLAUDE.md 必须仍然生效
        // （回归：若先去重后过滤，被排除的 AGENTS.md 会「连坐」挤掉 CLAUDE.md → 一条都不注入）
        std::fs::write(dir.path().join("AGENTS.md"), "shared rules").unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "shared rules").unwrap();

        let excluded = dir.path().join("AGENTS.md");
        let mw = mw_in(dir.path()).with_excludes(vec![excluded.display().to_string()]);
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();

        assert_eq!(state.messages().len(), 1, "未被排除的候选仍应注入");
        let content = state.messages()[0].content();
        assert!(content.contains("shared rules"), "{content}");
        assert!(content.contains("## CLAUDE.md"), "{content}");
    }

    #[tokio::test]
    async fn test_excludes_do_not_consume_abs_dedup_slot() {
        let dir = repo_dir();
        std::fs::write(dir.path().join("CLAUDE.md"), "keep me").unwrap();

        // 排除一个不存在的路径不应影响正常加载
        let mw = mw_in(dir.path())
            .with_excludes(vec![dir.path().join("nope.md").display().to_string()]);
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();
        assert_eq!(state.messages().len(), 1);
    }

    #[test]
    fn test_first_file_truncation_keeps_provenance_header() {
        // 首个文件自己就顶破总量时，provenance 头必须留下（§2.2）
        let cfg = AgentsMdConfig {
            max_bytes: 200,
            ..Default::default()
        };
        let files = vec![InstructionFile {
            source_path: PathBuf::from("/tmp/big.md"),
            abs_path: PathBuf::from("/tmp/big.md"),
            display: "big.md".to_string(),
            content: "q".repeat(10_000),
        }];

        let out = render_instruction_set(&files, &cfg);
        assert!(out.len() <= 200, "{}", out.len());
        assert!(out.starts_with("## big.md\n\n"), "{out}");
    }

    #[test]
    fn test_import_cycle_and_depth_in_load() {
        let dir = repo_dir();
        let a = dir.path().join("cyc-a.md");
        let b = dir.path().join("cyc-b.md");
        std::fs::write(&a, "A <!-- @import cyc-b.md -->").unwrap();
        std::fs::write(&b, "B <!-- @import cyc-a.md -->").unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "top <!-- @import cyc-a.md -->").unwrap();

        let out = load_instructions(dir.path(), &cfg_in(dir.path())).unwrap();
        assert!(out.contains("A "), "{out}");
        assert!(out.contains("B "), "{out}");
        // 环上的回边保留占位符，不 panic、不死循环
        assert!(out.contains("@import cyc-a.md"), "{out}");
    }
