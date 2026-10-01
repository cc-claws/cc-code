use super::*;
use crate::app::App;
use std::time::Duration;

fn make_status(branch: &str, dirty: bool) -> GitBranchStatus {
    GitBranchStatus {
        branch: branch.to_string(),
        dirty,
    }
}

#[test]
fn test_poller_due_on_first_call() {
    let mut poller = GitBranchPoller::new();
    assert!(poller.due(), "全新 poller 应立即需要探测");
    let _tx = poller.begin();
}

#[test]
fn test_poller_not_due_while_pending() {
    let mut poller = GitBranchPoller::new();
    let _tx = poller.begin();
    assert!(!poller.due(), "有探测在途时不应重复发起");
}

#[test]
fn test_poller_drain_clears_pending_and_returns_status() {
    let mut poller = GitBranchPoller::new();
    let tx = poller.begin();
    tx.send(Some(make_status("feat/x", false))).unwrap();

    let got = poller.drain().expect("应收到探测结果");
    assert_eq!(got.map(|s| s.branch), Some("feat/x".to_string()));
    assert!(
        !poller.due(),
        "刚完成一次探测，TTL 内不应再触发（pending 已清除但 last_request 仍在 TTL 内）"
    );
}

#[test]
fn test_poller_drain_returns_none_when_empty() {
    let mut poller = GitBranchPoller::new();
    assert!(poller.drain().is_none(), "无结果时 drain 应返回 None");
}

#[test]
fn test_poller_invalidate_makes_due_again() {
    let mut poller = GitBranchPoller::new();
    let tx = poller.begin();
    tx.send(Some(make_status("feat/x", false))).unwrap();
    poller.drain().unwrap();
    assert!(!poller.due(), "TTL 内不应触发");

    poller.invalidate();
    assert!(
        poller.due(),
        "invalidate 后应立即允许重新探测（轮末强制刷新）"
    );
}

#[test]
fn test_poller_drain_propagates_detection_failure() {
    let mut poller = GitBranchPoller::new();
    let tx = poller.begin();
    tx.send(None).unwrap(); // 模拟探测失败（非 git 目录等）

    let got = poller.drain().expect("应收到一条结果");
    assert!(got.is_none(), "失败结果应原样传出（用于清空缓存）");
}

#[test]
fn test_cache_set_and_get() {
    let mut cache = GitBranchCache::new();
    assert!(cache.get_cached().is_none(), "初始为空");

    cache.set_status(Some(make_status("main", true)));
    let s = cache.get_cached().expect("应有缓存");
    assert_eq!(s.branch, "main");
    assert!(s.dirty);

    cache.set_status(None);
    assert!(cache.get_cached().is_none(), "写回 None 应清空缓存");
}

/// 端到端：请求 → 结果回传 → 缓存更新，且能识别「变化」。
#[tokio::test]
async fn test_app_git_branch_async_refresh_updates_cache() {
    let (mut app, _handle) = App::new_headless(100, 30).await;

    // 初始无缓存，且应处于「该探测」状态
    assert!(app.services.git_branch_cache.lock().get_cached().is_none());

    // 直接走 poll 路径验证写回（不依赖真实 git 调用）
    {
        let tx = app.services.git_branch_poller.get_mut().begin();
        tx.send(Some(make_status("feat/async", true))).unwrap();
    }
    let updated = app.poll_git_branch_refresh();
    assert!(updated, "首次写回应报告有变化（需重绘）");

    {
        let cache = app.services.git_branch_cache.lock();
        let s = cache.get_cached().expect("缓存应已写入");
        assert_eq!(s.branch, "feat/async");
        assert!(s.dirty);
    }

    // 相同值再次写回 → 不应报告变化（避免无谓重绘）
    {
        let tx = app.services.git_branch_poller.get_mut().begin();
        tx.send(Some(make_status("feat/async", true))).unwrap();
    }
    assert!(!app.poll_git_branch_refresh(), "值未变化时不应触发重绘");
}

