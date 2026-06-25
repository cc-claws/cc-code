# FileTree Widget 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `peri-widgets` 中实现一个通用的文件树组件，类似 VS Code 侧边栏，支持展开/折叠、虚拟滚动、懒加载、选中高亮。

**Architecture:** 拆为两个核心结构——`FileNode`（树形数据）+ `FileTreeState`（状态管理）+ `FileTree`（StatefulWidget 渲染）。组件内部将树扁平化为 `FlatNode` 列表实现虚拟滚动。展开/折叠通过 `toggle()` 返回 `ToggleResult` 通知调用方按需加载子节点。

**Tech Stack:** ratatui（StatefulWidget + Paragraph + Scroll）、unicode-width（列宽计算）、组件复用 `ScrollState`。

---

## File Structure

| 操作 | 文件 | 职责 |
|------|------|------|
| Create | `peri-widgets/src/file_tree.rs` | `FileNode` + `FileTreeState` + `ToggleResult` + `FlatNode` 数据结构和状态逻辑 |
| Create | `peri-widgets/src/file_tree_test.rs` | 状态逻辑单元测试（flatten、toggle、sort、navigate） |
| Create | `peri-widgets/src/file_tree_render.rs` | `FileTree` widget（实现 StatefulWidget） |
| Create | `peri-widgets/src/file_tree_render_test.rs` | 渲染测试（TestBackend + Terminal 验证输出） |
| Modify | `peri-widgets/src/lib.rs` | 新增 `pub mod file_tree;` + re-export |

---

## Task 1: FileNode 数据结构 + FlatNode + ToggleResult

**Files:**
- Create: `peri-widgets/src/file_tree.rs`
- Test: `peri-widgets/src/file_tree_test.rs`

- [ ] **Step 1: 写 file_tree.rs 的数据结构定义**

```rust
use crate::scrollable::ScrollState;

/// 文件树节点
#[derive(Debug, Clone)]
pub struct FileNode {
    /// 文件或目录名（不含路径前缀）
    pub name: String,
    /// 是否为目录
    pub is_dir: bool,
    /// 子节点（仅目录有效）
    pub children: Vec<FileNode>,
    /// 目录是否展开（仅目录有效）
    pub expanded: bool,
    /// 子节点是否已加载（未加载时点击展开需要调用方填充 children）
    pub loaded: bool,
    /// 完整路径（由调用方填充）
    pub path: Option<String>,
}

/// toggle() 操作的返回值，通知调用方是否需要加载子节点
#[derive(Debug, Clone)]
pub struct ToggleResult {
    /// 目录未加载，调用方需要调用 set_children() 填充
    pub needs_load: bool,
    /// 被操作节点的完整路径
    pub path: String,
    /// 是否为目录
    pub is_dir: bool,
    /// toggle 后的展开状态
    pub expanded: bool,
}

/// 扁平化后的行数据（虚拟滚动用）
#[derive(Debug, Clone)]
pub struct FlatNode {
    /// 节点名称
    pub name: String,
    /// 缩进深度（0 = 根级）
    pub depth: usize,
    /// 是否为目录
    pub is_dir: bool,
    /// 目录是否展开（仅目录有效）
    pub expanded: bool,
    /// 子节点是否已加载
    pub loaded: bool,
    /// 完整路径
    pub path: Option<String>,
    /// 该行在原始 FileNode 树中的可寻址路径（用于 toggle 时定位节点）
    tree_path: Vec<usize>,
}
```

- [ ] **Step 2: 写 FlatNode 的 tree_path 测试**

在 `file_tree_test.rs` 中：

```rust
use super::*;

#[test]
fn flat_node_stores_tree_path() {
    let flat = FlatNode {
        name: "src".to_string(),
        depth: 0,
        is_dir: true,
        expanded: false,
        loaded: true,
        path: Some("/project/src".to_string()),
        tree_path: vec![0],
    };
    assert_eq!(flat.tree_path, vec![0]);
}
```

- [ ] **Step 3: 运行测试确认失败**

Run: `cargo test -p peri-widgets -- file_tree`
Expected: 编译失败（模块未注册）

---

## Task 2: 注册模块 + FileTreeState 基础（new + flatten）

