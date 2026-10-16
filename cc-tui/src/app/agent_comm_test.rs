use std::time::{Duration, Instant};

/// 构造一条最近工具条目
fn make_recent_entry(
    id: &str,
    display: &str,
    args_summary: &str,
    running: bool,
    started_at: Instant,
    visible_until: Option<Instant>,
) -> RecentToolEntry {
    RecentToolEntry {
        tool_call_id: id.to_string(),
        display: display.to_string(),
        args_summary: args_summary.to_string(),
        running,
        started_at,
        visible_until,
    }
}

#[test]
fn test_push_recent_tool_keeps_newest_first() {
    let now = Instant::now();
    let mut comm = AgentComm::default();
    comm.push_recent_tool(make_recent_entry("a", "Bash", "sleep 15", true, now, None));
    comm.push_recent_tool(make_recent_entry("b", "Read", "src/main.rs", true, now, None));
    let order: Vec<&str> = comm
        .recent_tools
        .iter()
        .map(|entry| entry.tool_call_id.as_str())
        .collect();
    assert_eq!(order, vec!["b", "a"]);
}

#[test]
fn test_push_recent_tool_truncates_to_max_visible() {
    let now = Instant::now();
    let mut comm = AgentComm::default();
    for id in ["a", "b", "c"] {
        comm.push_recent_tool(make_recent_entry(id, "Read", "x.rs", true, now, None));
    }
    let order: Vec<&str> = comm
        .recent_tools
        .iter()
        .map(|entry| entry.tool_call_id.as_str())
        .collect();
    assert_eq!(order, vec!["c", "b"]);
}

#[test]
fn test_finish_recent_tool_holds_fast_tool_until_min_visible() {
    let started = Instant::now();
    let now = started + Duration::from_millis(8);
    let mut comm = AgentComm::default();
    comm.push_recent_tool(make_recent_entry("a", "Read", "src/main.rs", true, started, None));
    comm.finish_recent_tool("a", now);
    let entry = comm.recent_tools.front().unwrap();
    assert!(!entry.running);
    assert_eq!(
        entry.visible_until,
        Some(started + Duration::from_millis(TOOL_MIN_VISIBLE_MS))
    );
    assert!(comm.has_visible_recent_tools(now));
}

#[test]
fn test_finish_recent_tool_removes_slow_tool_right_after_end() {
    let now = Instant::now();
    let started = now - Duration::from_millis(15_000);
    let mut comm = AgentComm::default();
    comm.push_recent_tool(make_recent_entry("a", "Bash", "sleep 15", true, started, None));
    comm.finish_recent_tool("a", now);
    let entry = comm.recent_tools.front().unwrap();
    assert_eq!(entry.visible_until, Some(now));
    assert!(!comm.has_visible_recent_tools(now));
}

#[test]
fn test_visible_recent_tools_filters_expired_and_keeps_running() {
    let now = Instant::now();
    let mut comm = AgentComm::default();
    comm.push_recent_tool(make_recent_entry(
        "done",
        "Read",
        "x.rs",
        false,
        now - Duration::from_secs(1),
        Some(now - Duration::from_millis(700)),
    ));
    comm.push_recent_tool(make_recent_entry("running", "Bash", "sleep 15", true, now, None));
    let visible: Vec<&str> = comm
        .visible_recent_tools(now)
        .map(|entry| entry.tool_call_id.as_str())
        .collect();
    assert_eq!(visible, vec!["running"]);
}

#[test]
fn test_finish_recent_tool_ignores_unknown_id() {
    let now = Instant::now();
    let mut comm = AgentComm::default();
    comm.push_recent_tool(make_recent_entry("a", "Read", "x.rs", true, now, None));
    comm.finish_recent_tool("not-exist", now);
    assert!(comm.recent_tools.front().unwrap().running);
}

/// 构造一条系统提示（最短可见时长自 `now` 起算）
fn make_system_notice(now: Instant) -> SystemNotice {
    SystemNotice::new(
        cc_agent::agent::events::NoticeLevel::Warn,
        "安全判定服务不可用，已降级到兜底分类器".to_string(),
        now,
    )
}

/// 构造一个已挂系统提示的 AgentComm（提示自 `now` 起算最短可见时长）
fn make_comm_with_notice(now: Instant) -> AgentComm {
    AgentComm {
        system_notice: Some(make_system_notice(now)),
        ..AgentComm::default()
    }
}

#[test]
fn test_system_notice_new_sets_visible_until_from_min_visible() {
    let now = Instant::now();
    let notice = make_system_notice(now);
    assert_eq!(
        notice.visible_until,
        now + Duration::from_millis(SYSTEM_NOTICE_MIN_VISIBLE_MS)
    );
    assert!(!notice.expired(now));
}

#[test]
fn test_system_notice_dismiss_before_min_visible_keeps_notice() {
    let now = Instant::now();
    let mut comm = make_comm_with_notice(now);
    comm.dismiss_system_notice(now + Duration::from_millis(100));
    assert!(comm.system_notice.is_some());
    assert!(!comm.expire_system_notice(now + Duration::from_millis(100)));
    assert!(comm.system_notice.is_some());
}

#[test]
fn test_system_notice_dismiss_after_min_visible_clears_notice() {
    let now = Instant::now();
    let mut comm = make_comm_with_notice(now);
    comm.dismiss_system_notice(now + Duration::from_millis(SYSTEM_NOTICE_MIN_VISIBLE_MS + 1));
    assert!(comm.system_notice.is_none());
}

#[test]
fn test_system_notice_expire_reports_change_after_min_visible() {
    let now = Instant::now();
    let mut comm = make_comm_with_notice(now);
    assert!(comm.expire_system_notice(now + Duration::from_millis(SYSTEM_NOTICE_MIN_VISIBLE_MS)));
    assert!(comm.system_notice.is_none());
    assert!(!comm.expire_system_notice(now + Duration::from_millis(SYSTEM_NOTICE_MIN_VISIBLE_MS)));
}

#[test]
fn test_clear_system_notice_ignores_min_visible() {
    let now = Instant::now();
    let mut comm = make_comm_with_notice(now);
    comm.clear_system_notice();
    assert!(comm.system_notice.is_none());
}
