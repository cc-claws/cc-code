use std::sync::Arc;
use std::time::Duration;

use ratatui::{
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    Terminal,
};

use super::*;
use crate::thread::SqliteThreadStore;

async fn make_app() -> (App, tempfile::TempDir) {
    let (mut app, _handle) = App::new_headless(120, 30).await;
    let directory = tempfile::tempdir().unwrap();
    app.services.thread_store = Arc::new(
        SqliteThreadStore::new(directory.path().join("threads.db"))
            .await
            .unwrap(),
    );
    app.services.lc = crate::i18n::LcRegistry::new(Some("en"));
    app.services.cwd = "/project/current".into();
    (app, directory)
}

fn make_thread(cwd: &str, title: &str) -> ThreadMeta {
    let mut meta = ThreadMeta::new(cwd);
    meta.title = Some(title.into());
    meta
}

async fn make_center(app: &mut App, count: usize) {
    for index in 0..count {
        let meta = make_thread(&format!("/project/{}", index % 3), &format!("会话 {index}"));
        app.services.thread_store.create_thread(meta).await.unwrap();
    }
    app.open_agent_center();
    wait_for_center(app).await;
}

async fn wait_for_center(app: &mut App) {
    for _ in 0..2000 {
        app.poll_agent_center();
        if app
            .global_ui
            .agent_center
            .as_ref()
            .is_some_and(|center| center.page_rx.is_none() && center.mutation_rx.is_none())
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("中心异步操作超时");
}

fn press(app: &mut App, code: KeyCode) {
    assert!(app.handle_agent_center_key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn draw(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| crate::ui::main_ui::render(frame, app))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(width as usize)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn test_center_lists_threads_across_all_cwds() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 3).await;
    let center = app.global_ui.agent_center.as_ref().unwrap();
    assert_eq!(center.threads.len(), 3, "三个项目均可见");
    assert_eq!(center.counts(&app), [3, 0, 0, 3, 0]);
}

#[tokio::test]
async fn test_center_groups_by_cwd_and_counts() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 6).await;
    let screen = draw(&mut app, 120, 28);
    for index in 0..3 {
        assert!(
            screen.contains(&format!("/project/{index}  2")),
            "组头应显示完整路径与当组计数"
        );
    }
    assert_eq!(
        screen.lines().filter(|line| line.contains("──")).count(),
        1,
        "全页仅一条横线"
    );
    assert!(screen.contains("Task details"));
}

#[tokio::test]
async fn test_center_filter_tabs_update_counts() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 3).await;
    let id = app.global_ui.agent_center.as_ref().unwrap().threads[0]
        .id
        .clone();
    app.session_mgr.current_mut().current_thread_id = Some(id);
    app.session_mgr.current_mut().ui.loading = true;
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    let center = app.global_ui.agent_center.as_ref().unwrap();
    assert_eq!(center.counts(&app), [3, 0, 1, 2, 0]);
    assert_eq!(center.rows(&app).len(), 1);
    assert!(center.page_rx.is_none(), "切换过滤器不请求存储");
}

#[tokio::test]
async fn test_center_show_more_loads_next_page() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 43).await;
    assert_eq!(
        app.global_ui.agent_center.as_ref().unwrap().threads.len(),
        20
    );
    for _ in 0..20 {
        press(&mut app, KeyCode::Down);
    }
    let center = app.global_ui.agent_center.as_ref().unwrap();
    assert_eq!(
        center.rows(&app).get(center.selected),
        Some(&CenterRow::More),
        "向下能到达 Show more"
    );
    press(&mut app, KeyCode::Enter);
    wait_for_center(&mut app).await;
    assert_eq!(
        app.global_ui.agent_center.as_ref().unwrap().threads.len(),
        40
    );
    assert_eq!(
        app.global_ui.agent_center.as_ref().unwrap().catalog.len(),
        43
    );
}

