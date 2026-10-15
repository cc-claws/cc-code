    fn make_lc() -> crate::i18n::LcRegistry {
        crate::i18n::LcRegistry::default()
    }

    fn cand(id: &str, cooldown: u64, priority: i64) -> super::TipCandidate {
        super::TipCandidate {
            id: id.to_string(),
            text: format!("text-{id}"),
            cooldown_sessions: cooldown,
            priority,
            label: None,
        }
    }

    #[test]
    fn test_tips_contains_slash_command_hint() {
        let lc = make_lc();
        let tip = lc.tr("tip-0");
        assert!(tip.contains("/"), "tip-0 应包含 '/' 提示: {}", tip);
    }

    #[test]
    fn test_tips_tab_hint() {
        let lc = make_lc();
        let tip = lc.tr("tip-0");
        assert!(tip.contains("Tab"), "tip-0 应包含 'Tab': {}", tip);
    }

    #[test]
    fn test_builtin_candidates_cover_all_i18n_tips() {
        let lc = make_lc();
        let cands = super::builtin_candidates(&lc);
        assert_eq!(cands.len(), super::BUILTIN_TIP_COUNT);
        for (i, c) in cands.iter().enumerate() {
            assert_eq!(c.id, format!("tip-{i}"));
            assert!(!c.text.is_empty(), "tip-{i} 文案不应为空");
            assert_ne!(c.text, c.id, "tip-{i} 应有翻译内容");
            assert_eq!(c.cooldown_sessions, super::BUILTIN_COOLDOWN_SESSIONS);
        }
    }

    #[test]
    fn test_select_tip_prefers_never_shown_then_longest_since() {
        let cands = vec![cand("a", 0, 0), cand("b", 0, 0), cand("c", 0, 0)];
        let mut history = super::TipHistory {
            session_counter: 20,
            ..Default::default()
        };
        // 全部未展示：取注册顺序第一条
        assert_eq!(super::select_tip(&cands, &history).unwrap().id, "a");
        // a 在 19 展示过（间隔 1），b 在 5 展示过（间隔 15），c 未展示（间隔 ∞）→ c
        history.shown.insert(
            "a".into(),
            super::TipShownRecord { last_shown_session: 19, shown_count: 1 },
        );
        history.shown.insert(
            "b".into(),
            super::TipShownRecord { last_shown_session: 5, shown_count: 1 },
        );
        assert_eq!(super::select_tip(&cands, &history).unwrap().id, "c");
        // c 也刚展示（间隔 0）→ b（间隔 15 > a 的 1）
        history.shown.insert(
            "c".into(),
            super::TipShownRecord { last_shown_session: 20, shown_count: 1 },
        );
        assert_eq!(super::select_tip(&cands, &history).unwrap().id, "b");
    }

    #[test]
    fn test_select_tip_respects_cooldown_and_priority() {
        let cands = vec![cand("a", 10, 0), cand("b", 10, 5)];
        let mut history = super::TipHistory {
            session_counter: 12,
            ..Default::default()
        };
        history.shown.insert(
            "a".into(),
            super::TipShownRecord { last_shown_session: 5, shown_count: 1 },
        );
        history.shown.insert(
            "b".into(),
            super::TipShownRecord { last_shown_session: 5, shown_count: 1 },
        );
        // 间隔都是 7 < 冷却 10 → 无可选
        assert!(super::select_tip(&cands, &history).is_none());
        // 间隔达到冷却后，两条并列 → 优先级高者（b）
        history.session_counter = 15;
        assert_eq!(super::select_tip(&cands, &history).unwrap().id, "b");
    }

    #[test]
    fn test_scheduler_session_counter_and_record_roundtrip() {
        let dir = std::env::temp_dir().join(format!("cc-tips-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("spinner-tips.json");
        let _ = std::fs::remove_file(&path);
        let mut sched = super::TipScheduler::new(path.clone());
        assert_eq!(sched.session_index(), 0);
        sched.begin_session();
        assert_eq!(sched.session_index(), 1);
        sched.record_shown("tip-3");
        // 重新加载：计数与记录都在
        let sched2 = super::TipScheduler::new(path.clone());
        assert_eq!(sched2.session_index(), 1);
        let rec = sched2.history().shown.get("tip-3").unwrap();
        assert_eq!(rec.last_shown_session, 1);
        assert_eq!(rec.shown_count, 1);
        assert_eq!(sched2.history().sessions_since_shown("tip-3"), 0);
        assert_eq!(sched2.history().sessions_since_shown("never"), u64::MAX);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_override_candidates_parsing_and_dedup() {
        use cc_acp::provider::config::{SpinnerTipEntry, SpinnerTipsOverride};
        let cfg = SpinnerTipsOverride {
            tips: vec![
                SpinnerTipEntry::Text("喝水休息一下".into()),
                SpinnerTipEntry::Detailed {
                    id: "org-1".into(),
                    text: "组织提示".into(),
                    cooldown_sessions: 3,
                    priority: Some(7),
                },
                // 与首条同 id（文案即 id）→ 去重保留首条
                SpinnerTipEntry::Text("喝水休息一下".into()),
            ],
            tips_file: None,
            label: Some("提示".into()),
        };
        let cands = super::override_candidates(&cfg);
        assert_eq!(cands.len(), 2);
        assert_eq!(cands[0].text, "喝水休息一下");
        assert_eq!(cands[0].label.as_deref(), Some("提示"));
        assert_eq!(cands[1].id, "org-1");
        assert_eq!(cands[1].cooldown_sessions, 3);
        assert_eq!(cands[1].priority, 7);
    }

    #[test]
    fn test_tip_state_hold_and_publish() {
        let mut state = super::SpinnerTipState::default();
        assert!(state.current().is_none());
        // 轮开始挑选并扣留，本轮内不展示
        state.on_turn_start(|| Some(cand("a", 0, 0)));
        assert!(state.current().is_none());
        // 同轮重复调用不重复挑
        state.on_turn_start(|| Some(cand("b", 0, 0)));
        // 轮结束发布
        let published = state.on_turn_end();
        assert_eq!(published.as_deref(), Some("a"));
        assert_eq!(state.current().unwrap().id, "a");
        // 下一轮无候选 → 发布时清空
        state.on_turn_start(|| None);
        assert!(state.on_turn_end().is_none());
        assert!(state.current().is_none());
    }
