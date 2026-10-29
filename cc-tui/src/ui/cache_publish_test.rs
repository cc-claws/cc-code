use super::*;

fn make_render_task(width: u16, cache: RenderCache) -> RenderTask {
    RenderTask {
        last_messages: Vec::new(),
        message_lines: Vec::new(),
        message_links: Vec::new(),
        message_hashes: Vec::new(),
        cache: Arc::new(RwLock::new(cache)),
        notify: Arc::new(Notify::new()),
        width,
        show_tool_messages: false,
        diff_visible: false,
        hook_rows_visible: true,
        detail_mode: false,
        theme_fingerprint: theme::fingerprint(),
        alt_lines: Vec::new(),
        alt_links: Vec::new(),
        alt_keys: Vec::new(),
        alt_epochs: Vec::new(),
        line_epochs: Vec::new(),
        epoch_seq: 0,
        wrap_segs: [Vec::new(), Vec::new()],
    }
}

fn make_published_cache() -> RenderCache {
    let lines = vec![Line::from("previous owned line".to_owned())];
    let (total_lines, wrap_map) = RenderTask::build_wrap_map(&lines, 37);
    RenderCache {
        virtual_layout: Some(Arc::new(VirtualLayoutSnapshot::default())),
        lines,
        message_offsets: vec![0],
        total_lines,
        version: 41,
        wrap_map,
        links: vec![LinkHit {
            line: 0,
            g_start: 0,
            g_end: 8,
            url: "https://example.test/previous".to_owned(),
            recolor: true,
        }],
        width: 37,
        scroll_anchor: Some(7),
    }
}

fn make_finished_tool(url: &str) -> MessageViewModel {
    let mut message =
        MessageViewModel::tool_block("Bash".to_owned(), "Bash".to_owned(), None, false);
    if let MessageViewModel::ToolBlock { content, phase, .. } = &mut message {
        *content = url.to_owned();
        *phase = ToolPhase::Finished;
    }
    message.recompute_hash();
    message
}

#[test]
fn test_cache_publish_rebuild_preserves_pending_anchor_and_replaces_all_products() {
    let cache = make_published_cache();
    let previous_layout = Arc::downgrade(cache.virtual_layout.as_ref().expect("应有旧虚拟布局"));
    let mut task = make_render_task(40, cache);
    let messages = vec![
        MessageViewModel::user("中文内容与换行定位 ".repeat(8)),
        make_finished_tool("https://example.test/current"),
    ];
    task.rebuild(messages);
    let cache = task.cache.read();
    assert_eq!(cache.version, 42, "发布必须只推进一次版本");
    assert_eq!(cache.width, 40, "新帧必须使用当前排版宽度");
    assert_eq!(
        cache.scroll_anchor,
        Some(7),
        "未消费的滚动锚点不能因整帧替换丢失"
    );
    assert!(cache.virtual_layout.is_none(), "经典缓存不能保留旧虚拟布局");
    assert!(previous_layout.upgrade().is_none(), "发布后必须释放旧布局");
    assert_eq!(cache.message_offsets.len(), 2, "消息偏移必须完整替换");
    assert_eq!(cache.message_offsets[0], 0, "首条消息必须从缓存开头定位");
    assert_eq!(
        cache.lines.len(),
        cache.wrap_map.len(),
        "新行与换行条目必须对应"
    );
    assert_eq!(
        cache.wrap_map.last().map(|info| info.visual_row_end),
        Some(cache.total_lines),
        "发布行数必须与换行表一致"
    );
    assert!(
        cache.lines.iter().all(|line| !line
            .spans
            .iter()
            .any(|span| span.content.contains("previous owned line"))),
        "新帧不能残留旧行"
    );
    assert!(
        cache.links.iter().any(|link| {
            link.url == "https://example.test/current" && link.line < cache.lines.len()
        }),
        "新帧链接必须与新行坐标一起发布"
    );
    assert!(
        cache
            .links
            .iter()
            .all(|link| link.url != "https://example.test/previous"),
        "新帧不能残留旧链接"
    );
}

#[test]
fn test_cache_publish_clear_preserves_width_and_allows_fresh_rebuild() {
    let cache = make_published_cache();
    let previous_layout = Arc::downgrade(cache.virtual_layout.as_ref().expect("应有旧虚拟布局"));
    let mut task = make_render_task(24, cache);
    let (_sender, mut receiver) = mpsc::channel(1);
    task.handle_event(RenderEvent::Clear, &mut receiver);
    {
        let cache = task.cache.read();
        assert_eq!(cache.version, 42, "清空必须只推进一次版本");
        assert_eq!(cache.width, 37, "清空必须保留已发布宽度而非任务目标宽度");
        assert_eq!(cache.total_lines, 0, "清空必须发布零行");
        assert_eq!(cache.scroll_anchor, None, "清空必须移除旧滚动锚点");
        assert!(cache.virtual_layout.is_none(), "清空必须移除旧虚拟布局");
        assert!(cache.lines.is_empty(), "清空不能残留旧行");
        assert!(cache.message_offsets.is_empty(), "清空不能残留消息偏移");
        assert!(cache.wrap_map.is_empty(), "清空不能残留换行表");
        assert!(cache.links.is_empty(), "清空不能残留超链接");
    }
    assert!(previous_layout.upgrade().is_none(), "清空后必须释放旧布局");
    task.rebuild(vec![MessageViewModel::user("fresh content".to_owned())]);
    let cache = task.cache.read();
    assert_eq!(cache.version, 43, "清空后的重建必须继续推进版本");
    assert_eq!(cache.width, 24, "后续重建必须使用任务目标宽度");
    assert_eq!(cache.scroll_anchor, None, "后续重建不能恢复已清除的锚点");
    assert_eq!(cache.message_offsets, vec![0], "新历史偏移必须从零开始");
    assert!(cache.links.is_empty(), "新历史不能恢复已清除的链接");
    assert!(
        cache.lines.iter().any(|line| line
            .spans
            .iter()
            .any(|span| span.content.contains("fresh content"))),
        "清空后必须仍可发布新内容"
    );
}