#[tokio::test]
async fn test_center_catalog_refresh_realigns_pages_after_external_update() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 43).await;
    let center = app.global_ui.agent_center.as_ref().unwrap();
    let selected_id = center.selected_meta(&app).unwrap().id.clone();
    let mut promoted = center.catalog[25].clone();
    let promoted_id = promoted.id.clone();
    promoted.updated_at = chrono::Utc::now() + chrono::Duration::hours(1);
    app.services
        .thread_store
        .update_meta(&promoted_id, promoted)
        .await
        .unwrap();
    let catalog = app
        .services
        .thread_store
        .list_threads_metadata()
        .await
        .unwrap();
    let (tx, rx) = oneshot::channel();
    app.global_ui.agent_count_rx = Some(rx);
    tx.send(Ok(catalog)).unwrap();
    app.poll_agent_center();
    let center = app.global_ui.agent_center.as_ref().unwrap();
    assert_eq!(center.offset, 20, "目录变化后仍对应完整首屏");
    assert_eq!(
        center.threads[0].id, promoted_id,
        "未加载会话前移后立即可见"
    );
    assert_eq!(
        center.selected_meta(&app).unwrap().id,
        selected_id,
        "保留选中会话"
    );
    for expected in [40, 43] {
        let center = app.global_ui.agent_center.as_mut().unwrap();
        center.selected = center.threads.len();
        press(&mut app, KeyCode::Enter);
        wait_for_center(&mut app).await;
        let center = app.global_ui.agent_center.as_ref().unwrap();
        assert_eq!(center.threads.len(), expected, "下一页不得因重复行跳过会话");
        assert_eq!(center.offset, expected);
        let expected_ids: Vec<_> = center
            .catalog
            .iter()
            .take(expected)
            .map(|meta| &meta.id)
            .collect();
        let actual_ids: Vec<_> = center.threads.iter().map(|meta| &meta.id).collect();
        assert_eq!(actual_ids, expected_ids, "已加载页始终是目录连续前缀");
    }
}

#[tokio::test]
async fn test_center_page_result_realigns_after_concurrent_catalog_reorder() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 23).await;
    let center = app.global_ui.agent_center.as_ref().unwrap();
    let old_page = center.catalog[10..20].to_vec();
    let mut catalog = center.catalog.clone();
    let promoted = catalog.remove(22);
    let promoted_id = promoted.id.clone();
    catalog.insert(0, promoted);
    let (tx, rx) = oneshot::channel();
    app.global_ui.agent_center.as_mut().unwrap().page_rx = Some(rx);
    tx.send(Ok(CenterPage {
        catalog,
        threads: old_page,
        offset: 10,
    }))
    .ok();
    app.poll_agent_center();
    let center = app.global_ui.agent_center.as_ref().unwrap();
    assert_eq!(center.offset, 20);
    assert_eq!(center.threads.len(), 20);
    assert_eq!(
        center.threads[0].id, promoted_id,
        "页请求进行期间前移的会话不能遗漏"
    );
    assert_eq!(center.threads[19].id, center.catalog[19].id);
}

#[test]
fn test_center_cursor_stays_visible_while_scrolling() {
    let mut scroll = 0;
    for selected in 0..60 {
        render::keep_visible(&mut scroll, selected, 8);
        assert!(
            selected >= scroll && selected < scroll + 8,
            "选中行始终在视口内"
        );
    }
    render::keep_visible(&mut scroll, 0, 8);
    assert_eq!(scroll, 0);
}

#[test]
fn test_center_status_bucket_maps_agent_status() {
    let now = chrono::Utc::now();
    let mut meta = make_thread("/project", "旧会话");
    for status in ["done", "cancelled", "error"] {
        meta.agent_status = status.into();
        assert_eq!(Bucket::from_meta(&meta, now), Bucket::Inactive);
    }
    meta.agent_status = "active".into();
    meta.updated_at = now - chrono::Duration::hours(25);
    assert_eq!(Bucket::from_meta(&meta, now), Bucket::Inactive);
    meta.updated_at = now - chrono::Duration::hours(1);
    assert_eq!(Bucket::from_meta(&meta, now), Bucket::Ready);
}

#[tokio::test]
async fn test_center_detail_shows_unknown_for_legacy_threads() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 1).await;
    let screen = draw(&mut app, 120, 28);
    for label in [
        "Model: Unknown",
        "Reasoning: Unknown",
        "Branch",
        "Last message",
        "Prompt",
    ] {
        assert!(screen.contains(label), "历史缺失字段仍保留详情占位");
    }
    let narrow = draw(&mut app, 70, 18);
    assert!(!narrow.contains("Task details"), "窄屏隐藏右栏");
}

#[tokio::test]
async fn test_agents_key_requires_empty_draft_and_ignores_repeat() {
    let (mut app, _directory) = make_app().await;
    app.session_mgr.current_mut().ui.textarea.insert_str("ab");
    assert!(!app.handle_agent_center_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)));
    app.session_mgr.current_mut().ui.textarea = super::super::build_textarea(false);
    let mut repeat = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
    repeat.kind = KeyEventKind::Repeat;
    assert!(!app.handle_agent_center_key(repeat));
    assert!(app.global_ui.agent_center.is_none());
    app.focused = false;
    assert!(!app.handle_agent_center_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)));
    app.focused = true;
    assert!(app.handle_agent_center_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)));
    assert!(app.global_ui.agent_center.is_some());
}

