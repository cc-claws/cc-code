use super::*;
use crate::ui::render_thread::{VirtualLayoutSnapshot, WrappedLineInfo};
use std::sync::Arc;

fn make_window_cache() -> RenderCache {
    let mut cache = RenderCache::new();
    cache.width = 80;
    cache.lines = vec![ratatui::text::Line::from("A中e\u{301}👩‍💻Z")];
    cache.wrap_map = vec![WrappedLineInfo {
        line_idx: 0,
        visual_row_start: 6_000,
        visual_row_end: 6_003,
        plain_text: "A中e\u{301}👩‍💻Z".to_string(),
        char_widths: vec![1, 2, 1, 2, 1],
    }];
    let layout = VirtualLayoutSnapshot {
        history_epoch: 7,
        window_start: 6_000,
        line_origins: vec![(21, 4)],
        message_versions: vec![100; 22].into(),
        ..Default::default()
    };
    cache.virtual_layout = Some(Arc::new(layout));
    cache
}

#[test]
fn test_viewport_anchor_preserves_offset_inside_wrapped_line() {
    let cache = make_window_cache();
    assert_eq!(
        cache.viewport_anchor_at(6_002),
        Some(ViewportAnchor {
            message_index: 21,
            logical_line: 4,
            row_in_line: 2,
        }),
        "锚点应保存消息原始逻辑行及折行内偏移"
    );
    assert_eq!(
        cache.viewport_anchor_at(5_999),
        None,
        "未物化区域不能伪造锚点"
    );
    assert_eq!(cache.viewport_anchor_at(6_003), None, "右开边界不属于该行");
}

#[test]
fn test_selection_anchor_uses_window_origin_and_grapheme_offset() {
    let cache = make_window_cache();
    assert_eq!(
        cache.selection_anchor_at(6_000, 4, 80),
        Some(SelectionAnchor {
            history_epoch: 7,
            message_index: 21,
            message_version: 100,
            logical_line: 4,
            grapheme_offset: 3,
        }),
        "窗口局部行需还原原文位置，组合字符与emoji按字素计数"
    );
}

#[test]
fn test_link_anchor_rejects_replaced_content_or_history() {
    let mut cache = make_window_cache();
    cache.links.push(cc_widgets::markdown::LinkHit {
        line: 0,
        g_start: 2,
        g_end: 4,
        url: "https://example.com".to_string(),
    });
    let anchor = cache.selection_anchor_at(6_000, 4, 80).expect("选区应命中");
    assert_eq!(
        cache.link_at_selection_anchor(anchor).as_deref(),
        Some("https://example.com")
    );
    assert_eq!(
        cache.link_at_selection_anchor(SelectionAnchor {
            message_version: 101,
            ..anchor
        }),
        None,
        "内容替换后不可打开旧链接"
    );
    assert_eq!(
        cache.link_at_selection_anchor(SelectionAnchor {
            history_epoch: 8,
            ..anchor
        }),
        None,
        "历史替换后不可打开旧链接"
    );
}
