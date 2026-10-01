use crate::scrollable::ScrollState;

/// 文件树节点
#[derive(Debug, Clone)]
pub struct FileNode {
    pub name: String,
    pub is_dir: bool,
    pub children: Vec<FileNode>,
    pub expanded: bool,
    /// lazy load 标记：false 表示子节点尚未加载
    pub loaded: bool,
    pub path: Option<String>,
}

/// toggle() 返回值，通知调用方是否需要加载子节点
#[derive(Debug, Clone)]
pub struct ToggleResult {
    pub needs_load: bool,
    pub path: String,
    pub is_dir: bool,
    pub expanded: bool,
}

/// 扁平化行数据（虚拟滚动用）
#[derive(Debug, Clone, PartialEq)]
pub struct FlatNode {
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub expanded: bool,
    pub loaded: bool,
    pub path: Option<String>,
    /// 该节点在 FileNode 树中的索引路径（用于 toggle 定位）
    pub tree_path: Vec<usize>,
}

/// 文件树状态
pub struct FileTreeState {
    root: Vec<FileNode>,
    flat: Vec<FlatNode>,
    cursor: usize,
    pub scroll: ScrollState,
    icon_fn: Box<dyn Fn(&str) -> char>,
}

impl FileTreeState {
    pub fn new() -> Self {
        Self {
            root: Vec::new(),
            flat: Vec::new(),
            cursor: 0,
            scroll: ScrollState::new(),
            icon_fn: Box::new(|_| ' '),
        }
    }

    /// 设置根节点列表，重建扁平列表并钳位光标
    pub fn set_root(&mut self, root: Vec<FileNode>) {
        self.root = root;
        self.rebuild_flat();
        self.clamp_cursor();
    }

    /// 获取当前光标位置的扁平节点
    pub fn selected(&self) -> Option<&FlatNode> {
        self.flat.get(self.cursor)
    }

    /// 获取扁平列表引用
    pub fn flat(&self) -> &[FlatNode] {
        &self.flat
    }

    /// 获取当前光标位置
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// 扁平列表长度
    pub fn len(&self) -> usize {
        self.flat.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.flat.is_empty()
    }

    /// 从根节点重建扁平列表
    fn rebuild_flat(&mut self) {
        let mut flat = Vec::new();
        for (i, node) in self.root.iter().enumerate() {
            Self::flatten_node_into(&mut flat, node, 0, vec![i]);
        }
        self.flat = flat;
    }

    /// 递归展平节点到给定的 vec 中
    fn flatten_node_into(
        flat: &mut Vec<FlatNode>,
        node: &FileNode,
        depth: usize,
        tree_path: Vec<usize>,
    ) {
        flat.push(FlatNode {
            name: node.name.clone(),
            depth,
            is_dir: node.is_dir,
            expanded: node.expanded,
            loaded: node.loaded,
            path: node.path.clone(),
            tree_path,
        });
        if node.is_dir && node.expanded {
            let idx = flat.len() - 1;
            for (i, child) in node.children.iter().enumerate() {
                let mut child_path = flat[idx].tree_path.clone();
                child_path.push(i);
                Self::flatten_node_into(flat, child, depth + 1, child_path);
            }
        }
    }

    /// 将光标钳位到有效范围 [0, flat.len())
    fn clamp_cursor(&mut self) {
        if self.flat.is_empty() {
            self.cursor = 0;
        } else {
            self.cursor = self.cursor.min(self.flat.len() - 1);
        }
    }

    /// 移动光标（clamp 模式）
    pub fn move_cursor(&mut self, delta: i32) {
        if self.flat.is_empty() {
            return;
        }
        let max = self.flat.len() - 1;
        let new = self.cursor as i32 + delta;
        self.cursor = new.clamp(0, max as i32) as usize;
    }

    /// 点击某行（相对视口行号），返回 flat 索引
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
    #[allow(dead_code)]
    pub fn icon_for(&self, name: &str) -> char {
        (self.icon_fn)(name)
    }

    /// 确保 cursor 在可见视口内
    pub fn ensure_visible(&mut self, visible_height: u16) {
        self.scroll
            .ensure_visible(self.cursor as u16, visible_height);
    }

    /// 切换展开/折叠。文件返回 None。未加载目录返回 needs_load=true。
    /// 已加载空目录返回 None（不可展开）。
    pub fn toggle(&mut self, flat_idx: usize) -> Option<ToggleResult> {
        let flat_node = self.flat.get(flat_idx)?;
        if !flat_node.is_dir {
            return None;
        }
        if !flat_node.loaded {
            return Some(ToggleResult {
                needs_load: true,
                path: flat_node.path.clone()?,
                is_dir: true,
                expanded: false,
            });
        }
        let tree_path = flat_node.tree_path.clone();
        let node = self.node_by_path(&tree_path)?;
        if node.children.is_empty() {
            return None;
        }
        let new_expanded = !node.expanded;
        let path = node.path.clone()?;
        self.node_by_path_mut(&tree_path)?.expanded = new_expanded;
        self.rebuild_flat();
        self.clamp_cursor();
        Some(ToggleResult {
            needs_load: false,
            path,
            is_dir: true,
            expanded: new_expanded,
        })
    }

    /// 懒加载后填充子节点
    pub fn set_children(&mut self, dir_path: &str, children: Vec<FileNode>) {
        if let Some(node) = Self::find_node_by_path_mut(&mut self.root, dir_path) {
            node.children = children;
            node.loaded = true;
            self.rebuild_flat();
            self.clamp_cursor();
        }
    }

    /// 按 tree_path 查找节点（不可变）
    fn node_by_path(&self, path: &[usize]) -> Option<&FileNode> {
        let idx = *path.first()?;
        let node = self.root.get(idx)?;
        path.iter()
            .skip(1)
            .try_fold(node, |cur, &i| cur.children.get(i))
    }

    /// 按 tree_path 查找节点（可变）
    fn node_by_path_mut(&mut self, path: &[usize]) -> Option<&mut FileNode> {
        let idx = *path.first()?;
        let node = self.root.get_mut(idx)?;
        if path.len() == 1 {
            return Some(node);
        }
        path.iter()
            .skip(1)
            .try_fold(node, |cur, &i| cur.children.get_mut(i))
    }

    /// 按完整路径字符串 DFS 查找（可变）
    fn find_node_by_path_mut<'a>(
        nodes: &'a mut [FileNode],
        target: &str,
    ) -> Option<&'a mut FileNode> {
        for node in nodes.iter_mut() {
            if node.path.as_deref() == Some(target) {
                return Some(node);
            }
            if let found @ Some(_) = Self::find_node_by_path_mut(&mut node.children, target) {
                return found;
            }
        }
        None
    }

    /// 排序：目录优先，同类型按字母序（递归）
    pub fn sort(&mut self) {
        Self::sort_nodes(&mut self.root);
        self.rebuild_flat();
    }

    fn sort_nodes(nodes: &mut [FileNode]) {
        nodes.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });
        for node in nodes.iter_mut() {
            if node.is_dir {
                Self::sort_nodes(&mut node.children);
            }
        }
    }
}

impl Default for FileTreeState {
    fn default() -> Self {
        Self::new()
    }
}

pub mod render;

#[path = "file_tree_test.rs"]
#[cfg(test)]
mod tests;