#[tokio::test]
async fn test_agents_key_toggles_help_and_esc_closes() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 1).await;
    press(&mut app, KeyCode::Char('?'));
    assert!(app.global_ui.agent_center.as_ref().unwrap().help);
    press(&mut app, KeyCode::Esc);
    assert!(!app.global_ui.agent_center.as_ref().unwrap().help);
    press(&mut app, KeyCode::Esc);
    assert!(app.global_ui.agent_center.is_none());
}

#[tokio::test]
async fn test_center_rename_and_search_handle_chinese_paste() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 2).await;
    let id = app
        .global_ui
        .agent_center
        .as_ref()
        .unwrap()
        .selected_meta(&app)
        .unwrap()
        .id
        .clone();
    press(&mut app, KeyCode::Char('r'));
    app.global_ui.agent_center.as_mut().unwrap().rename = Some(String::new());
    app.paste_agent_center_text("中文新名字\r\n");
    press(&mut app, KeyCode::Enter);
    wait_for_center(&mut app).await;
    assert_eq!(
        app.services
            .thread_store
            .load_meta(&id)
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("中文新名字")
    );
    press(&mut app, KeyCode::Char('/'));
    app.paste_agent_center_text("中文新");
    let center = app.global_ui.agent_center.as_ref().unwrap();
    assert_eq!(center.rows(&app).len(), 1);
    assert!(
        app.session_mgr
            .current()
            .ui
            .textarea
            .lines()
            .iter()
            .all(String::is_empty),
        "粘贴不穿透主编辑器"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delete_thread_confirmed_removes_row_and_messages() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 1).await;
    let id = app.global_ui.agent_center.as_ref().unwrap().threads[0]
        .id
        .clone();
    app.services
        .thread_store
        .append_messages(&id, &[cc_agent::messages::BaseMessage::human("待删除")])
        .await
        .unwrap();
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Esc);
    assert!(
        app.services.thread_store.load_meta(&id).await.is_ok(),
        "取消确认保留数据"
    );
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Enter);
    assert!(
        app.services.thread_store.load_meta(&id).await.is_ok(),
        "Enter 默认不能误删"
    );
    press(&mut app, KeyCode::Char('y'));
    wait_for_center(&mut app).await;
    assert!(app.services.thread_store.load_meta(&id).await.is_err());
    assert!(app
        .services
        .thread_store
        .load_messages(&id)
        .await
        .unwrap()
        .is_empty());
    assert!(app
        .global_ui
        .agent_center
        .as_ref()
        .unwrap()
        .threads
        .is_empty());
}

#[test]
fn test_tasks_command_not_registered() {
    let registry = crate::command::default_registry();
    assert!(registry.find("tasks").is_none());
    assert_eq!(registry.find("agents").unwrap().name(), "agents");
    assert_eq!(
        registry.find("agent-design").unwrap().name(),
        "agent-design"
    );
}

#[tokio::test]
async fn test_enter_closes_center_and_left_reopens() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 1).await;
    let id = app.global_ui.agent_center.as_ref().unwrap().threads[0]
        .id
        .clone();
    app.session_mgr.current_mut().current_thread_id = Some(id);
    app.session_mgr.current_mut().ui.loading = true;
    press(&mut app, KeyCode::Enter);
    assert!(
        app.global_ui.agent_center.is_none(),
        "回到当前会话不重载或中断任务"
    );
    assert!(app.session_mgr.current().ui.loading);
    press(&mut app, KeyCode::Left);
    assert!(app.global_ui.agent_center.is_some());
}

#[tokio::test]
async fn test_center_rename_keeps_original_identity_when_selection_changes() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 2).await;
    let id = app
        .global_ui
        .agent_center
        .as_ref()
        .unwrap()
        .selected_meta(&app)
        .unwrap()
        .id
        .clone();
    press(&mut app, KeyCode::Char('r'));
    let center = app.global_ui.agent_center.as_mut().unwrap();
    center.rename = Some("固定身份".into());
    center.selected = 1;
    press(&mut app, KeyCode::Enter);
    wait_for_center(&mut app).await;
    assert_eq!(
        app.services
            .thread_store
            .load_meta(&id)
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("固定身份")
    );
}

#[test]
fn test_center_detail_uses_first_prompt_and_last_visible_message() {
    let detail = state::Detail::from_messages(&[
        cc_agent::messages::BaseMessage::system("私有提示词"),
        cc_agent::messages::BaseMessage::human("首个问题"),
        cc_agent::messages::BaseMessage::ai("最后回答"),
        cc_agent::messages::BaseMessage::system("内部注入"),
    ]);
    assert_eq!(detail.prompt, "首个问题");
    assert_eq!(detail.last, "最后回答");
}