**Files:**
- Modify: `peri-widgets/src/file_tree.rs`
- Modify: `peri-widgets/src/lib.rs`
- Test: `peri-widgets/src/file_tree_test.rs`

- [ ] **Step 1: 在 lib.rs 注册模块**

在 `pub mod form;` 之后添加 `pub mod file_tree;`。

- [ ] **Step 2: 实现 FileTreeState::new() 和 flatten()**

在 `file_tree.rs` 中添加：

```rust
/// 文件树状态——管理树形数据 + 光标 + 滚动
pub struct FileTreeState {
    root: Vec<FileNode>,
    /// 扁平化后的行列表（每次树变更时重建）
    flat: Vec<FlatNode>,
    /// 光标位置（flat 列表索引）
    cursor: usize,
    /// 滚动状态
    pub scroll: ScrollState,
    /// 文件图标闭包：参数为文件名（含扩展名），返回显示的 char
    icon_fn: Box<dyn Fn(&str) -> char>,
}

impl FileTreeState {
    /// 创建空文件树状态
    pub fn new() -> Self {
        Self {
            root: Vec::new(),
            flat: Vec::new(),
            cursor: 0,
            scroll: ScrollState::new(),
            icon_fn: Box::new(|_| ' '),
        }
    }

    /// 设置根节点列表，重建 flat 并 clamp cursor
    pub fn set_root(&mut self, root: Vec<FileNode>) {
        self.root = root;
        self.rebuild_flat();
        self.clamp_cursor();
    }

    /// 获取当前光标指向的扁平节点
    pub fn selected(&self) -> Option<&FlatNode> {
        self.flat.get(self.cursor)
    }

    /// 获取扁平列表引用
    pub fn flat(&self) -> &[FlatNode] {
        &self.flat
    }

    /// 光标位置
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// 总行数
    pub fn len(&self) -> usize {
        self.flat.len()
    }

    /// 重建扁平列表
    fn rebuild_flat(&mut self) {
        self.flat.clear();
        for (i, node) in self.root.iter().enumerate() {
            self.flatten_node(node, 0, vec![i]);
        }
    }

    /// 递归扁平化
    fn flatten_node(&mut self, node: &FileNode, depth: usize, tree_path: Vec<usize>) {
        self.flat.push(FlatNode {
            name: node.name.clone(),
            depth,
            is_dir: node.is_dir,
            expanded: node.expanded,
            loaded: node.loaded,
            path: node.path.clone(),
            tree_path: tree_path.clone(),
        });
        // 展开的目录递归子节点
        if node.is_dir && node.expanded {
            for (i, child) in node.children.iter().enumerate() {
                let mut child_path = tree_path.clone();
                child_path.push(i);
                self.flatten_node(child, depth + 1, child_path);
            }
        }
    }

    /// clamp cursor 到 [0, flat.len())
    fn clamp_cursor(&mut self) {
        if self.flat.is_empty() {
            self.cursor = 0;
        } else {
            self.cursor = self.cursor.min(self.flat.len() - 1);
        }
    }
}

impl Default for FileTreeState {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 3: 写 flatten 测试**

```rust
#[test]
fn flatten_linear_tree() {
    let root = vec![FileNode {
        name: "src".to_string(),
        is_dir: true,
        expanded: true,
        loaded: true,
        path: Some("/src".to_string()),
        children: vec![
            FileNode {
                name: "main.rs".to_string(),
                is_dir: false,
                expanded: false,
                loaded: true,
                path: Some("/src/main.rs".to_string()),
                children: vec![],
            },
            FileNode {
                name: "lib.rs".to_string(),
                is_dir: false,
                expanded: false,
                loaded: true,
                path: Some("/src/lib.rs".to_string()),
                children: vec![],
            },
        ],
    }];
    let mut state = FileTreeState::new();
    state.set_root(root);
    assert_eq!(state.flat().len(), 3);
    assert_eq!(state.flat()[0].name, "src");
    assert_eq!(state.flat()[0].depth, 0);
    assert_eq!(state.flat()[1].name, "main.rs");
    assert_eq!(state.flat()[1].depth, 1);
    assert_eq!(state.flat()[2].name, "lib.rs");
    assert_eq!(state.flat()[2].depth, 1);
}

