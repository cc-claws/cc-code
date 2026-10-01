use super::*;
use crate::file_tree::{FileNode, FileTreeState};
use ratatui::{backend::TestBackend, style::Color, Terminal};

fn make_file(name: &str) -> FileNode {
    FileNode {
        name: name.to_string(),
        is_dir: false,
        children: vec![],
        expanded: false,
        loaded: true,
        path: Some(format!("/{}", name)),
    }
}

fn make_dir(name: &str, children: Vec<FileNode>, expanded: bool) -> FileNode {
    FileNode {
        name: name.to_string(),
        is_dir: true,
        children,
        expanded,
        loaded: true,
        path: Some(format!("/{}", name)),
    }
}

#[test]
fn render_折叠目录显示三角和斜杠() {
    // Arrange: 单个折叠目录
    let dir = make_dir("src", vec![], false);
    let mut state = FileTreeState::new();
    state.set_root(vec![dir]);
    let tree = FileTree::new();

    // Act
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 20, 5);
            f.render_stateful_widget(tree, area, &mut state);
        })
        .unwrap();

    // Assert: 第一行应含 ▸ 和 src/
    let buf = terminal.backend().buffer().clone();
    let row0: String = (0..20)
        .map(|x| buf.cell((x, 0)).unwrap().symbol().to_string())
        .collect();
    assert!(row0.contains("▸"), "折叠目录应显示 ▸，实际: {:?}", row0);
    assert!(row0.contains("src/"), "目录名应带斜杠，实际: {:?}", row0);
}

#[test]
fn render_展开目录显示子文件和竖线() {
    // Arrange: 展开的目录含一个子文件
    let dir = make_dir("src", vec![make_file("main.rs")], true);
    let mut state = FileTreeState::new();
    state.set_root(vec![dir]);
    let tree = FileTree::new();

    // Act
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 20, 5);
            f.render_stateful_widget(tree, area, &mut state);
        })
        .unwrap();

    // Assert: 行0含 ▾ src/，行1含 │ main.rs
    let buf = terminal.backend().buffer().clone();
    let row0: String = (0..20)
        .map(|x| buf.cell((x, 0)).unwrap().symbol().to_string())
        .collect();
    assert!(row0.contains("▾"), "展开目录应显示 ▾，实际: {:?}", row0);
    let row1: String = (0..20)
        .map(|x| buf.cell((x, 1)).unwrap().symbol().to_string())
        .collect();
    assert!(row1.contains("│"), "子文件行应有竖线缩进，实际: {:?}", row1);
    assert!(row1.contains("main.rs"), "子文件名应显示，实际: {:?}", row1);
}

#[test]
fn render_选中行有背景色() {
    // Arrange: cursor_style 设蓝色背景
    let dir = make_dir("src", vec![make_file("a.rs"), make_file("b.rs")], false);
    let mut state = FileTreeState::new();
    state.set_root(vec![dir, make_file("readme.md")]);
    state.move_cursor(1); // cursor 在 readme.md
    let tree = FileTree::new().cursor_style(Style::default().bg(Color::Blue));

    // Act
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 20, 5);
            f.render_stateful_widget(tree, area, &mut state);
        })
        .unwrap();

    // Assert: 行1（cursor 行）有蓝色背景，行0没有
    let buf = terminal.backend().buffer().clone();
    let cursor_cell = buf.cell((0, 1)).unwrap();
    assert_eq!(cursor_cell.bg, Color::Blue, "cursor 行应有蓝色背景");
    let normal_cell = buf.cell((0, 0)).unwrap();
    assert_ne!(normal_cell.bg, Color::Blue, "非 cursor 行不应有蓝色背景");
}

#[test]
fn render_空树不崩溃() {
    // Arrange: 空 FileTreeState
    let mut state = FileTreeState::new();
    let tree = FileTree::new();

    // Act
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 20, 5);
            f.render_stateful_widget(tree, area, &mut state);
        })
        .unwrap();

    // Assert: 无 panic，所有行为空格
    let buf = terminal.backend().buffer().clone();
    let row0: String = (0..20)
        .map(|x| buf.cell((x, 0)).unwrap().symbol().to_string())
        .collect();
    assert!(row0.trim().is_empty(), "空树应渲染为空行，实际: {:?}", row0);
}
