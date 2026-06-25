use super::*;

fn make_file(name: &str) -> FileNode {
    FileNode {
        name: name.to_string(),
        is_dir: false,
        children: Vec::new(),
        expanded: false,
        loaded: true,
        path: Some(name.to_string()),
    }
}

fn make_dir(name: &str, children: Vec<FileNode>, expanded: bool) -> FileNode {
    FileNode {
        name: name.to_string(),
        is_dir: true,
        children,
        expanded,
        loaded: true,
        path: Some(name.to_string()),
    }
}

#[test]
fn test_flatten_展开目录显示子节点() {
    // Arrange: 展开的目录包含 2 个文件
    let dir = make_dir("src", vec![make_file("main.rs"), make_file("lib.rs")], true);
    let mut state = FileTreeState::new();
    // Act
    state.set_root(vec![dir]);
    // Assert: flat = [src, main.rs, lib.rs]
    assert_eq!(state.flat().len(), 3);
    assert_eq!(state.flat()[0].name, "src");
    assert_eq!(state.flat()[0].depth, 0);
    assert_eq!(state.flat()[1].name, "main.rs");
    assert_eq!(state.flat()[1].depth, 1);
    assert_eq!(state.flat()[2].name, "lib.rs");
    assert_eq!(state.flat()[2].depth, 1);
}

#[test]
fn test_flatten_折叠目录跳过子节点() {
    // Arrange: 折叠的目录包含 2 个文件
    let dir = make_dir(
        "src",
        vec![make_file("main.rs"), make_file("lib.rs")],
        false,
    );
    let mut state = FileTreeState::new();
    // Act
    state.set_root(vec![dir]);
    // Assert: flat = [src] 只有目录自身
    assert_eq!(state.flat().len(), 1);
    assert_eq!(state.flat()[0].name, "src");
    assert!(!state.flat()[0].expanded);
}

#[test]
fn test_flatten_空根节点() {
    // Arrange: 空根节点列表
    let mut state = FileTreeState::new();
    // Act
    state.set_root(vec![]);
    // Assert
    assert_eq!(state.flat().len(), 0);
    assert!(state.is_empty());
    assert_eq!(state.selected(), None);
}

#[test]
fn test_set_root_重建扁平列表() {
    // Arrange: 先设置 3 个根节点
    let mut state = FileTreeState::new();
    state.set_root(vec![make_file("a"), make_file("b"), make_file("c")]);
    assert_eq!(state.flat().len(), 3);
    // Act: 替换为 1 个根节点
    state.set_root(vec![make_file("x")]);
    // Assert: 旧数据被清除
    assert_eq!(state.flat().len(), 1);
    assert_eq!(state.flat()[0].name, "x");
}

#[test]
fn test_clamp_cursor_缩短列表后光标归位() {
    // Arrange: 光标在第 4 个位置
    let mut state = FileTreeState::new();
    state.set_root(vec![
        make_file("a"),
        make_file("b"),
        make_file("c"),
        make_file("d"),
        make_file("e"),
    ]);
    state.cursor = 4;
    // Act: 替换为更短的列表（2 个元素）
    state.set_root(vec![make_file("x"), make_file("y")]);
    // Assert: 光标钳位到最后一个元素
    assert_eq!(state.cursor(), 1);
    assert_eq!(state.selected().unwrap().name, "y");
}

#[test]
fn test_flatten_嵌套展开目录() {
    // Arrange: src(展开) > lib(展开) > mod.rs
    let inner = make_dir("lib", vec![make_file("mod.rs")], true);
    let root = make_dir("src", vec![inner], true);
    let mut state = FileTreeState::new();
    // Act
    state.set_root(vec![root]);
    // Assert: flat = [src, lib, mod.rs]
    assert_eq!(state.flat().len(), 3);
    assert_eq!(state.flat()[0].depth, 0);
    assert_eq!(state.flat()[1].depth, 1);
    assert_eq!(state.flat()[2].depth, 2);
}

#[test]
fn test_move_cursor_clamp到边界() {
    // Arrange: 5 个文件
    let mut state = FileTreeState::new();
    state.set_root(vec![
        make_file("a"),
        make_file("b"),
        make_file("c"),
        make_file("d"),
        make_file("e"),
    ]);
    // Act & Assert: 向下越界 → clamp 到末尾
    state.move_cursor(100);
    assert_eq!(state.cursor(), 4);
    assert_eq!(state.selected().unwrap().name, "e");
    // 向上越界 → clamp 到开头
    state.move_cursor(-100);
    assert_eq!(state.cursor(), 0);
    assert_eq!(state.selected().unwrap().name, "a");
    // 正常移动
    state.move_cursor(2);
    assert_eq!(state.cursor(), 2);
}

#[test]
fn test_move_cursor_空列表不panic() {
    let mut state = FileTreeState::new();
    state.set_root(vec![]);
    // 不 panic
    state.move_cursor(1);
    state.move_cursor(-1);
    assert_eq!(state.cursor(), 0);
}

#[test]
fn test_click_返回flat索引并移动cursor() {
    let mut state = FileTreeState::new();
    state.set_root(vec![make_file("a"), make_file("b"), make_file("c")]);
    // scroll offset 为 0，点击第 2 行
    let result = state.click(1);
    assert_eq!(result, Some(1));
    assert_eq!(state.cursor(), 1);
    assert_eq!(state.selected().unwrap().name, "b");
}

