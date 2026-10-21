use std::{sync::Arc, time::Duration};

use parking_lot::RwLock;
use tokio::sync::{mpsc, Notify};

use crate::ui::{
    message_view::MessageViewModel,
    render_thread::{spawn_render_thread, RenderCache, RenderEvent, SelectionAnchor},
};

async fn make_wait_for_version(
    cache: &Arc<RwLock<RenderCache>>,
    notify: &Arc<Notify>,
    minimum: u64,
) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let notified = notify.notified();
            if cache.read().version >= minimum {
                return;
            }
            notified.await;
        }
    })
    .await
    .expect("渲染线程应按控制事件推进版本，不得丢事件或停止响应");
}

fn make_source_anchor(cache: &RenderCache, message_index: usize) -> SelectionAnchor {
    let layout = cache.virtual_layout.as_ref().expect("应处于虚拟模式");
    SelectionAnchor {
        history_epoch: layout.history_epoch,
        message_index,
        message_version: layout.message_versions[message_index],
        logical_line: 0,
        grapheme_offset: 0,
    }
}

async fn make_copy_reply(
    tx: &mpsc::Sender<RenderEvent>,
    anchor: SelectionAnchor,
) -> Option<String> {
    let (reply, receive) = tokio::sync::oneshot::channel();
    tx.send(RenderEvent::CopySelection {
        start: anchor,
        end: SelectionAnchor {
            grapheme_offset: 100,
            ..anchor
        },
        reply,
    })
    .await
    .expect("复制请求应入队");
    tokio::time::timeout(Duration::from_secs(10), receive)
        .await
        .expect("复制控制请求必须得到响应")
        .expect("复制响应通道不能中途关闭")
}

#[tokio::test]
async fn test_worker_auto_promotes_anchored_rebuild_without_rendering_full_history() {
    let (tx, cache, notify) = spawn_render_thread(80);
    tx.send(RenderEvent::Viewport {
        top: 0,
        height: 8,
        follow_tail: true,
        anchor: None,
    })
    .await
    .expect("视口应入队");
    tx.send(RenderEvent::ToggleDetail(true))
        .await
        .expect("详情开关应入队");
    let mut messages: Vec<_> = (0..30)
        .map(|index| {
            MessageViewModel::system_with_detail(
                format!("history {index}"),
                "offscreen history\n".repeat(10_000),
            )
        })
        .collect();
    messages.push(MessageViewModel::system(
        (0..40)
            .map(|index| format!("tail line {index}\n"))
            .collect(),
    ));
    tx.send(RenderEvent::RebuildWithAnchor {
        messages,
        anchor_message_idx: 30,
    })
    .await
    .expect("带锚点重建应入队");
    make_wait_for_version(&cache, &notify, 1).await;
    {
        let output = cache.read();
        let layout = output
            .virtual_layout
            .as_ref()
            .expect("auto 必须从带锚点重建识别三十万行详情并升档");
        assert!(
            layout.diagnostics.resident_segments <= 2,
            "只能物化尾部窗口"
        );
        assert!(
            layout.diagnostics.estimated_segments >= 29,
            "历史应保持未物化"
        );
        assert!(output.lines.len() < 80, "共享输出不得包含全量历史成品");
        assert!(output
            .lines
            .iter()
            .any(|line| line.to_string().contains("tail line 39")));
    }
    let epoch = cache.read().virtual_layout.as_ref().unwrap().history_epoch;
    let mut smaller = vec![MessageViewModel::system_with_detail(
        "history 0".to_owned(),
        "offscreen history\n".repeat(10_000),
    )];
    smaller.extend((1..31).map(|index| MessageViewModel::system(format!("smaller {index}"))));
    let version = cache.read().version;
    tx.send(RenderEvent::RebuildWithAnchor {
        messages: smaller,
        anchor_message_idx: 30,
    })
    .await
    .expect("同一代际的内容更新应入队");
    make_wait_for_version(&cache, &notify, version + 1).await;
    {
        let output = cache.read();
        let layout = output
            .virtual_layout
            .as_ref()
            .expect("同一代际不能因估算下降来回切档");
        assert_eq!(
            layout.history_epoch, epoch,
            "稳定首条和消息数不应误判历史替换"
        );
    }
    let version = cache.read().version;
    tx.send(RenderEvent::RebuildWithAnchor {
        messages: vec![MessageViewModel::system("compact replacement".to_owned())],
        anchor_message_idx: 0,
    })
    .await
    .expect("压缩后的替换历史应入队");
    make_wait_for_version(&cache, &notify, version + 1).await;
    assert!(
        cache.read().virtual_layout.is_none(),
        "历史缩短后 auto 必须重新判定而非永久锁定虚拟模式"
    );
}

