use super::App;

fn make_thread(cwd: &str) -> ThreadMeta {
    ThreadMeta::new(cwd)
}

fn make_app() -> App {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(App::new())
}

#[test]
fn filter_keeps_matching_cwd() {
    let cwd = "/Users/alice/project";
    let threads = vec![
        make_thread(cwd),
        make_thread("/Users/alice/other"),
        make_thread(cwd),
    ];
    let filtered: Vec<_> = threads.into_iter().filter(|t| t.cwd == cwd).collect();
    assert_eq!(filtered.len(), 2);
}

#[test]
fn filter_returns_empty_when_no_match() {
    let cwd = "/Users/alice/project";
    let threads = vec![
        make_thread("/Users/alice/other"),
        make_thread("/Users/bob/project"),
    ];
    let filtered: Vec<_> = threads.into_iter().filter(|t| t.cwd == cwd).collect();
    assert!(filtered.is_empty());
}

#[test]
fn filter_keeps_all_when_all_match() {
    let cwd = "/Users/alice/project";
    let threads = vec![make_thread(cwd), make_thread(cwd), make_thread(cwd)];
    let filtered: Vec<_> = threads.into_iter().filter(|t| t.cwd == cwd).collect();
    assert_eq!(filtered.len(), 3);
}

#[test]
fn test_scroll_up_from_follow_starts_at_bottom() {
    let mut app = make_app();
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.scroll_follow = true;
    ui.scrollbar_min_offset = 10;
    ui.scrollbar_max_offset = 20;
    app.scroll_up();
    let ui = &app.session_mgr.current().ui;
    assert_eq!(ui.scroll_offset, 17);
    assert!(!ui.scroll_follow);
}

#[test]
fn test_scroll_down_to_bottom_restores_follow() {
    let mut app = make_app();
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.scroll_follow = false;
    ui.scroll_offset = 18;
    ui.scrollbar_min_offset = 10;
    ui.scrollbar_max_offset = 20;
    app.scroll_down();
    let ui = &app.session_mgr.current().ui;
    assert_eq!(ui.scroll_offset, 20);
    assert!(ui.scroll_follow);
}

#[test]
fn test_scroll_up_does_not_cross_native_scrollback_boundary() {
    let mut app = make_app();
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.scroll_follow = false;
    ui.scroll_offset = 11;
    ui.scrollbar_min_offset = 10;
    ui.scrollbar_max_offset = 20;
    app.scroll_up();
    assert_eq!(app.session_mgr.current().ui.scroll_offset, 10);
}