#[test]
fn test_click_越界返回none() {
    let mut state = FileTreeState::new();
    state.set_root(vec![make_file("a")]);
    // cursor 初始为 0
    assert_eq!(state.cursor(), 0);
    let result = state.click(5);
    assert_eq!(result, None);
    // cursor 不变
    assert_eq!(state.cursor(), 0);
}

#[test]
fn test_selected_返回当前节点() {
    let mut state = FileTreeState::new();
    state.set_root(vec![make_file("a"), make_file("b"), make_file("c")]);
    state.move_cursor(2);
    let node = state.selected().unwrap();
    assert_eq!(node.name, "c");
    assert!(!node.is_dir);
}

#[test]
fn test_flatten_tree_path正确索引() {
    // Arrange: 多个根节点，第一个有子节点
    let dir = make_dir("src", vec![make_file("a.rs"), make_file("b.rs")], true);
    let file = make_file("Cargo.toml");
    let mut state = FileTreeState::new();
    state.set_root(vec![dir, file]);
    // Assert: tree_path 正确
    assert_eq!(state.flat()[0].tree_path, vec![0]); // src
    assert_eq!(state.flat()[1].tree_path, vec![0, 0]); // a.rs
    assert_eq!(state.flat()[2].tree_path, vec![0, 1]); // b.rs
    assert_eq!(state.flat()[3].tree_path, vec![1]); // Cargo.toml
}

#[test]
fn test_toggle_展开已加载目录() {
    // Arrange: 折叠目录包含 2 个子节点
    let dir = make_dir("src", vec![make_file("a.rs"), make_file("b.rs")], false);
    let mut state = FileTreeState::new();
    state.set_root(vec![dir]);
    assert_eq!(state.flat().len(), 1); // 只有 src
                                       // Act: toggle 展开
    let result = state.toggle(0).unwrap();
    // Assert
    assert!(!result.needs_load);
    assert!(result.expanded);
    assert_eq!(state.flat().len(), 3); // src, a.rs, b.rs
}

#[test]
fn test_toggle_折叠已展开目录() {
    // Arrange: 展开的目录
    let dir = make_dir("src", vec![make_file("a.rs"), make_file("b.rs")], true);
    let mut state = FileTreeState::new();
    state.set_root(vec![dir]);
    assert_eq!(state.flat().len(), 3);
    // Act: toggle 折叠
    let result = state.toggle(0).unwrap();
    // Assert
    assert!(!result.needs_load);
    assert!(!result.expanded);
    assert_eq!(state.flat().len(), 1); // 只有 src
}

#[test]
fn test_toggle_未加载目录返回needs_load() {
    // Arrange: 未加载目录
    let dir = FileNode {
        name: "src".to_string(),
        is_dir: true,
        children: Vec::new(),
        expanded: false,
        loaded: false,
        path: Some("src".to_string()),
    };
    let mut state = FileTreeState::new();
    state.set_root(vec![dir]);
    assert_eq!(state.flat().len(), 1);
    // Act
    let result = state.toggle(0).unwrap();
    // Assert: needs_load=true，flat 不变
    assert!(result.needs_load);
    assert!(!result.expanded);
    assert_eq!(result.path, "src");
    assert_eq!(state.flat().len(), 1);
}

#[test]
fn test_set_children_填充后toggle展开() {
    // Arrange: 未加载目录
    let dir = FileNode {
        name: "src".to_string(),
        is_dir: true,
        children: Vec::new(),
        expanded: false,
        loaded: false,
        path: Some("src".to_string()),
    };
    let mut state = FileTreeState::new();
    state.set_root(vec![dir]);
    // Act: 先 toggle 获得 needs_load
    let result = state.toggle(0).unwrap();
    assert!(result.needs_load);
    // 填充子节点
    state.set_children("src", vec![make_file("a.rs"), make_file("b.rs")]);
    // 再次 toggle → 展开
    let result = state.toggle(0).unwrap();
    assert!(!result.needs_load);
    assert!(result.expanded);
    assert_eq!(state.flat().len(), 3);
}

#[test]
fn test_toggle_文件返回none() {
    let mut state = FileTreeState::new();
    state.set_root(vec![make_file("main.rs")]);
    assert!(state.toggle(0).is_none());
}

#[test]
fn test_toggle_空已加载目录不可展开() {
    let dir = make_dir("empty", vec![], false);
    let mut state = FileTreeState::new();
    state.set_root(vec![dir]);
    assert!(state.toggle(0).is_none());
}

#[test]
fn test_sort_目录优先加字母序() {
    // Arrange: 混合文件和目录，乱序
    let mut state = FileTreeState::new();
    state.set_root(vec![
        make_file("z.txt"),
        make_dir("alpha", vec![], false),
        make_file("a.txt"),
        make_dir("beta", vec![], false),
        make_file("m.txt"),
    ]);
    // Act
    state.sort();
    // Assert: 目录在前，文件在后，各自按字母序
    let names: Vec<&str> = state.flat().iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "beta", "a.txt", "m.txt", "z.txt"]);
}

#[test]
fn test_sort_递归排序子目录() {
    // Arrange: 展开的目录中子节点乱序
    let inner = make_dir(
        "sub",
        vec![
            make_file("z.rs"),
            make_file("a.rs"),
            make_dir("nested", vec![], false),
        ],
        true,
    );
    let mut state = FileTreeState::new();
    state.set_root(vec![inner]);
    // Act
    state.sort();
    // Assert: sub 内部也是目录优先 + 字母序
    let names: Vec<&str> = state.flat().iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, vec!["sub", "nested", "a.rs", "z.rs"]);
}