#[tokio::test]
async fn test_worker_strategy_round_trip_releases_old_virtual_residents() {
    let (tx, cache, notify) = spawn_render_thread(80);
    tx.send(RenderEvent::SetDetailStrategy("virtual".to_owned()))
        .await
        .expect("策略应入队");
    tx.send(RenderEvent::Viewport {
        top: 0,
        height: 3,
        follow_tail: true,
        anchor: None,
    })
    .await
    .expect("视口应入队");
    tx.send(RenderEvent::Rebuild(
        (0..20)
            .map(|index| MessageViewModel::system(format!("block {index}\n").repeat(12)))
            .collect(),
    ))
    .await
    .expect("历史应入队");
    make_wait_for_version(&cache, &notify, 1).await;
    let version = cache.read().version;
    tx.send(RenderEvent::Viewport {
        top: 100,
        height: 3,
        follow_tail: false,
        anchor: Some(crate::ui::render_thread::ViewportAnchor {
            message_index: 10,
            logical_line: 0,
            row_in_line: 0,
        }),
    })
    .await
    .expect("中段视口应入队");
    make_wait_for_version(&cache, &notify, version + 1).await;
    let version = cache.read().version;
    tx.send(RenderEvent::Viewport {
        top: 0,
        height: 3,
        follow_tail: false,
        anchor: None,
    })
    .await
    .expect("头部视口应入队");
    make_wait_for_version(&cache, &notify, version + 1).await;
    let resident_count = cache
        .read()
        .virtual_layout
        .as_ref()
        .expect("应处于虚拟模式")
        .diagnostics
        .resident_segments;
    assert!(resident_count >= 2, "应先积累头尾两个互不重叠的成品");
    let version = cache.read().version;
    tx.send(RenderEvent::SetDetailStrategy("cache".to_owned()))
        .await
        .expect("全量回退应入队");
    make_wait_for_version(&cache, &notify, version + 1).await;
    {
        let output = cache.read();
        assert!(output.virtual_layout.is_none(), "cache 必须回退到兼容路径");
        assert!(output
            .lines
            .iter()
            .any(|line| line.to_string().contains("block 19")));
        assert!(output.lines.len() > 100, "cache 应发布全量成品");
    }
    let version = cache.read().version;
    tx.send(RenderEvent::SetDetailStrategy("virtual".to_owned()))
        .await
        .expect("重新进入虚拟模式应入队");
    make_wait_for_version(&cache, &notify, version + 1).await;
    let output = cache.read();
    let layout = output.virtual_layout.as_ref().expect("应恢复虚拟模式");
    assert!(
        layout.diagnostics.resident_segments < resident_count,
        "回退必须释放旧居民而非保留头尾两份"
    );
    assert!(
        layout.line_origins.iter().all(|(index, _)| *index == 0),
        "只应重新物化当前头部窗口"
    );
    assert!(
        !output
            .lines
            .iter()
            .any(|line| line.to_string().contains("block 19")),
        "窗口中不得保留离屏旧成品"
    );
}

#[tokio::test]
async fn test_worker_clear_removes_urls_and_invalidates_selection_epoch() {
    let (tx, cache, notify) = spawn_render_thread(80);
    tx.send(RenderEvent::SetDetailStrategy("virtual".to_owned()))
        .await
        .unwrap();
    tx.send(RenderEvent::Rebuild(vec![
        MessageViewModel::user("[old](https://old.example/clear)".to_owned()),
        MessageViewModel::system("old tail".to_owned()),
    ]))
    .await
    .unwrap();
    make_wait_for_version(&cache, &notify, 1).await;
    let old_anchor = {
        let output = cache.read();
        assert!(output
            .links
            .iter()
            .any(|hit| hit.url == "https://old.example/clear"));
        make_source_anchor(&output, 0)
    };
    let version = cache.read().version;
    tx.send(RenderEvent::SetDetailStrategy("cache".to_owned()))
        .await
        .unwrap();
    make_wait_for_version(&cache, &notify, version + 1).await;
    assert!(
        cache.read().virtual_layout.is_none(),
        "同时覆盖全量回退后的 Clear 清理"
    );
    let version = cache.read().version;
    tx.send(RenderEvent::Clear).await.unwrap();
    make_wait_for_version(&cache, &notify, version + 1).await;
    {
        let output = cache.read();
        assert!(
            output.lines.is_empty() && output.wrap_map.is_empty() && output.links.is_empty(),
            "Clear 必须释放旧文本、换行和URL"
        );
        assert!(output.message_offsets.is_empty() && output.virtual_layout.is_none());
        assert_eq!(output.total_lines, 0);
        assert_eq!(output.scroll_anchor, None, "不能保留清空前的滚动锚点");
    }
    assert_eq!(
        make_copy_reply(&tx, old_anchor).await,
        None,
        "清空后必须拒绝旧选区"
    );
    let version = cache.read().version;
    tx.send(RenderEvent::SetDetailStrategy("virtual".to_owned()))
        .await
        .unwrap();
    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "[new](https://new.example/clear)".to_owned(),
    )]))
    .await
    .unwrap();
    make_wait_for_version(&cache, &notify, version + 1).await;
    {
        let output = cache.read();
        assert_ne!(
            output.virtual_layout.as_ref().unwrap().history_epoch,
            old_anchor.history_epoch,
            "Clear 后的新历史必须使用新代际"
        );
        assert!(output
            .links
            .iter()
            .any(|hit| hit.url == "https://new.example/clear"));
        assert!(!output
            .links
            .iter()
            .any(|hit| hit.url == "https://old.example/clear"));
    }
    assert_eq!(
        make_copy_reply(&tx, old_anchor).await,
        None,
        "重建新历史后仍不得接受旧选区"
    );
}