#[test]
fn test_scroll_to_top_stops_at_native_scrollback_boundary() {
    let mut app = make_app();
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.scrollbar_min_offset = 10;
    ui.scrollbar_max_offset = 20;
    app.scroll_to_top();
    let ui = &app.session_mgr.current().ui;
    assert_eq!(ui.scroll_offset, 10);
    assert!(!ui.scroll_follow);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_open_thread_uses_target_project_for_history_and_pipeline() -> anyhow::Result<()> {
    let (mut app, _) = App::new_headless(100, 30).await;
    let project = tempfile::tempdir()?;
    let process_cwd = std::env::current_dir()?;
    let cwd = project.path().to_string_lossy().to_string();
    let id = app
        .services
        .thread_store
        .create_thread(ThreadMeta::new(&cwd))
        .await?;
    app.services
        .thread_store
        .append_message(&id, cc_agent::messages::BaseMessage::human("目标项目任务"))
        .await?;
    assert!(app.open_thread(id.clone()), "跨项目线程应可以打开");
    assert_eq!(app.services.cwd, cwd);
    assert_eq!(app.session_mgr.current().messages.pipeline.cwd(), cwd);
    assert_eq!(
        app.session_mgr.current().current_thread_id.as_ref(),
        Some(&id)
    );
    assert_eq!(
        app.session_mgr.current().agent.origin_messages[0].content(),
        "目标项目任务"
    );
    assert_eq!(std::env::current_dir()?, process_cwd, "不得修改进程目录");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_new_thread_in_project_keeps_empty_draft_out_of_store() -> anyhow::Result<()> {
    let (mut app, _) = App::new_headless(100, 30).await;
    let project = tempfile::tempdir()?;
    let count = app.services.thread_store.list_threads().await?.len();
    let cwd = project.path().to_string_lossy().to_string();
    app.services
        .git_branch_cache
        .lock()
        .set_status(Some(crate::app::GitBranchStatus {
            branch: "old-project".into(),
            dirty: false,
        }));
    let previous_query = app.services.git_branch_poller.lock().begin();
    assert!(app.new_thread_in_cwd(cwd.clone()));
    assert_eq!(app.services.cwd, cwd);
    assert_eq!(app.session_mgr.current().messages.pipeline.cwd(), cwd);
    assert!(app.session_mgr.current().current_thread_id.is_none());
    assert_eq!(
        app.services.thread_store.list_threads().await?.len(),
        count,
        "空白草稿不得新建存储记录"
    );
    assert!(app.services.git_branch_cache.lock().get_cached().is_none());
    assert!(
        previous_query.send(None).is_err(),
        "旧目录查询结果必须被丢弃"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_thread_switch_rejects_active_round_and_pending_approval() -> anyhow::Result<()> {
    let (mut app, _) = App::new_headless(100, 30).await;
    let project = tempfile::tempdir()?;
    let cwd = project.path().to_string_lossy().to_string();
    let original = app.services.cwd.clone();
    app.session_mgr.current_mut().ui.loading = true;
    assert!(!app.new_thread_in_cwd(cwd.clone()));
    assert_eq!(app.services.cwd, original);
    app.session_mgr.current_mut().ui.loading = false;
    app.session_mgr.current_mut().agent.pending_hitl_items = Some(vec!["Bash".into()]);
    assert!(!app.new_thread_in_cwd(cwd));
    assert_eq!(app.services.cwd, original);
    assert!(
        app.session_mgr.current().agent.pending_hitl_items.is_some(),
        "拒绝切换不得丢审批"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_open_thread_rejects_missing_project_without_changing_current_thread(
) -> anyhow::Result<()> {
    let (mut app, _) = App::new_headless(100, 30).await;
    let project = tempfile::tempdir()?;
    let original = app.services.cwd.clone();
    let missing = project.path().join("deleted-project");
    let id = app
        .services
        .thread_store
        .create_thread(ThreadMeta::new(missing.to_string_lossy()))
        .await?;
    assert!(!app.open_thread(id));
    assert_eq!(app.services.cwd, original);
    assert!(app.session_mgr.current().current_thread_id.is_none());
    Ok(())
}

#[tokio::test]
async fn test_thread_switch_only_blocks_active_background_agents() {
    use crate::app::subagent_session::{BackgroundAgentSession, SubagentRunStatus};
    let (mut app, _) = App::new_headless(100, 30).await;
    let mut agent = BackgroundAgentSession::new(
        "background".into(),
        "reviewer".into(),
        "检查".into(),
        app.services.cwd.clone(),
        "zh-CN",
    );
    agent.status = SubagentRunStatus::Completed;
    app.session_mgr
        .current_mut()
        .subagent_sessions
        .insert("background".into(), agent);
    app.session_mgr.current_mut().background_agents.push(
        crate::app::chat_session::RunningBgAgent {
            agent_name: "reviewer".into(),
            instance_id: "background".into(),
            started_at: std::time::Instant::now(),
            total_steps: 1,
        },
    );
    assert!(
        app.can_switch_thread_context(),
        "已完成任务的反馈行不得阻止切换"
    );
    app.session_mgr
        .current_mut()
        .subagent_sessions
        .get_mut("background")
        .unwrap()
        .status = SubagentRunStatus::AwaitingApproval;
    assert!(
        !app.can_switch_thread_context(),
        "子线程审批必须保留在原会话"
    );
    app.session_mgr
        .current_mut()
        .subagent_sessions
        .get_mut("background")
        .unwrap()
        .status = SubagentRunStatus::Running;
    assert!(!app.can_switch_thread_context(), "运行中子任务必须阻止切换");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_thread_switch_ignores_late_title_generation_from_previous_session(
) -> anyhow::Result<()> {
    let (mut app, _) = App::new_headless(100, 30).await;
    let project = tempfile::tempdir()?;
    let cwd = project.path().to_string_lossy().to_string();
    let previous_session = app.session_mgr.current().metadata.session_id;
    let mut meta = ThreadMeta::new(&cwd);
    meta.title = Some("目标会话标题".into());
    let id = app.services.thread_store.create_thread(meta).await?;
    assert!(app.open_thread(id.clone()));
    let opened_session = app.session_mgr.current().metadata.session_id;
    assert_ne!(
        opened_session, previous_session,
        "成功进入线程后必须轮换异步任务身份"
    );
    app.handle_agent_event(crate::app::AgentEvent::ThreadTitleGenerated {
        session_id: previous_session,
        thread_id: None,
        title: "旧会话迟到标题".into(),
    });
    assert_eq!(
        app.session_mgr.current().metadata.thread_title.as_deref(),
        Some("目标会话标题")
    );
    assert_eq!(
        app.services
            .thread_store
            .load_meta(&id)
            .await?
            .title
            .as_deref(),
        Some("目标会话标题"),
        "旧事件不得回退写入当前线程"
    );
    assert!(app.new_thread_in_cwd(cwd.clone()));
    assert_ne!(
        app.session_mgr.current().metadata.session_id,
        opened_session,
        "空白草稿同样必须轮换身份"
    );
    let mut meta = ThreadMeta::new(&cwd);
    meta.title = Some("新会话标题".into());
    let draft_id = app.services.thread_store.create_thread(meta).await?;
    app.session_mgr.current_mut().current_thread_id = Some(draft_id.clone());
    app.session_mgr.current_mut().metadata.thread_title = Some("新会话标题".into());
    app.handle_agent_event(crate::app::AgentEvent::ThreadTitleGenerated {
        session_id: opened_session,
        thread_id: None,
        title: "上个线程迟到标题".into(),
    });
    assert_eq!(
        app.session_mgr.current().metadata.thread_title.as_deref(),
        Some("新会话标题")
    );
    assert_eq!(
        app.services
            .thread_store
            .load_meta(&draft_id)
            .await?
            .title
            .as_deref(),
        Some("新会话标题")
    );
    Ok(())
}
#[tokio::test(flavor = "multi_thread")]
async fn test_new_thread_clears_image_index_without_deleting_files() -> anyhow::Result<()> {
    let (mut app, _) = App::new_headless(80, 24).await;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("image.png");
    std::fs::write(&path, b"image")?;
    let metadata = &mut app.session_mgr.current_mut().metadata;
    metadata.image_paths.insert(1, path.clone());
    metadata.next_image_id = 8;
    assert!(app.new_thread_in_cwd(directory.path().to_string_lossy().into_owned()));
    assert!(app.session_mgr.current().metadata.image_paths.is_empty());
    assert_eq!(app.session_mgr.current().metadata.next_image_id, 1);
    assert_eq!(
        std::fs::read(path)?,
        b"image",
        "清会话只能清索引，不能删除原图"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_open_thread_clears_image_paths_and_reserves_historical_identifiers(
) -> anyhow::Result<()> {
    let (mut app, _) = App::new_headless(80, 24).await;
    let directory = tempfile::tempdir()?;
    let cwd = directory.path().to_string_lossy().into_owned();
    let id = app
        .services
        .thread_store
        .create_thread(ThreadMeta::new(&cwd))
        .await?;
    app.services
        .thread_store
        .append_message(
            &id,
            cc_agent::messages::BaseMessage::human("旧图片 [Image #42]"),
        )
        .await?;
    app.session_mgr
        .current_mut()
        .metadata
        .image_paths
        .insert(42, directory.path().join("another-session.png"));
    assert!(app.open_thread(id), "历史会话应可以恢复");
    assert!(
        app.session_mgr.current().metadata.image_paths.is_empty(),
        "旧会话不能复用另一会话的路径"
    );
    assert_eq!(
        app.session_mgr.current_mut().metadata.alloc_image_id(),
        43,
        "新图片编号必须避开旧引用"
    );
    Ok(())
}
