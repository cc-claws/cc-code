use super::*;
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn test_overlay_state_initial_none() {
    let state = ListOverlayState::new();
    assert!(state.area().is_none());
}

#[test]
fn test_overlay_state_tracks_area() {
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut list_state = ListState::new(vec!["a", "b", "c"]);
    let mut overlay_state = ListOverlayState::new();
    terminal
        .draw(|f| {
            let viewport = Rect::new(0, 0, 40, 20);
            let list = SelectableList::new(|item: &&str, _is_cursor: bool, _is_hovered: bool| {
                ratatui::text::Line::from(*item)
            });
            ListOverlay::new(list)
                .width(20)
                .anchor(Anchor::Below { x: 5, y: 3 })
                .render(f, viewport, &mut list_state, &mut overlay_state);
        })
        .unwrap();
    let area = overlay_state.area().unwrap();
    assert!(area.width > 0);
    assert!(area.height > 0);
}

#[test]
fn test_overlay_renders_items() {
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut list_state = ListState::new(vec!["alpha", "beta", "gamma"]);
    let mut overlay_state = ListOverlayState::new();
    terminal
        .draw(|f| {
            let viewport = Rect::new(0, 0, 40, 20);
            let list = SelectableList::new(|item: &&str, _is_cursor: bool, _is_hovered: bool| {
                ratatui::text::Line::from(*item)
            });
            ListOverlay::new(list)
                .width(20)
                .anchor(Anchor::Below { x: 5, y: 3 })
                .render(f, viewport, &mut list_state, &mut overlay_state);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let area = overlay_state.area().unwrap();
    // 检查 buffer 中包含预期内容
    let mut found_alpha = false;
    let mut found_beta = false;
    for y in area.y..area.y + area.height {
        let row: String = (area.x..area.x + area.width)
            .map(|x| buf.cell((x, y)).unwrap().symbol().to_string())
            .collect();
        if row.contains("alpha") {
            found_alpha = true;
        }
        if row.contains("beta") {
            found_beta = true;
        }
    }
    assert!(found_alpha, "Buffer 中应包含 'alpha'");
    assert!(found_beta, "Buffer 中应包含 'beta'");
}

#[test]
fn test_overlay_below_anchor() {
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut list_state = ListState::new(vec!["a", "b"]);
    let mut overlay_state = ListOverlayState::new();
    terminal
        .draw(|f| {
            let viewport = Rect::new(0, 0, 40, 20);
            let list = SelectableList::new(|item: &&str, _c: bool, _h: bool| {
                ratatui::text::Line::from(*item)
            });
            ListOverlay::new(list)
                .width(20)
                .anchor(Anchor::Below { x: 5, y: 3 })
                .render(f, viewport, &mut list_state, &mut overlay_state);
        })
        .unwrap();
    let area = overlay_state.area().unwrap();
    assert!(area.y >= 3, "Below 锚点时 y 应 >= anchor.y");
}

#[test]
fn test_overlay_above_anchor_fallback() {
    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut list_state = ListState::new(vec!["a", "b", "c"]);
    let mut overlay_state = ListOverlayState::new();
    terminal
        .draw(|f| {
            let viewport = Rect::new(0, 0, 40, 10);
            let list = SelectableList::new(|item: &&str, _c: bool, _h: bool| {
                ratatui::text::Line::from(*item)
            });
            // anchor y=1，上方空间不足（panel_height=5），应回退到 Below
            ListOverlay::new(list)
                .width(20)
                .position(OverlayPosition::Above)
                .anchor(Anchor::Above { x: 5, y: 1 })
                .render(f, viewport, &mut list_state, &mut overlay_state);
        })
        .unwrap();
    let area = overlay_state.area().unwrap();
    assert!(area.y >= 1, "上方空间不足时应回退到 Below");
}

#[test]
fn test_overlay_max_height_clamped() {
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let items: Vec<String> = (0..20).map(|i| i.to_string()).collect();
    let mut list_state = ListState::new(items.clone());
    let mut overlay_state = ListOverlayState::new();
    terminal
        .draw(|f| {
            let viewport = Rect::new(0, 0, 40, 20);
            let list = SelectableList::new(|item: &String, _c: bool, _h: bool| {
                ratatui::text::Line::from(item.clone())
            });
            ListOverlay::new(list)
                .width(20)
                .max_height(5)
                .anchor(Anchor::Below { x: 0, y: 0 })
                .render(f, viewport, &mut list_state, &mut overlay_state);
        })
        .unwrap();
    let area = overlay_state.area().unwrap();
    // items=20, max_height=5 → content_height=5, panel_height=7
    assert_eq!(area.height, 7, "面板高度应为 max_height + 2");
}

#[test]
fn test_overlay_clears_background() {
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut list_state = ListState::new(vec!["a", "b"]);
    let mut overlay_state = ListOverlayState::new();
    // 先写入一些内容到 buffer
    terminal
        .draw(|f| {
            let viewport = Rect::new(0, 0, 40, 20);
            // 写入 "XXXXXXXXXX" 到预期面板区域
            let paragraph =
                ratatui::widgets::Paragraph::new("XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
            f.render_widget(paragraph, viewport);
        })
        .unwrap();
    // 再渲染 overlay，Clear 应覆盖背景
    terminal
        .draw(|f| {
            let viewport = Rect::new(0, 0, 40, 20);
            let list = SelectableList::new(|item: &&str, _c: bool, _h: bool| {
                ratatui::text::Line::from(*item)
            });
            ListOverlay::new(list)
                .width(20)
                .anchor(Anchor::Below { x: 0, y: 0 })
                .render(f, viewport, &mut list_state, &mut overlay_state);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let area = overlay_state.area().unwrap();
    // 面板区域的 cells 应已被 Clear 覆盖（不再全是 'X'）
    let first_row: String = (area.x..area.x + area.width)
        .map(|x| buf.cell((x, area.y)).unwrap().symbol().to_string())
        .collect();
    assert!(!first_row.chars().all(|c| c == 'X'), "Clear 应覆盖背景内容");
}