#[test]
fn flatten_collapsed_dir_skips_children() {
    let root = vec![FileNode {
        name: "src".to_string(),
        is_dir: true,
        expanded: false,
        loaded: true,
        path: Some("/src".to_string()),
        children: vec![FileNode {
            name: "main.rs".to_string(),
            is_dir: false,
            expanded: false,
            loaded: true,
            path: Some("/src/main.rs".to_string()),
            children: vec![],
        }],
    }];
    let mut state = FileTreeState::new();
    state.set_root(root);
    assert_eq!(state.flat().len(), 1);
    assert_eq!(state.flat()[0].name, "src");
}

#[test]
fn flatten_empty_root() {
    let mut state = FileTreeState::new();
    state.set_root(vec![]);
    assert_eq!(state.flat().len(), 0);
    assert!(state.selected().is_none());
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p peri-widgets -- file_tree`
Expected: 3 tests PASS

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/file_tree.rs peri-widgets/src/file_tree_test.rs peri-widgets/src/lib.rs
git commit -m "feat(peri-widgets): add FileNode data structures and FileTreeState flatten"
```

---

## Task 3: FileTreeState 导航（move_cursor + click + selected）

**Files:**
- Modify: `peri-widgets/src/file_tree.rs`
- Test: `peri-widgets/src/file_tree_test.rs`

- [ ] **Step 1: 写导航方法失败的测试**

```rust
#[test]
fn move_cursor_clamps_to_bounds() {
    let root = vec![
        make_file("a.rs"),
        make_file("b.rs"),
        make_file("c.rs"),
    ];
    let mut state = FileTreeState::new();
    state.set_root(root);
    assert_eq!(state.cursor(), 0);
    state.move_cursor(2);
    assert_eq!(state.cursor(), 2);
    state.move_cursor(5); // 超出 → clamp
    assert_eq!(state.cursor(), 2);
    state.move_cursor(-10); // 超出 → clamp
    assert_eq!(state.cursor(), 0);
}

#[test]
fn move_cursor_empty_no_panic() {
    let mut state = FileTreeState::new();
    state.move_cursor(1); // 不应 panic
    assert_eq!(state.cursor(), 0);
}

#[test]
fn click_returns_flat_index() {
    let root = vec![make_file("a.rs"), make_file("b.rs"), make_file("c.rs")];
    let mut state = FileTreeState::new();
    state.set_root(root);
    let result = state.click(1);
    assert_eq!(result, Some(1));
    assert_eq!(state.cursor(), 1);
}

#[test]
fn click_out_of_bounds_returns_none() {
    let root = vec![make_file("a.rs")];
    let mut state = FileTreeState::new();
    state.set_root(root);
    let result = state.click(5);
    assert_eq!(result, None);
    assert_eq!(state.cursor(), 0); // cursor 不变
}

#[test]
fn selected_returns_current_flat_node() {
    let root = vec![make_file("a.rs"), make_file("b.rs")];
    let mut state = FileTreeState::new();
    state.set_root(root);
    state.move_cursor(1);
    let sel = state.selected().unwrap();
    assert_eq!(sel.name, "b.rs");
}
```

其中 `make_file` 辅助函数：

```rust
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p peri-widgets -- file_tree`
Expected: FAIL（方法不存在）

- [ ] **Step 3: 实现导航方法**

在 `impl FileTreeState` 中添加：

```rust
/// 移动光标（clamp 模式）
pub fn move_cursor(&mut self, delta: i32) {
    if self.flat.is_empty() {
        return;
    }
    let max = self.flat.len() - 1;
    let new = self.cursor as i32 + delta;
    self.cursor = new.clamp(0, max as i32) as usize;
}

/// 点击某行（相对视口行号），返回对应的 flat 索引
pub fn click(&mut self, row: u16) -> Option<usize> {
    let idx = row as usize + self.scroll.offset() as usize;
    if idx < self.flat.len() {
        self.cursor = idx;
        Some(idx)
    } else {
        None
    }
}

/// 设置图标闭包
pub fn set_icon_fn(&mut self, f: impl Fn(&str) -> char + 'static) {
    self.icon_fn = Box::new(f);
}

/// 获取图标 char
pub fn icon_for(&self, name: &str) -> char {
    (self.icon_fn)(name)
}

/// 确保 cursor 在可见视口内
pub fn ensure_visible(&mut self, visible_height: u16) {
    self.scroll.ensure_visible(self.cursor as u16, visible_height);
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p peri-widgets -- file_tree`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/file_tree.rs peri-widgets/src/file_tree_test.rs
git commit -m "feat(peri-widgets): add FileTreeState navigation methods"
```

---

## Task 4: toggle + set_children（展开/折叠 + 懒加载）

**Files:**
- Modify: `peri-widgets/src/file_tree.rs`
- Test: `peri-widgets/src/file_tree_test.rs`

- [ ] **Step 1: 写 toggle 测试**

```rust
#[test]
fn toggle_expands_loaded_dir() {
    let root = vec![make_dir("src", vec![make_file("main.rs")], false)];
    let mut state = FileTreeState::new();
    state.set_root(root);
    assert_eq!(state.flat().len(), 1); // 折叠

    let result = state.toggle(0).unwrap();
    assert!(!result.needs_load);
    assert!(result.expanded);
    assert_eq!(state.flat().len(), 2); // src + main.rs
}

#[test]
fn toggle_collapses_expanded_dir() {
    let root = vec![make_dir("src", vec![make_file("main.rs")], true)];
    let mut state = FileTreeState::new();
    state.set_root(root);
    assert_eq!(state.flat().len(), 2); // src + main.rs

    let result = state.toggle(0).unwrap();
    assert!(!result.needs_load);
    assert!(!result.expanded);
    assert_eq!(state.flat().len(), 1); // 折叠
}

#[test]
fn toggle_unloaded_dir_returns_needs_load() {
    let root = vec![FileNode {
        name: "node_modules".to_string(),
        is_dir: true,
        children: vec![],
        expanded: false,
        loaded: false,
        path: Some("/node_modules".to_string()),
    }];
    let mut state = FileTreeState::new();
    state.set_root(root);

    let result = state.toggle(0).unwrap();
    assert!(result.needs_load);
    assert_eq!(result.path, "/node_modules");
    // 目录仍折叠，等待 set_children
    assert_eq!(state.flat().len(), 1);
}

#[test]
fn set_children_then_toggle_shows_children() {
    let root = vec![FileNode {
        name: "node_modules".to_string(),
        is_dir: true,
        children: vec![],
        expanded: false,
        loaded: false,
        path: Some("/node_modules".to_string()),
    }];
    let mut state = FileTreeState::new();
    state.set_root(root);

    // 模拟调用方加载子节点
    state.set_children(
        "/node_modules",
        vec![make_file("foo.js"), make_file("bar.js")],
    );

    // 再次 toggle → 展开
    let result = state.toggle(0).unwrap();
    assert!(!result.needs_load);
    assert!(result.expanded);
    assert_eq!(state.flat().len(), 3); // node_modules + foo.js + bar.js
}

#[test]
fn toggle_on_file_returns_none() {
    let root = vec![make_file("readme.md")];
    let mut state = FileTreeState::new();
    state.set_root(root);
    assert!(state.toggle(0).is_none());
}

#[test]
fn toggle_empty_loaded_dir_noop() {
    let root = vec![FileNode {
        name: "empty".to_string(),
        is_dir: true,
        children: vec![],
        expanded: false,
        loaded: true,
        path: Some("/empty".to_string()),
    }];
    let mut state = FileTreeState::new();
    state.set_root(root);
    // loaded=true + children 为空 → 不允许展开
    assert!(state.toggle(0).is_none());
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p peri-widgets -- file_tree`
Expected: FAIL

- [ ] **Step 3: 实现 toggle 和 set_children**

在 `impl FileTreeState` 中添加：

```rust
/// 切换指定行的展开/折叠状态
///
/// - 文件行：返回 None
/// - 已加载空目录：返回 None（不可展开）
/// - 未加载目录：返回 ToggleResult { needs_load: true, ... }
/// - 已加载非空目录：切换 expanded，返回 ToggleResult { needs_load: false, ... }
pub fn toggle(&mut self, flat_idx: usize) -> Option<ToggleResult> {
    let flat_node = self.flat.get(flat_idx)?;
    if !flat_node.is_dir {
        return None;
    }
    // 未加载 → 通知调用方
    if !flat_node.loaded {
        return Some(ToggleResult {
            needs_load: true,
            path: flat_node.path.clone()?,
            is_dir: true,
            expanded: false,
        });
    }
    // 已加载但空 → 不可展开
    let tree_path = flat_node.tree_path.clone();
    let node = self.node_by_path(&tree_path)?;
    if node.children.is_empty() {
        return None;
    }
    // 切换 expanded
    let new_expanded = !node.expanded;
    let path = node.path.clone()?;
    self.node_by_path_mut(&tree_path).expanded = new_expanded;
    self.rebuild_flat();
    self.clamp_cursor();
    Some(ToggleResult {
        needs_load: false,
        path,
        is_dir: true,
        expanded: new_expanded,
    })
}

/// 为指定路径的目录填充子节点（懒加载后调用）
pub fn set_children(&mut self, dir_path: &str, children: Vec<FileNode>) {
    if let Some(node) = self.find_node_by_path_mut(dir_path) {
        node.children = children;
        node.loaded = true;
        // 不自动展开——调用方需再次 toggle
    }
}

/// 按 tree_path 路径查找节点（不可变）
fn node_by_path(&self, path: &[usize]) -> Option<&FileNode> {
    let idx = *path.first()?;
    let node = self.root.get(idx)?;
    path.iter().skip(1).try_fold(node, |current, &i| {
        current.children.get(i)
    })
}

/// 按 tree_path 路径查找节点（可变）
fn node_by_path_mut(&mut self, path: &[usize]) -> Option<&mut FileNode> {
    let idx = *path.first()?;
    let node = self.root.get_mut(idx)?;
    if path.len() == 1 {
        return Some(node);
    }
    path.iter().skip(1).try_fold(node, |current, &i| {
        current.children.get_mut(i)
    })
}

/// 按完整路径字符串查找节点（可变），BFS
fn find_node_by_path_mut(&mut self, target: &str) -> Option<&mut FileNode> {
    fn search(nodes: &mut [FileNode], target: &str) -> Option<&mut FileNode> {
        for node in nodes.iter_mut() {
            if node.path.as_deref() == Some(target) {
                return Some(node);
            }
            if let found @ Some(_) = search(&mut node.children, target) {
                return found;
            }
        }
        None
    }
    search(&mut self.root, target)
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p peri-widgets -- file_tree`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/file_tree.rs peri-widgets/src/file_tree_test.rs
git commit -m "feat(peri-widgets): add toggle/set_children with lazy loading"
```

---

## Task 5: sort（目录优先 + 字母序）

**Files:**
- Modify: `peri-widgets/src/file_tree.rs`
- Test: `peri-widgets/src/file_tree_test.rs`

- [ ] **Step 1: 写 sort 测试**

```rust
#[test]
fn sort_directories_first_then_alphabetical() {
    let root = vec![
        make_file("zebra.rs"),
        make_dir("beta", vec![], false),
        make_file("alpha.rs"),
        make_dir("alpha", vec![], false),
    ];
    let mut state = FileTreeState::new();
    state.set_root(root);
    state.sort();

    let names: Vec<&str> = state.flat().iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "beta", "alpha.rs", "zebra.rs"]);
}

#[test]
fn sort_recursive_sorts_children() {
    let root = vec![make_dir("src", vec![
        make_file("z.rs"),
        make_file("a.rs"),
        make_dir("sub", vec![
            make_file("b.rs"),
            make_file("a.rs"),
        ], true),
    ], true)];
    let mut state = FileTreeState::new();
    state.set_root(root);
    state.sort();

    // 根级: src
    // src 下: sub(目录), a.rs, z.rs (目录优先)
    // sub 下: a.rs, b.rs
    let names: Vec<&str> = state.flat().iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["src", "sub", "a.rs", "b.rs", "a.rs", "z.rs"]);
}
```

- [ ] **Step 2: 运行测试确认失败**

- [ ] **Step 3: 实现 sort**

在 `impl FileTreeState` 中添加：

```rust
/// 排序：目录优先，同类型按字母序（递归）
pub fn sort(&mut self) {
    self.sort_nodes(&mut self.root);
    self.rebuild_flat();
}

fn sort_nodes(&mut self, nodes: &mut [FileNode]) {
    nodes.sort_by(|a, b| {
        // 目录优先
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });
    for node in nodes.iter_mut() {
        if node.is_dir {
            self.sort_nodes(&mut node.children);
        }
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/file_tree.rs peri-widgets/src/file_tree_test.rs
git commit -m "feat(peri-widgets): add sort (directories first + alphabetical)"
```

---

## Task 6: FileTree Widget 渲染（StatefulWidget）

**Files:**
- Create: `peri-widgets/src/file_tree_render.rs`
- Create: `peri-widgets/src/file_tree_render_test.rs`
- Modify: `peri-widgets/src/file_tree.rs`（在末尾 mod tests 之前添加 `include!` 渲染模块）

- [ ] **Step 1: 实现 FileTree widget**

`file_tree_render.rs`:

```rust
use crate::file_tree::{FileTreeState, FlatNode};
use ratatui::{
    layout::Rect,
    prelude::*,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Paragraph, StatefulWidget, Widget},
};

/// 文件树渲染 widget
pub struct FileTree<'a> {
    /// 选中行样式（背景高亮）
    cursor_style: Style,
    /// │ 连线样式（dim 色）
    line_style: Style,
    /// 目录名样式
    dir_style: Style,
    /// 文件名样式
    file_style: Style,
    /// phantom data
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> FileTree<'a> {
    pub fn new() -> Self {
        Self {
            cursor_style: Style::default(),
            line_style: Style::default(),
            dir_style: Style::default(),
            file_style: Style::default(),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn cursor_style(mut self, style: Style) -> Self {
        self.cursor_style = style;
        self
    }

    pub fn line_style(mut self, style: Style) -> Self {
        self.line_style = style;
        self
    }

    pub fn dir_style(mut self, style: Style) -> Self {
        self.dir_style = style;
        self
    }

    pub fn file_style(mut self, style: Style) -> Self {
        self.file_style = style;
        self
    }
}

impl StatefulWidget for FileTree<'_> {
    type State = FileTreeState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.height == 0 {
            return;
        }
        let visible_height = area.height;
        state.scroll.ensure_visible(state.cursor as u16, visible_height);
        let offset = state.scroll.offset() as usize;
        let cursor = state.cursor;

        let flat = state.flat();
        let lines: Vec<Line<'_>> = flat
            .iter()
            .enumerate()
            .skip(offset)
            .take(visible_height as usize)
            .map(|(i, node)| {
                let is_cursor = i == cursor;
                build_line(node, is_cursor, &self)
            })
            .collect();

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text).scroll((0, 0));
        Widget::render(paragraph, area, buf);
    }
}

/// 构建一行的渲染内容
fn build_line(node: &FlatNode, is_cursor: bool, tree: &FileTree<'_>) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    // 缩进 + │ 竖线
    for d in 0..node.depth {
        if d < node.depth {
            spans.push(Span::styled("│ ".to_string(), tree.line_style));
        }
    }

    // 展开符号 或 文件图标
    if node.is_dir {
        let marker = if node.expanded { "▾ " } else { "▸ " };
        spans.push(Span::styled(marker.to_string(), tree.dir_style));
    } else {
        // 图标占位（由 icon_fn 决定）
        spans.push(Span::styled("  ".to_string(), Style::default()));
    }

    // 名称
    let name = if node.is_dir {
        format!("{}/", node.name)
    } else {
        node.name.clone()
    };
    let name_style = if node.is_dir {
        tree.dir_style
    } else {
        tree.file_style
    };
    spans.push(Span::styled(name, name_style));

    // 选中态：整行覆盖 cursor_style
    if is_cursor {
        for span in &mut spans {
            *span = span.clone().patch_style(tree.cursor_style);
        }
    }

    Line::from(spans)
}
```

- [ ] **Step 2: 在 file_tree.rs 中引入渲染模块**

在 `file_tree.rs` 末尾 `#[cfg(test)]` 之前添加：

```rust
pub mod file_tree_render;
```

- [ ] **Step 3: 写渲染测试**

`file_tree_render_test.rs`:

```rust
use crate::file_tree::file_tree_render::FileTree;
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
fn render_shows_dir_with_slash() {
    let backend = TestBackend::new(30, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    let root = vec![make_dir("src", vec![], false)];
    let mut state = FileTreeState::new();
    state.set_root(root);
    terminal
        .draw(|f| {
            let tree = FileTree::new();
            f.render_stateful_widget(tree, Rect::new(0, 0, 30, 10), &mut state);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let row: String = (0..30).map(|x| buf.cell((x, 0)).unwrap().symbol().to_string()).collect();
    assert!(row.contains("▸"), "折叠目录应显示 ▸");
    assert!(row.contains("src/"), "目录名应有 / 后缀");
}

#[test]
fn render_shows_expanded_dir() {
    let backend = TestBackend::new(30, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    let root = vec![make_dir("src", vec![make_file("main.rs")], true)];
    let mut state = FileTreeState::new();
    state.set_root(root);
    terminal
        .draw(|f| {
            let tree = FileTree::new();
            f.render_stateful_widget(tree, Rect::new(0, 0, 30, 10), &mut state);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let row0: String = (0..30).map(|x| buf.cell((x, 0)).unwrap().symbol().to_string()).collect();
    assert!(row0.contains("▾"), "展开目录应显示 ▾");
    let row1: String = (0..30).map(|x| buf.cell((x, 1)).unwrap().symbol().to_string()).collect();
    assert!(row1.contains("main.rs"), "子文件应显示");
    assert!(row1.contains("│"), "子文件应有 │ 缩进线");
}

#[test]
fn render_cursor_highlight() {
    let backend = TestBackend::new(30, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    let root = vec![make_file("a.rs"), make_file("b.rs")];
    let mut state = FileTreeState::new();
    state.set_root(root);
    state.move_cursor(1);
    terminal
        .draw(|f| {
            let tree = FileTree::new().cursor_style(
                ratatui::style::Style::default().bg(Color::Rgb(38, 38, 38)),
            );
            f.render_stateful_widget(tree, Rect::new(0, 0, 30, 10), &mut state);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let cell = buf.cell((2, 1)).unwrap();
    assert_eq!(cell.bg, Color::Rgb(38, 38, 38), "选中行应有背景色");
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p peri-widgets -- file_tree`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/file_tree_render.rs peri-widgets/src/file_tree_render_test.rs peri-widgets/src/file_tree.rs
git commit -m "feat(peri-widgets): add FileTree StatefulWidget with virtual scroll rendering"
```

---

## Task 7: 更新 lib.rs re-export

**Files:**
- Modify: `peri-widgets/src/lib.rs`

- [ ] **Step 1: 添加 re-export**

在 `pub use scrollable::...` 之后添加：

```rust
pub use file_tree::{FileNode, FileTreeState, FlatNode, ToggleResult};
pub use file_tree::file_tree_render::FileTree;
```

- [ ] **Step 2: 运行全量测试确认无破坏**

Run: `cargo test -p peri-widgets`
Expected: ALL PASS

- [ ] **Step 3: 运行 clippy**

Run: `cargo clippy -p peri-widgets -- -D warnings`
Expected: 0 warnings

- [ ] **Step 4: Commit**

```bash
git add peri-widgets/src/lib.rs
git commit -m "feat(peri-widgets): re-export FileTree types from lib.rs"
```

---

## Self-Review Checklist

**1. Spec coverage:**
- [x] FileNode 数据结构（name, is_dir, children, expanded, loaded, path）→ Task 1
- [x] FileTreeState（new, set_root, flatten）→ Task 2
- [x] 导航（move_cursor, click, selected）→ Task 3
- [x] 展开/折叠 + 懒加载（toggle → ToggleResult, set_children）→ Task 4
- [x] 排序（目录优先 + 字母序）→ Task 5
- [x] 渲染（StatefulWidget, ▾▸, │ 缩进线, cursor 高亮）→ Task 6
- [x] 虚拟滚动 → Task 6（skip/ take by visible_height）
- [x] 文件图标闭包（icon_fn）→ Task 3（set_icon_fn + icon_for）
- [x] 模块注册 + re-export → Task 7

**2. Placeholder scan:** 无 TBD / TODO / "implement later" / 未完成的步骤。

**3. Type consistency:**
- `FileNode` 所有字段在 Task 1 定义，Task 2-6 使用一致
- `ToggleResult` 在 Task 1 定义，Task 4 的 toggle 返回值匹配
- `FlatNode.tree_path` 类型 `Vec<usize>` 在 Task 4 的 `node_by_path` 中使用一致
- `FileTreeState::click()` 返回 `Option<usize>`，测试中断言一致