#[test]
fn test_center_catalog_refresh_removes_deleted_rows_and_updates_snapshots() {
    let mut center = AgentCenter::new();
    let first = make_thread("/project", "旧标题");
    let second = make_thread("/project", "已删除");
    center.threads = vec![first.clone(), second];
    center.offset = 2;
    let mut fresh = first;
    fresh.title = Some("新标题".into());
    fresh.agent_status = "done".into();
    center.refresh_catalog(vec![fresh]);
    assert_eq!(center.threads.len(), 1, "外部删除不会留下幽灵行");
    assert_eq!(center.offset, 1, "删除已加载行后修正分页偏移");
    assert_eq!(center.threads[0].title.as_deref(), Some("新标题"));
    assert_eq!(center.threads[0].agent_status, "done");
}

#[tokio::test]
async fn test_center_long_metadata_keeps_detail_labels_visible() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 1).await;
    let center = app.global_ui.agent_center.as_mut().unwrap();
    center.threads[0].title = Some("长标题".repeat(100));
    center.threads[0].cwd = format!("/project/{}", "very-long-path/".repeat(50));
    center.threads[0].model = Some("long-model-name".repeat(50));
    center.threads[0].branch = Some("long-branch-name".repeat(50));
    let screen = draw(&mut app, 120, 24);
    for label in ["Reasoning:", "Project", "Branch", "Last message", "Prompt"] {
        assert!(screen.contains(label), "长字段不能把必要详情项挤出视口");
    }
}

/// 详情面板对齐状态栏排版：键值同行（`Label: value`），分支用 `git:(branch)` 形态。
#[tokio::test]
async fn test_center_detail_renders_labels_and_values_on_one_line() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 1).await;
    {
        let center = app.global_ui.agent_center.as_mut().unwrap();
        center.threads[0].cwd = "/project/current".into();
        center.threads[0].model = Some("claude-fable-5".into());
        center.threads[0].reasoning_effort = Some("xhigh".into());
        center.threads[0].branch = Some("main".into());
    }
    let screen = draw(&mut app, 120, 24);
    assert!(
        screen.contains("Model: claude-fable-5"),
        "模型标签与值必须同行: {screen}"
    );
    assert!(
        screen.contains("Project: /project/current"),
        "项目标签与值必须同行: {screen}"
    );
    assert!(
        screen.contains("Branch: git:(main)"),
        "分支必须沿用状态栏 git:(...) 形态: {screen}"
    );
}

/// 弹层底色必须用主题的 `popup_bg`，不能拿品牌色 `background`（暗色主题下是青绿）铺满。
#[tokio::test]
async fn test_center_delete_overlay_uses_popup_background() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 1).await;
    press(&mut app, KeyCode::Delete);

    let width = 120u16;
    let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
    terminal
        .draw(|frame| crate::ui::main_ui::render(frame, &mut app))
        .unwrap();
    let backgrounds: std::collections::HashSet<_> = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.bg)
        .collect();
    let palette = crate::ui::theme::cur();
    assert!(
        backgrounds.contains(&palette.popup_bg),
        "弹层底应使用 popup_bg，实际出现的背景色: {backgrounds:?}"
    );
    assert!(
        !backgrounds.contains(&palette.background),
        "不得把品牌色 background 当弹层底: {backgrounds:?}"
    );
}

#[tokio::test]
async fn test_center_delete_warning_and_help_remain_visible_for_long_title() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 1).await;
    app.global_ui.agent_center.as_mut().unwrap().threads[0].title = Some("长标题".repeat(300));
    press(&mut app, KeyCode::Delete);
    let screen = draw(&mut app, 120, 24);
    assert!(
        screen.contains("cannot be undone"),
        "删除确认必须保留不可恢复提示"
    );
    press(&mut app, KeyCode::Char('?'));
    let help = draw(&mut app, 120, 24);
    assert!(help.contains("Navigate"), "帮助必须显示在删除确认上层");
    press(&mut app, KeyCode::Esc);
    assert!(
        app.global_ui
            .agent_center
            .as_ref()
            .unwrap()
            .confirm_delete
            .is_some(),
        "退出帮助回到确认层"
    );
}

