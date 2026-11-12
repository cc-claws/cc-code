use super::*;
use crate::ui::{
    message_view::{ToolCategory, ToolEntry},
    render_layout::LayoutMode,
};

fn make_renderer(messages: Vec<MessageViewModel>) -> (VirtualRenderer, Vec<Arc<MessageViewModel>>) {
    let mut renderer = VirtualRenderer::default();
    renderer.sync_messages(&messages);
    renderer.set_mode(LayoutMode {
        width: 80,
        detail: false,
        diff: false,
        hook_rows: true,
        theme: crate::ui::theme::fingerprint(),
    });
    (renderer, messages.into_iter().map(Arc::new).collect())
}

fn make_hidden_group() -> MessageViewModel {
    let mut message = MessageViewModel::ToolCallGroup {
        thinking: None,
        category: ToolCategory::Read,
        tools: vec![ToolEntry {
            tool_name: "Read".into(),
            display_name: "Read".into(),
            args_display: Some("file.txt".into()),
            content: "success".into(),
            is_error: false,
            file_inspect: false,
        }],
        collapsed: true,
        content_hash: 0,
        standalone_action: None,
    };
    message.recompute_hash();
    message
}

#[test]
fn test_relative_scroll_uses_real_heights_across_multiple_messages_and_clamps_edges() {
    for (start, distance, message_index, logical_line) in [
        (0, 104, 2, 0),
        (0, 105, 2, 1),
        (2, -4, 1, 97),
        (2, -105, 0, 0),
        (0, 10_000, 2, 19),
    ] {
        let (mut renderer, mut messages) = make_renderer(vec![
            MessageViewModel::user("a".repeat(156)),
            MessageViewModel::user("b".repeat(7_800)),
            MessageViewModel::user("c".repeat(1_560)),
        ]);
        assert!(
            renderer.metadata[1].measured.is_none(),
            "中间长消息应尚未实测"
        );
        let resolved = renderer.resolve_relative_anchor(
            &mut messages,
            ViewportAnchor {
                message_index: start,
                relative_rows: distance,
                ..Default::default()
            },
            0,
        );
        assert_eq!(resolved.message_index, message_index, "须按真实高度跨消息");
        assert_eq!(resolved.logical_line, logical_line, "须保留真实滚动距离");
        assert_eq!(resolved.relative_rows, 0, "解析后的锚点不能再次消费位移");
    }
}

#[test]
fn test_relative_scroll_crosses_hidden_zero_height_groups_in_both_directions() {
    for (start, distance, message_index, logical_line) in [(0, 3, 2, 0), (4, -4, 0, 2)] {
        let (mut renderer, mut messages) = make_renderer(vec![
            MessageViewModel::user("a".repeat(156)),
            make_hidden_group(),
            MessageViewModel::user("b".repeat(156)),
            make_hidden_group(),
            MessageViewModel::user("c".repeat(156)),
        ]);
        let resolved = renderer.resolve_relative_anchor(
            &mut messages,
            ViewportAnchor {
                message_index: start,
                relative_rows: distance,
                ..Default::default()
            },
            0,
        );
        assert_eq!(
            resolved.message_index, message_index,
            "隐藏组不能消耗滚动行"
        );
        assert_eq!(
            resolved.logical_line, logical_line,
            "跨隐藏组后仍按三行定位"
        );
    }
}
