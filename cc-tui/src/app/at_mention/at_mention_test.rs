use super::*;
use std::time::Duration;

fn make_candidate(path: &str) -> FileCandidate {
    FileCandidate {
        path: path.into(),
        display: path.into(),
        is_dir: false,
        score: 1,
    }
}

#[test]
fn test_detect_at_sign_at_start_and_after_newline() {
    assert_eq!(AtMentionState::detect("@", 1), Some((String::new(), 0)));
    assert_eq!(
        AtMentionState::detect("中文\n@", 8),
        Some((String::new(), 7))
    );
    assert_eq!(
        AtMentionState::detect("中文", 1),
        None,
        "非字符边界应安全返回"
    );
    assert_eq!(AtMentionState::detect("@", 2), None, "越界光标应安全返回");
}

#[test]
fn test_detect_does_not_open_for_email_prefix() {
    for text in ["user@", "user@example"] {
        assert_eq!(
            AtMentionState::detect(text, text.len()),
            None,
            "邮箱中的 @ 不应触发"
        );
    }
}

#[test]
fn test_activate_clears_stale_candidates_and_close_resets_searching() {
    let mut state = AtMentionState::new();
    state.activate(String::new(), 0);
    state.update_candidates(vec![make_candidate("old.rs")]);
    state.activate("new".into(), 0);
    assert!(state.active && state.searching, "新查询应显示搜索状态");
    assert!(state.selected_candidate().is_none(), "搜索时不能选择旧候选");
    state.close();
    assert!(!state.active && !state.searching, "关闭应清理搜索状态");
}

#[test]
fn test_cache_does_not_filter_truncated_or_default_candidates() {
    let mut state = AtMentionState::new();
    state.cache_result("", vec![make_candidate("root.rs")]);
    state.set_last_search_query("");
    assert!(
        state.try_filter_from_cache("nested").is_none(),
        "默认目录列表不能限制后续搜索"
    );
    state.cache_result(
        "file",
        (0..file_search::MAX_CANDIDATES)
            .map(|index| make_candidate(&format!("file_{index}.rs")))
            .collect(),
    );
    state.set_last_search_query("file");
    assert!(
        state.try_filter_from_cache("file_499").is_none(),
        "截断列表必须重新搜索"
    );
}

#[test]
fn test_cache_does_not_filter_when_entering_directory() {
    let mut state = AtMentionState::new();
    let mut directory = make_candidate("src");
    directory.is_dir = true;
    state.cache_result("s", vec![directory]);
    state.set_last_search_query("s");
    assert!(
        state.try_filter_from_cache("src/").is_none(),
        "进入目录应重新搜索，不能从目录名候选推断内容"
    );
}

#[test]
fn test_search_thread_keeps_latest_query() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("latest.rs"), "").unwrap();
    let (query_tx, query_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    query_tx.send("l".into()).unwrap();
    query_tx.send("la".into()).unwrap();
    query_tx.send("latest".into()).unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let worker = thread::spawn(move || search_thread_main(cwd, query_rx, result_tx));
    let (query, candidates) = result_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("最后一次查询应完成");
    assert_eq!(query, "latest", "快速输入必须使用最后一次查询");
    assert_eq!(candidates[0].path, "latest.rs");
    drop(query_tx);
    worker.join().expect("搜索线程应正常退出");
}

#[test]
fn test_detect_at_sign_with_text() {
    let text = "请看 @main";
    let result = AtMentionState::detect(text, text.len());
    assert!(result.is_some(), "应检测到 @ 提及");
    let (query, pos) = result.unwrap();
    assert_eq!(query, "main");
    assert_eq!(pos, "请看 ".len());
}

#[test]
fn test_detect_no_at_sign() {
    let result = AtMentionState::detect("hello world", "hello world".len());
    assert!(result.is_none(), "无 @ 应返回 None");
}

#[test]
fn test_detect_at_sign_only() {
    let result = AtMentionState::detect("看 @", "看 @".len());
    assert_eq!(
        result,
        Some((String::new(), "看 ".len())),
        "单独 @ 应立即触发"
    );
}

#[test]
fn test_detect_path_with_slash() {
    let text = "看 @src/main";
    let result = AtMentionState::detect(text, text.len());
    assert!(result.is_some());
    let (query, _) = result.unwrap();
    assert_eq!(query, "src/main");
}

#[test]
fn test_detect_not_at_line_start() {
    let result = AtMentionState::detect("user@example", "user@example".len());
    assert!(result.is_none(), "非空白前导的 @ 不应触发");
}

#[test]
fn test_move_up_down() {
    let mut state = AtMentionState::new();
    state.active = true;
    state.candidates = vec![
        FileCandidate {
            path: "a.rs".into(),
            display: "a.rs".into(),
            is_dir: false,
            score: 10,
        },
        FileCandidate {
            path: "b.rs".into(),
            display: "b.rs".into(),
            is_dir: false,
            score: 5,
        },
        FileCandidate {
            path: "c.rs".into(),
            display: "c.rs".into(),
            is_dir: false,
            score: 1,
        },
    ];
    assert_eq!(state.selected, 0);
    state.move_down();
    assert_eq!(state.selected, 1);
    state.move_down();
    assert_eq!(state.selected, 2);
    state.move_down();
    assert_eq!(state.selected, 0);
    state.move_up();
    assert_eq!(state.selected, 2);
}

#[test]
fn test_cache_hit() {
    let mut state = AtMentionState::new();
    let candidates = vec![FileCandidate {
        path: "main.rs".into(),
        display: "main.rs".into(),
        is_dir: false,
        score: 10,
    }];
    state.cache_result("main", candidates.clone());
    let cached = state.try_filter_from_cache("main");
    assert!(cached.is_some());
    assert_eq!(cached.unwrap().len(), 1);
}

#[test]
fn test_ensure_cwd_sets_once() {
    let mut state = AtMentionState::new();
    assert!(state.cwd.is_empty());
    state.ensure_cwd("/tmp".to_string());
    assert_eq!(state.cwd, "/tmp");
    state.ensure_cwd("/other".to_string());
    assert_eq!(state.cwd, "/tmp", "ensure_cwd 只设置一次");
}

#[test]
fn test_search_thread_idle_exit() {
    let mut state = AtMentionState::new();
    let dir = tempfile::tempdir().unwrap();
    state.set_cwd(dir.path().to_string_lossy().to_string());
    std::fs::write(dir.path().join("test.rs"), "").unwrap();

    state.start_search(String::new());
    assert!(!state.search_thread.as_ref().unwrap().is_finished());

    std::thread::sleep(Duration::from_millis(1200));
    assert!(
        state.search_thread.as_ref().unwrap().is_finished(),
        "线程应在 1s idle 后退出"
    );
}