#[tokio::test]
async fn test_worker_history_replacement_discards_old_links_and_selection() {
    let (tx, cache, notify) = spawn_render_thread(80);
    tx.send(RenderEvent::SetDetailStrategy("virtual".to_owned()))
        .await
        .unwrap();
    tx.send(RenderEvent::Rebuild(vec![
        MessageViewModel::user("[previous](https://old.example/rewind)".to_owned()),
        MessageViewModel::system("previous tail".to_owned()),
    ]))
    .await
    .unwrap();
    make_wait_for_version(&cache, &notify, 1).await;
    let old_anchor = make_source_anchor(&cache.read(), 0);
    let version = cache.read().version;
    tx.send(RenderEvent::RebuildWithAnchor {
        messages: vec![MessageViewModel::user(
            "[replacement](https://new.example/rewind)".to_owned(),
        )],
        anchor_message_idx: 0,
    })
    .await
    .unwrap();
    make_wait_for_version(&cache, &notify, version + 1).await;
    {
        let output = cache.read();
        let layout = output
            .virtual_layout
            .as_ref()
            .expect("显式 virtual 不应随历史缩短退出");
        assert_ne!(layout.history_epoch, old_anchor.history_epoch);
        assert_eq!(layout.message_versions.len(), 1, "元数据不得残留被回滚消息");
        assert!(output
            .links
            .iter()
            .any(|hit| hit.url == "https://new.example/rewind"));
        assert!(!output
            .links
            .iter()
            .any(|hit| hit.url == "https://old.example/rewind"));
    }
    assert_eq!(
        make_copy_reply(&tx, old_anchor).await,
        None,
        "历史替换必须拒绝旧选区快照"
    );
}

#[tokio::test]
async fn test_worker_resize_preserves_following_strategy_and_detail_events() {
    let (tx, cache, notify) = spawn_render_thread(80);
    tx.send(RenderEvent::Rebuild(vec![
        MessageViewModel::system_with_detail(
            "compact marker".to_owned(),
            "detail marker\nlast detail line".to_owned(),
        ),
    ]))
    .await
    .unwrap();
    make_wait_for_version(&cache, &notify, 1).await;
    let version = cache.read().version;
    tx.try_send(RenderEvent::Resize(63)).unwrap();
    tx.try_send(RenderEvent::SetDetailStrategy("virtual".to_owned()))
        .unwrap();
    tx.try_send(RenderEvent::ToggleDetail(true)).unwrap();
    make_wait_for_version(&cache, &notify, version + 3).await;
    {
        let output = cache.read();
        assert_eq!(output.width, 63, "Resize 应保留目标宽度");
        assert!(
            output.virtual_layout.is_some(),
            "Resize 合并不能吞掉紧邻的策略事件"
        );
        assert!(
            output
                .lines
                .iter()
                .any(|line| line.to_string().contains("last detail line")),
            "策略切换不能吞掉后续详情事件"
        );
        assert!(!output
            .lines
            .iter()
            .any(|line| line.to_string().contains("compact marker")));
    }
    let version = cache.read().version;
    tx.try_send(RenderEvent::Resize(51)).unwrap();
    tx.try_send(RenderEvent::SetDetailStrategy("cache".to_owned()))
        .unwrap();
    tx.try_send(RenderEvent::ToggleDetail(false)).unwrap();
    make_wait_for_version(&cache, &notify, version + 3).await;
    let output = cache.read();
    assert_eq!(output.width, 51);
    assert!(
        output.virtual_layout.is_none(),
        "虚拟模式中的 Resize 同样不能吞掉回退事件"
    );
    assert!(output
        .lines
        .iter()
        .any(|line| line.to_string().contains("compact marker")));
    assert!(!output
        .lines
        .iter()
        .any(|line| line.to_string().contains("last detail line")));
}