#[tokio::test]
async fn test_center_manual_rename_locks_title_before_async_completion() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 1).await;
    let id = app.global_ui.agent_center.as_ref().unwrap().threads[0]
        .id
        .clone();
    app.session_mgr.current_mut().current_thread_id = Some(id);
    press(&mut app, KeyCode::Char('r'));
    app.global_ui.agent_center.as_mut().unwrap().rename = Some("手工标题".into());
    press(&mut app, KeyCode::Enter);
    assert!(
        app.session_mgr.current().metadata.user_renamed,
        "写入未完成时也应阻止自动标题覆盖"
    );
    press(&mut app, KeyCode::Esc);
    assert!(
        app.global_ui.agent_center.is_some(),
        "保留在途写入的错误与成功接收器"
    );
    wait_for_center(&mut app).await;
    assert_eq!(
        app.session_mgr.current().metadata.thread_title.as_deref(),
        Some("手工标题")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_center_open_and_new_use_selected_project_directory() {
    let (mut app, _directory) = make_app().await;
    let project = tempfile::tempdir().unwrap();
    let cwd = project.path().to_string_lossy().to_string();
    let id = app
        .services
        .thread_store
        .create_thread(make_thread(&cwd, "目标项目"))
        .await
        .unwrap();
    app.open_agent_center();
    wait_for_center(&mut app).await;
    press(&mut app, KeyCode::Enter);
    assert!(app.global_ui.agent_center.is_none());
    assert_eq!(app.services.cwd, cwd, "Enter 使用所选线程的目录");
    assert_eq!(
        app.session_mgr.current().current_thread_id.as_deref(),
        Some(id.as_str())
    );
    app.services.cwd = "/project/previous".into();
    app.open_agent_center();
    wait_for_center(&mut app).await;
    press(&mut app, KeyCode::Char('n'));
    assert!(app.global_ui.agent_center.is_none());
    assert_eq!(app.services.cwd, cwd, "n 使用选中行的目录");
    assert!(app.session_mgr.current().current_thread_id.is_none());
    assert_eq!(
        app.services
            .thread_store
            .list_threads_metadata()
            .await
            .unwrap()
            .len(),
        1,
        "空白草稿不插入线程行"
    );
}

#[tokio::test]
async fn test_center_manual_rename_wins_over_queued_automatic_title_for_other_thread() {
    let (mut app, _directory) = make_app().await;
    make_center(&mut app, 1).await;
    let id = app.global_ui.agent_center.as_ref().unwrap().threads[0]
        .id
        .clone();
    let write_lock = app.global_ui.thread_title_write_lock.clone();
    let guard = write_lock.lock().await;
    app.handle_agent_event(crate::app::AgentEvent::ThreadTitleGenerated {
        session_id: uuid::Uuid::nil(),
        thread_id: Some(id.clone()),
        title: "迟到自动标题".into(),
    });
    press(&mut app, KeyCode::Char('r'));
    app.global_ui.agent_center.as_mut().unwrap().rename = Some("手工标题优先".into());
    press(&mut app, KeyCode::Enter);
    drop(guard);
    wait_for_center(&mut app).await;
    assert_eq!(
        app.services
            .thread_store
            .load_meta(&id)
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("手工标题优先")
    );
    app.handle_agent_event(crate::app::AgentEvent::ThreadTitleGenerated {
        session_id: uuid::Uuid::nil(),
        thread_id: Some(id.clone()),
        title: "再次迟到的标题".into(),
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(
        app.services
            .thread_store
            .load_meta(&id)
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("手工标题优先"),
        "切换后的历史会话也保留手工命名"
    );
}

/// 状态栏 `← {n} agents` 的口径：当前目录 + `Ready`（24h 内有更新）的根会话。
///
/// 覆盖三个排除维度：其他目录、超过 24h 未更新、子 agent（非根）。
#[tokio::test]
async fn test_current_project_ready_count_filters_cwd_bucket_and_root() {
    let (mut app, _directory) = make_app().await;
    let now = chrono::Utc::now();

    let mut ready = make_thread("/project/current", "当前目录活跃");
    ready.updated_at = now - chrono::Duration::hours(1);

    let mut stale = make_thread("/project/current", "当前目录陈旧");
    stale.updated_at = now - chrono::Duration::hours(25);

    let mut other_project = make_thread("/project/other", "别的目录活跃");
    other_project.updated_at = now - chrono::Duration::hours(1);

    let mut child = make_thread("/project/current", "当前目录的子 agent");
    child.parent_thread_id = Some("parent-thread".into());
    child.updated_at = now - chrono::Duration::hours(1);

    app.global_ui.agent_catalog = vec![ready, stale, other_project, child];

    assert_eq!(
        app.current_project_ready_count(),
        1,
        "只应统计当前目录下 24h 内有更新的根会话"
    );
}
