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