/// 关键回归：loading 期间不再冻结 —— 探测与写回全链路照常工作。
///
/// 旧实现（`status_bar.rs` 的 `if loading { get_cached() }`）在 Agent 工作期间
/// 完全冻结缓存，导致分支名长期陈旧（issue #277）。新实现把探测移到异步任务，
/// 与 `loading` 无关，故此处应能正常刷新。
#[tokio::test]
async fn test_app_git_branch_refresh_works_while_loading() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    app.session_mgr.current_mut().ui.loading = true;

    // loading 中：TTL 首次到达，应仍可发起探测
    assert!(
        app.services.git_branch_poller.get_mut().due(),
        "loading 期间也应允许发起探测"
    );

    // loading 中：结果回传后应照常写入缓存并报告变化
    {
        let tx = app.services.git_branch_poller.get_mut().begin();
        tx.send(Some(make_status("fix/during-loading", false)))
            .unwrap();
    }
    assert!(
        app.poll_git_branch_refresh(),
        "loading 期间收到结果也应更新缓存并触发重绘"
    );
    let branch = app
        .services
        .git_branch_cache
        .lock()
        .get_cached()
        .map(|s| s.branch.clone());
    assert_eq!(
        branch,
        Some("fix/during-loading".to_string()),
        "loading 期间分支名应能更新（旧实现在此不更新）"
    );
}

/// 轮末强制刷新：invalidate 后下一轮应重新探测（对齐 Codex turn-end refresh）。
#[tokio::test]
async fn test_app_turn_end_invalidates_git_branch_cache() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    {
        let tx = app.services.git_branch_poller.get_mut().begin();
        tx.send(Some(make_status("before-turn-end", false)))
            .unwrap();
    }
    app.poll_git_branch_refresh();
    assert!(
        !app.services.git_branch_poller.get_mut().due(),
        "TTL 内本不应重探"
    );

    app.invalidate_git_branch_cache();
    assert!(
        app.services.git_branch_poller.get_mut().due(),
        "轮末 invalidate 后应允许立即重探"
    );
}

#[test]
fn test_poller_ttl_constant_is_five_seconds() {
    // 锁定 TTL 语义，避免被无意改动；loading 冻结问题已由异步化解决，
    // TTL 仅决定刷新频率上限。
    assert_eq!(GitBranchPoller::TTL, Duration::from_secs(5));
}

/// 端到端（真实 git）：切换分支后 `detect_status` 应返回**新**分支名。
///
/// 这是 #277 的核心前提——「探测能反映实际分支」；上层异步链路（请求 → 回传 →
/// 写回）由上面的 mock 测试覆盖，二者合起来构成完整验证。
#[test]
fn test_detect_status_reflects_branch_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(path)
            .output()
            .expect("git 应可用");
        assert!(
            out.status.success(),
            "git {args:?} 失败: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };

    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    std::fs::write(path.join("a.txt"), "1").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "init"]);

    let cwd = path.to_str().unwrap();
    let before = GitBranchCache::detect_status(cwd).expect("初始应探测到分支");
    assert!(!before.dirty, "刚提交后工作区应干净");

    // 切换分支
    git(&["checkout", "-q", "-b", "feat/async-refresh"]);
    let after = GitBranchCache::detect_status(cwd).expect("切换后应探测到分支");
    assert_eq!(after.branch, "feat/async-refresh", "应反映新分支");
    assert_ne!(before.branch, after.branch, "分支名应发生变化");

    // 制造脏工作区
    std::fs::write(path.join("b.txt"), "2").unwrap();
    let dirty = GitBranchCache::detect_status(cwd).expect("仍应探测到分支");
    assert_eq!(dirty.branch, "feat/async-refresh");
    assert!(dirty.dirty, "未跟踪/未提交文件应标记 dirty");
}

/// 非 git 目录：探测应返回 None（不 panic、不报错）。
#[test]
fn test_detect_status_returns_none_outside_git_repo() {
    let dir = tempfile::tempdir().unwrap();
    let got = GitBranchCache::detect_status(dir.path().to_str().unwrap());
    assert!(got.is_none(), "非 git 目录应返回 None");
}
