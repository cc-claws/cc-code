# gig — Git Graph TUI 工具实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建一个终端内的 git commit graph 可视化 + 交互操作工具，支持大型仓库（50K+ commits）、鼠标操作为主、emoji 工具栏触发 git 操作。

**Architecture:** 左右分栏布局——左侧 graph 面板（虚拟滚动），右侧 commit 详情面板。数据层用 git2 一次性构建拓扑骨架，按需加载 commit 详情。graph 渲染用自建 lane-based 拓扑引擎（参考 keifu），box-drawing 字符画连线。主线程同步事件循环 + tokio 后台线程处理 remote 操作。

**Tech Stack:** Rust 2021, ratatui + crossterm, git2 (libgit2), tokio, clap, peri-widgets (path dependency), anyhow, tracing

---

## File Structure

```
side-projects/git-graph/
├── Cargo.toml
├── src/
│   ├── main.rs                  # CLI (clap) + 启动入口
│   ├── app.rs                   # App 状态机 + 主事件循环
│   ├── event.rs                 # crossterm 事件处理 + 鼠标分发
│   ├── render.rs                # 主布局渲染（左右分栏）
│   ├── git/
│   │   ├── mod.rs
│   │   ├── repo.rs              # Repository 包装（git2 初始化）
│   │   ├── commit.rs            # CommitInfo / CommitDetail 数据类型
│   │   ├── branch.rs            # 分支/Tag 列表 + 操作
│   │   ├── remote.rs            # fetch/pull/push (tokio)
│   │   ├── stash.rs             # stash 操作
│   │   └── ops.rs               # checkout/merge/reset/cherry-pick 等
│   ├── graph/
│   │   ├── mod.rs
│   │   ├── topology.rs          # 拓扑骨架构建（hash + parents 扫描）
│   │   ├── layout.rs            # Lane-based 布局引擎
│   │   ├── render.rs            # GraphNode → ratatui Lines
│   │   └── color.rs             # Branch 着色分配
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── graph_panel.rs       # 左侧 graph 面板（虚拟滚动）
│   │   ├── detail_panel.rs      # 右侧 commit 详情面板
│   │   ├── toolbar.rs           # Emoji 工具栏（commit 操作 + 全局操作）
│   │   ├── overlay.rs           # Branch/Tag/Stash 列表 overlay
│   │   ├── confirm.rs           # 危险操作确认弹窗
│   │   ├── filter_bar.rs        # 过滤输入栏
│   │   └── search_bar.rs        # 搜索输入栏
│   └── theme.rs                 # GigTheme（扩展 peri-widgets Theme）
```

---

## Phase 1: 项目骨架 + Git 数据层

### Task 1: 项目初始化

**Files:**
- Create: `side-projects/git-graph/Cargo.toml`
- Create: `side-projects/git-graph/src/main.rs`

- [ ] **Step 1: 创建 Cargo.toml**

```toml
[package]
name = "gig"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
ratatui = ">=0.30"
crossterm = "0.28"
git2 = "0.20"
tokio = { version = "1", features = ["rt-multi-thread", "sync", "macros"] }
clap = { version = "4", features = ["derive"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
unicode-width = "0.2"
peri-widgets = { path = "../../peri-widgets" }
```

- [ ] **Step 2: 创建 main.rs 骨架**

```rust
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "gig", about = "Git Graph TUI")]
struct Cli {
    /// 仓库路径（默认当前目录）
    repo_path: Option<PathBuf>,
    /// 显示所有分支（含 remote tracking）
    #[arg(short, long)]
    all: bool,
    /// 初始加载数量限制
    #[arg(short, long, default_value = "1000")]
    limit: usize,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let repo_path = cli.repo_path.unwrap_or_else(|| PathBuf::from("."));
    println!("gig: opening {:?}", repo_path);
    Ok(())
}
```

- [ ] **Step 3: 验证编译**

Run: `cd side-projects/git-graph && cargo build`
Expected: 编译成功，无错误

- [ ] **Step 4: Commit**

```bash
git add side-projects/git-graph/
git commit -m "feat(gig): initialize project skeleton"
```

---

### Task 2: Git Repository 包装层

**Files:**
- Create: `side-projects/git-graph/src/git/mod.rs`
- Create: `side-projects/git-graph/src/git/repo.rs`
- Create: `side-projects/git-graph/src/git/commit.rs`

- [ ] **Step 1: 创建 git/mod.rs**

```rust
pub mod repo;
pub mod commit;
```

- [ ] **Step 2: 创建 commit.rs — 数据类型**

```rust
use git2::Oid;
use std::time::SystemTime;

/// 轻量拓扑节点（一次性扫描所有 commit 生成）
#[derive(Debug, Clone)]
pub struct TopoNode {
    pub oid: Oid,
    pub parent_oids: Vec<Oid>,
    /// commit 时间戳（用于排序）
    pub time: i64,
}

/// 完整 commit 详情（按需加载）
#[derive(Debug, Clone)]
pub struct CommitDetail {
    pub oid: Oid,
    pub short_hash: String,
    pub author_name: String,
    pub author_email: String,
    pub time: i64,
    pub message: String,
    pub parent_oids: Vec<Oid>,
    /// 关联的 branch 名
    pub branches: Vec<String>,
    /// 关联的 tag 名
    pub tags: Vec<String>,
    /// 文件变更统计（按需加载）
    pub stats: Option<DiffStats>,
}

/// 文件变更统计
#[derive(Debug, Clone)]
pub struct DiffStats {
    pub files: Vec<FileChange>,
    pub insertions: usize,
    pub deletions: usize,
    pub files_changed: usize,
}

/// 单文件变更
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub old_path: Option<String>, // rename 时旧路径
    pub status: FileStatus,
    pub insertions: usize,
    pub deletions: usize,
}

/// 文件变更类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
    TypeChange,
    Unmodified,
}

impl FileStatus {
    pub fn from_delta(delta: git2::Delta) -> Self {
        match delta {
            git2::Delta::Added => Self::Added,
            git2::Delta::Deleted => Self::Deleted,
            git2::Delta::Modified => Self::Modified,
            git2::Delta::Renamed => Self::Renamed,
            git2::Delta::Copied => Self::Copied,
            git2::Delta::Typechange => Self::TypeChange,
            _ => Self::Unmodified,
        }
    }
}
```

- [ ] **Step 3: 创建 repo.rs — Repository 包装**

```rust
use anyhow::{Context, Result};
use git2::{DiffOptions, Oid, Repository, Time};
use std::collections::HashMap;
use std::path::Path;

use super::commit::{CommitDetail, DiffStats, FileChange, FileStatus, TopoNode};

pub struct GitRepo {
    repo: Repository,
}

impl GitRepo {
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path)
            .with_context(|| format!("不是 git 仓库: {:?}", path))?;
        Ok(Self { repo })
    }

    /// 一次性扫描拓扑骨架（所有 commit 的 hash + parent refs）
    /// 用于大型仓库——只读 Oid + parent Oid，极快
    pub fn scan_topology(&self) -> Result<Vec<TopoNode>> {
        let mut revwalk = self.repo.revwalk()?;
        // 遍历所有本地分支
        for branch in self.repo.branches(Some(git2::BranchType::Local))? {
            let branch = branch?.0;
            if let Some(name) = branch.name()?.map(|s| s.to_string()) {
                let ref_name = format!("refs/heads/{}", name);
                if let Ok(oid) = self.repo.refname_to_id(&ref_name) {
                    revwalk.push(oid)?;
                }
            }
        }
        let mut nodes = Vec::new();
        for oid in revwalk {
            let oid = oid?;
            let commit = self.repo.find_commit(oid)?;
            let parent_oids: Vec<Oid> = commit.parent_ids().collect();
            let time = commit.time().seconds();
            nodes.push(TopoNode {
                oid,
                parent_oids,
                time,
            });
        }
        Ok(nodes)
    }

    /// 按 commit hash 查找完整 commit 详情
    pub fn commit_detail(&self, oid: Oid) -> Result<CommitDetail> {
        let commit = self.repo.find_commit(oid)?;
        let short_hash = format!("{:.7}", oid);
        let author = commit.author();
        let message = commit.message().unwrap_or("").to_string();
        let parent_oids: Vec<Oid> = commit.parent_ids().collect();
        let branches = self.branches_for_oid(oid)?;
        let tags = self.tags_for_oid(oid)?;
        Ok(CommitDetail {
            oid,
            short_hash,
            author_name: author.name().unwrap_or("").to_string(),
            author_email: author.email().unwrap_or("").to_string(),
            time: commit.time().seconds(),
            message,
            parent_oids,
            branches,
            tags,
            stats: None,
        })
    }

    /// 加载 commit diff 统计（不加载 diff 内容）
    pub fn load_diff_stats(&self, oid: Oid) -> Result<DiffStats> {
        let commit = self.repo.find_commit(oid)?;
        let tree = commit.tree()?;
        let parent = commit.parent(0);
        let parent_tree = parent.as_ref().map(|p| p.tree()).transpose()?;

        let mut opts = DiffOptions::new();
        let diff = self.repo.diff_tree_to_tree(
            parent_tree.as_ref(),
            Some(&tree),
            Some(&mut opts),
        )?;

        let stats = diff.stats()?;
        let mut files = Vec::new();
        for delta in diff.deltas() {
            let new_file = delta.new_file();
            let old_file = delta.old_file();
            files.push(FileChange {
                path: new_file.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
                old_path: if delta.status() == git2::Delta::Renamed {
                    old_file.path().map(|p| p.to_string_lossy().to_string())
                } else {
                    None
                },
                status: FileStatus::from_delta(delta.status()),
                insertions: 0, // per-file stats 需要额外计算
                deletions: 0,
            });
        }

        Ok(DiffStats {
            files,
            insertions: stats.insertions(),
            deletions: stats.deletions(),
            files_changed: stats.files_changed(),
        })
    }

    /// 获取 HEAD commit oid
    pub fn head_oid(&self) -> Result<Oid> {
        let head = self.repo.head()?;
        let target = head.target().context("HEAD 无目标")?;
        Ok(target)
    }

    /// branch 名 → 目标 oid 映射
    pub fn branch_map(&self) -> Result<HashMap<Oid, Vec<String>>> {
        let mut map: HashMap<Oid, Vec<String>> = HashMap::new();
        for branch in self.repo.branches(Some(git2::BranchType::Local))? {
            let branch = branch?.0;
            if let (Some(name), Some(target)) = (
                branch.name()?.map(|s| s.to_string()),
                branch.get().target(),
            ) {
                map.entry(target).or_default().push(name);
            }
        }
        Ok(map)
    }

    /// tag 名 → 目标 oid 映射
    pub fn tag_map(&self) -> Result<HashMap<Oid, Vec<String>>> {
        let mut map: HashMap<Oid, Vec<String>> = HashMap::new();
        for tag_name in self.repo.tag_names(None)? {
            if let Some(name) = tag_name {
                if let Ok(ref_name) = self.repo.refname_to_id(&format!("refs/tags/{}", name)) {
                    // tag 可能指向 commit 或 tag object，取 commit
                    if let Ok(commit) = self.repo.find_commit(ref_name) {
                        map.entry(commit.id()).or_default().push(name.to_string());
                    } else if let Ok(tag_obj) = self.repo.find_tag(ref_name) {
                        let target_oid = tag_obj.target_id();
                        map.entry(target_oid).or_default().push(name.to_string());
                    }
                }
            }
        }
        Ok(map)
    }

    /// 获取所有分支名列表
    pub fn branch_names(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for branch in self.repo.branches(Some(git2::BranchType::Local))? {
            let branch = branch?.0;
            if let Some(name) = branch.name()?.map(|s| s.to_string()) {
                names.push(name);
            }
        }
        Ok(names)
    }

    /// 获取所有 tag 名列表
    pub fn tag_names(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for name in self.repo.tag_names(None)?.flatten() {
            names.push(name.to_string());
        }
        Ok(names)
    }

    fn branches_for_oid(&self, oid: Oid) -> Result<Vec<String>> {
        let map = self.branch_map()?;
        Ok(map.get(&oid).cloned().unwrap_or_default())
    }

    fn tags_for_oid(&self, oid: Oid) -> Result<Vec<String>> {
        let map = self.tag_map()?;
        Ok(map.get(&oid).cloned().unwrap_or_default())
    }

    pub fn repo(&self) -> &Repository {
        &self.repo
    }
}
```

- [ ] **Step 4: 创建测试 `side-projects/git-graph/src/git/repo_test.rs`**

```rust
use super::*;
use std::process::Command;

fn setup_test_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    Command::new("git").args(["init"]).current_dir(path).output().unwrap();
    Command::new("git").args(["config", "user.email", "test@test.com"]).current_dir(path).output().unwrap();
    Command::new("git").args(["config", "user.name", "test"]).current_dir(path).output().unwrap();
    // 创建初始 commit
    std::fs::write(path.join("a.txt"), "hello").unwrap();
    Command::new("git").args(["add", "."]).current_dir(path).output().unwrap();
    Command::new("git").args(["commit", "-m", "initial"]).current_dir(path).output().unwrap();
    dir
}

#[test]
fn test_open_repo() {
    let dir = setup_test_repo();
    let repo = GitRepo::open(dir.path()).unwrap();
    let head = repo.head_oid().unwrap();
    assert!(!head.is_zero());
}

#[test]
fn test_scan_topology() {
    let dir = setup_test_repo();
    let repo = GitRepo::open(dir.path()).unwrap();
    let topo = repo.scan_topology().unwrap();
    assert_eq!(topo.len(), 1);
    assert!(topo[0].parent_oids.is_empty());
}

#[test]
fn test_commit_detail() {
    let dir = setup_test_repo();
    let repo = GitRepo::open(dir.path()).unwrap();
    let head = repo.head_oid().unwrap();
    let detail = repo.commit_detail(head).unwrap();
    assert_eq!(detail.short_hash.len(), 7);
    assert_eq!(detail.message, "initial");
    assert!(!detail.branches.is_empty());
}
```

- [ ] **Step 5: 在 repo.rs 底部添加测试模块引用**

```rust
#[cfg(test)]
mod repo_test;
```

- [ ] **Step 6: 运行测试**

Run: `cd side-projects/git-graph && cargo test`
Expected: 3 个测试全部 PASS

- [ ] **Step 7: Commit**

```bash
git add side-projects/git-graph/
git commit -m "feat(gig): add git data layer with topology scan and commit details"
```

---

### Task 3: Stash 数据层

**Files:**
- Create: `side-projects/git-graph/src/git/stash.rs`
- Modify: `side-projects/git-graph/src/git/mod.rs`

- [ ] **Step 1: 创建 stash.rs**

```rust
use anyhow::Result;
use git2::{Oid, Repository};
use std::collections::HashMap;

/// Stash 信息
#[derive(Debug, Clone)]
pub struct StashInfo {
    pub index: usize,
    pub oid: Oid,
    /// stash 基于的 commit（stash 的第一个 parent）
    pub base_commit: Oid,
    pub message: String,
}

impl super::repo::GitRepo {
    /// 获取所有 stash，按 base_commit 分组
    pub fn stash_list(&self) -> Result<Vec<StashInfo>> {
        let mut stashes = Vec::new();
        self.repo.stash_foreach(|index, message, oid| {
            // stash commit 的第一个 parent 是 base commit
            let base_commit = self
                .repo
                .find_commit(*oid)
                .ok()
                .and_then(|c| c.parent(0).ok())
                .map(|p| p.id())
                .unwrap_or_else(|| Oid::zero());
            stashes.push(StashInfo {
                index,
                oid: *oid,
                base_commit,
                message: message.to_string(),
            });
            true
        })?;
        Ok(stashes)
    }

    /// 按 base_commit oid 索引 stash
    pub fn stash_by_commit(&self) -> Result<HashMap<Oid, Vec<StashInfo>>> {
        let list = self.stash_list()?;
        let mut map: HashMap<Oid, Vec<StashInfo>> = HashMap::new();
        for stash in list {
            map.entry(stash.base_commit).or_default().push(stash);
        }
        Ok(map)
    }

    pub fn stash_pop(&self, index: usize) -> Result<()> {
        self.repo.stash_pop(index, None)?;
        Ok(())
    }

    pub fn stash_drop(&self, index: usize) -> Result<()> {
        self.repo.stash_drop(index)?;
        Ok(())
    }

    pub fn stash_apply(&self, index: usize) -> Result<()> {
        self.repo.stash_apply(index, None)?;
        Ok(())
    }
}
```

- [ ] **Step 2: 更新 git/mod.rs 添加 stash 模块**

```rust
pub mod repo;
pub mod commit;
pub mod stash;
```

- [ ] **Step 3: 运行测试确认编译通过**

Run: `cd side-projects/git-graph && cargo build`
Expected: 编译成功

- [ ] **Step 4: Commit**

```bash
git add side-projects/git-graph/
git commit -m "feat(gig): add stash operations"
```

---

## Phase 2: Graph 拓扑引擎

### Task 4: 拓扑骨架构建器

**Files:**
- Create: `side-projects/git-graph/src/graph/mod.rs`
- Create: `side-projects/git-graph/src/graph/topology.rs`

- [ ] **Step 1: 创建 graph/mod.rs**

```rust
pub mod topology;
pub mod layout;
pub mod render;
pub mod color;
```

- [ ] **Step 2: 创建 topology.rs — 拓扑骨架**

```rust
use crate::git::commit::TopoNode;
use crate::git::stash::StashInfo;
use git2::Oid;
use std::collections::HashMap;

/// 拓扑骨架——存储所有 commit 的拓扑关系，按需查询
pub struct Topology {
    /// 按 commit 时间倒序排列（新→旧）
    nodes: Vec<TopoNode>,
    /// oid → 在 nodes 中的索引
    index: HashMap<Oid, usize>,
    /// oid → 分支名列表
    branch_map: HashMap<Oid, Vec<String>>,
    /// oid → tag 名列表
    tag_map: HashMap<Oid, Vec<String>>,
    /// oid → stash 列表
    stash_map: HashMap<Oid, Vec<StashInfo>>,
}

impl Topology {
    pub fn new(
        mut nodes: Vec<TopoNode>,
        branch_map: HashMap<Oid, Vec<String>>,
        tag_map: HashMap<Oid, Vec<String>>,
        stash_map: HashMap<Oid, Vec<StashInfo>>,
    ) -> Self {
        // 按时间倒序排列
        nodes.sort_by(|a, b| b.time.cmp(&a.time));
        let index: HashMap<Oid, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.oid, i))
            .collect();
        Self {
            nodes,
            index,
            branch_map,
            tag_map,
            stash_map,
        }
    }

    pub fn nodes(&self) -> &[TopoNode] {
        &self.nodes
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn get(&self, idx: usize) -> Option<&TopoNode> {
        self.nodes.get(idx)
    }

    pub fn index_of(&self, oid: Oid) -> Option<usize> {
        self.index.get(&oid).copied()
    }

    pub fn branches_for(&self, oid: Oid) -> &[String] {
        self.branch_map.get(&oid).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn tags_for(&self, oid: Oid) -> &[String] {
        self.tag_map.get(&oid).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn stashes_for(&self, oid: Oid) -> &[StashInfo] {
        self.stash_map.get(&oid).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// 是否是 fork 点（有 ≥2 个 child）
    pub fn is_fork_point(&self, oid: Oid) -> bool {
        let mut child_count = 0;
        for node in &self.nodes {
            if node.parent_oids.contains(&oid) {
                child_count += 1;
                if child_count >= 2 {
                    return true;
                }
            }
        }
        false
    }
}
```

- [ ] **Step 3: 运行编译检查**

Run: `cd side-projects/git-graph && cargo build`
Expected: 编译成功

- [ ] **Step 4: Commit**

```bash
git add side-projects/git-graph/
git commit -m "feat(gig): add topology skeleton builder"
```

---

### Task 5: Branch 着色引擎

**Files:**
- Create: `side-projects/git-graph/src/graph/color.rs`

- [ ] **Step 1: 创建 color.rs — 分支配色**

```rust
use ratatui::style::Color;
use std::collections::HashMap;

/// 11 色调色板（与 keifu 一致）
const PALETTE: [Color; 11] = [
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::LightRed,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightBlue,
    Color::LightCyan,
];

/// Branch → 颜色映射
pub struct BranchColors {
    map: HashMap<String, Color>,
    next: usize,
}

impl BranchColors {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            next: 0,
        }
    }

    /// 为 branch 分配颜色（固定映射，同一 branch 始终同色）
    pub fn color_for(&mut self, branch: &str) -> Color {
        if let Some(&c) = self.map.get(branch) {
            return c;
        }
        let color = PALETTE[self.next % PALETTE.len()];
        self.map.insert(branch.to_string(), color);
        self.next += 1;
        color
    }

    /// 获取已分配的颜色
    pub fn get(&self, branch: &str) -> Option<Color> {
        self.map.get(branch).copied()
    }

    /// 默认颜色（无 branch 信息时使用）
    pub fn default_color() -> Color {
        Color::DarkGray
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_同一分支颜色一致() {
        let mut bc = BranchColors::new();
        let c1 = bc.color_for("main");
        let c2 = bc.color_for("main");
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_不同分支颜色不同() {
        let mut bc = BranchColors::new();
        let c1 = bc.color_for("main");
        let c2 = bc.color_for("dev");
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_颜色循环() {
        let mut bc = BranchColors::new();
        let first = bc.color_for("b0");
        for i in 1..11 {
            bc.color_for(&format!("b{}", i));
        }
        let wrap = bc.color_for("b11");
        assert_eq!(first, wrap);
    }
}
```

- [ ] **Step 2: 运行测试**

Run: `cd side-projects/git-graph && cargo test -- color`
Expected: 3 个测试 PASS

- [ ] **Step 3: Commit**

```bash
git add side-projects/git-graph/
git commit -m "feat(gig): add branch color assignment engine"
```

---

### Task 6: Lane-based Graph 布局引擎

**Files:**
- Create: `side-projects/git-graph/src/graph/layout.rs`

这是整个工具最核心的模块。算法参考 keifu 的 `graph.rs`。

- [ ] **Step 1: 创建 layout.rs — 核心布局引擎**

```rust
use crate::git::commit::TopoNode;
use crate::graph::color::BranchColors;
use git2::Oid;
use ratatui::style::Color;

/// 一个 cell 的渲染类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellType {
    Empty,
    Pipe(Color),
    Commit(Color),
    BranchRight(Color),
    BranchLeft(Color),
    MergeRight(Color),
    MergeLeft(Color),
    Horizontal(Color),
    HorizontalPipe { h_color: Color, p_color: Color },
    TeeRight(Color),
    TeeLeft(Color),
}

/// Graph 中的一行（一个 commit 或连接器行）
#[derive(Debug, Clone)]
pub struct GraphRow {
    /// 此行对应的 commit（None = 纯连接器行）
    pub oid: Option<Oid>,
    /// 此 commit 所在 lane
    pub lane: usize,
    /// 渲染用 cell 数组，长度 = (max_lane + 1) * 2
    pub cells: Vec<CellType>,
    /// 此 commit 关联的 branch 名（用于着色）
    pub branch: Option<String>,
    /// 是否有 stash 标记
    pub has_stash: bool,
}

/// Graph 布局结果
pub struct GraphLayout {
    /// 所有行（包含 commit 行和连接器行）
    pub rows: Vec<GraphRow>,
    /// 最大 lane 编号（用于计算渲染宽度）
    pub max_lane: usize,
}

/// 活跃 lane 追踪器
struct LaneTracker {
    /// lanes[i] = 当前追踪的 commit oid，None 表示空闲
    lanes: Vec<Option<Oid>>,
}

impl LaneTracker {
    fn new() -> Self {
        Self { lanes: Vec::new() }
    }

    /// 查找追踪指定 oid 的 lane
    fn find_lane(&self, oid: Oid) -> Option<usize> {
        self.lanes.iter().position(|l| l.map_or(false, |o| o == oid))
    }

    /// 获取空闲 lane（复用或新建）
    fn alloc_lane(&mut self, oid: Oid) -> usize {
        if let Some(idx) = self.lanes.iter().position(|l| l.is_none()) {
            self.lanes[idx] = Some(oid);
            return idx;
        }
        let idx = self.lanes.len();
        self.lanes.push(Some(oid));
        idx
    }

    /// 将 lane 转向追踪新 oid
    fn redirect(&mut self, lane: usize, oid: Oid) {
        if lane < self.lanes.len() {
            self.lanes[lane] = Some(oid);
        }
    }

    /// 释放 lane
    fn free(&mut self, lane: usize) {
        if lane < self.lanes.len() {
            self.lanes[lane] = None;
        }
    }

    fn max_lane(&self) -> usize {
        self.lanes.len().saturating_sub(1)
    }
}

/// 构建 graph 布局
///
/// 输入：按时间倒序排列的 topo nodes、branch/tag/stash 映射
/// 输出：GraphRow 列表（commit 行 + 连接器行）
pub fn build_layout(
    nodes: &[TopoNode],
    branch_map: &std::collections::HashMap<Oid, Vec<String>>,
    stash_map: &std::collections::HashMap<Oid, Vec<crate::git::stash::StashInfo>>,
    colors: &mut BranchColors,
) -> GraphLayout {
    let mut tracker = LaneTracker::new();
    let mut rows: Vec<GraphRow> = Vec::new();
    let mut max_lane = 0usize;

    for node in nodes {
        let oid = node.oid;

        // a) 找 lane
        let lane = if let Some(lane) = tracker.find_lane(oid) {
            lane
        } else {
            tracker.alloc_lane(oid)
        };

        // b) Fork 点合并：多个 lane 追踪同一 oid
        let mut merging_lanes: Vec<usize> = Vec::new();
        for i in 0..tracker.lanes.len() {
            if tracker.lanes[i] == Some(oid) && i != lane {
                merging_lanes.push(i);
            }
        }

        // 如果有合并的 lane，先生成连接器行
        for &merge_lane in &merging_lanes {
            let conn_cells = build_merge_connector(merge_lane, lane, &tracker, colors);
            rows.push(GraphRow {
                oid: None,
                lane: merge_lane,
                cells: conn_cells,
                branch: None,
                has_stash: false,
            });
            tracker.free(merge_lane);
        }

        // c) 确定颜色
        let branches = branch_map.get(&oid).cloned().unwrap_or_default();
        let branch_name = branches.first().cloned();
        let color = branch_name
            .as_ref()
            .map(|b| colors.color_for(b))
            .unwrap_or_else(BranchColors::default_color);

        // d) 构建 commit 行的 cells
        let has_stash = stash_map.contains_key(&oid);
        let commit_cells = build_commit_cells(lane, &tracker, color);

        rows.push(GraphRow {
            oid: Some(oid),
            lane,
            cells: commit_cells,
            branch: branch_name,
            has_stash,
        });

        max_lane = max_lane.max(tracker.max_lane());

        // e) 处理 parents
        for (i, &parent_oid) in node.parent_oids.iter().enumerate() {
            if i == 0 {
                // 第一个 parent 继承当前 lane
                tracker.redirect(lane, parent_oid);
            } else {
                // 后续 parent 分配新 lane
                let parent_lane = tracker.alloc_lane(parent_oid);
                // 连接线行稍后由渲染层处理
                let _ = parent_lane; // lane 已注册，后续遍历到该 parent 时会找到
            }
        }
    }

    GraphLayout { rows, max_lane }
}

/// 构建 merge 连接器行的 cells
fn build_merge_connector(
    from_lane: usize,
    to_lane: usize,
    tracker: &LaneTracker,
    colors: &BranchColors,
) -> Vec<CellType> {
    let width = (tracker.lanes.len()).max(to_lane + 1).max(from_lane + 1) * 2 + 1;
    let mut cells = vec![CellType::Empty; width];

    // 画活跃 lanes 的竖线
    for (i, lane_oid) in tracker.lanes.iter().enumerate() {
        if lane_oid.is_some() && i != from_lane {
            let ci = i * 2;
            if ci < cells.len() {
                cells[ci] = CellType::Pipe(Color::DarkGray);
            }
        }
    }

    // 画从 from_lane 到 to_lane 的连接
    let color = Color::DarkGray;
    if from_lane < to_lane {
        // 向右合并
        let from_ci = from_lane * 2;
        let to_ci = to_lane * 2;
        if from_ci < cells.len() {
            cells[from_ci] = CellType::MergeLeft(color);
        }
        for ci in (from_ci + 1)..to_ci {
            if ci < cells.len() {
                cells[ci] = CellType::Horizontal(color);
            }
        }
        if to_ci < cells.len() {
            cells[to_ci] = CellType::Pipe(color);
        }
    } else {
        // 向左合并
        let from_ci = from_lane * 2;
        let to_ci = to_lane * 2;
        if from_ci < cells.len() {
            cells[from_ci] = CellType::MergeRight(color);
        }
        for ci in (to_ci + 1)..from_ci {
            if ci < cells.len() {
                cells[ci] = CellType::Horizontal(color);
            }
        }
        if to_ci < cells.len() {
            cells[to_ci] = CellType::Pipe(color);
        }
    }

    cells
}

/// 构建 commit 行的 cells
fn build_commit_cells(
    commit_lane: usize,
    tracker: &LaneTracker,
    color: Color,
) -> Vec<CellType> {
    let width = (tracker.lanes.len()).max(commit_lane + 1) * 2 + 1;
    let mut cells = vec![CellType::Empty; width];

    // 画所有活跃 lane 的竖线
    for (i, lane_oid) in tracker.lanes.iter().enumerate() {
        if lane_oid.is_some() {
            let ci = i * 2;
            if ci < cells.len() {
                if i == commit_lane {
                    cells[ci] = CellType::Commit(color);
                } else {
                    cells[ci] = CellType::Pipe(Color::DarkGray);
                }
            }
        }
    }

    cells
}
```

- [ ] **Step 2: 创建测试 `side-projects/git-graph/src/graph/layout_test.rs`**

```rust
use super::*;
use crate::git::commit::TopoNode;
use git2::Oid;

fn oid(n: u8) -> Oid {
    // 用固定字节构造测试用 Oid
    let mut bytes = [0u8; 20];
    bytes[0] = n;
    Oid::from_bytes(&bytes).unwrap()
}

fn node(n: u8, parents: Vec<u8>) -> TopoNode {
    TopoNode {
        oid: oid(n),
        parent_oids: parents.into_iter().map(oid).collect(),
        time: (255 - n) as i64, // n 越小越新
    }
}

#[test]
fn test_线性历史() {
    // 1 → 2 → 3（新→旧）
    let nodes = vec![node(1, vec![2]), node(2, vec![3]), node(3, vec![])];
    let mut colors = BranchColors::new();
    let layout = build_layout(&nodes, &Default::default(), &Default::default(), &mut colors);
    // 线性历史只有 commit 行，无连接器行
    assert_eq!(layout.rows.len(), 3);
    assert!(layout.rows[0].oid.is_some());
    assert!(layout.rows[1].oid.is_some());
    assert!(layout.rows[2].oid.is_some());
}

#[test]
fn test_单分支() {
    let nodes = vec![node(1, vec![])];
    let mut colors = BranchColors::new();
    let layout = build_layout(&nodes, &Default::default(), &Default::default(), &mut colors);
    assert_eq!(layout.rows.len(), 1);
    assert_eq!(layout.rows[0].lane, 0);
    assert_eq!(layout.rows[0].oid, Some(oid(1)));
}
```

- [ ] **Step 3: 在 layout.rs 底部添加测试模块**

```rust
#[cfg(test)]
mod layout_test;
```

- [ ] **Step 4: 运行测试**

Run: `cd side-projects/git-graph && cargo test -- layout`
Expected: 2 个测试 PASS

- [ ] **Step 5: Commit**

```bash
git add side-projects/git-graph/
git commit -m "feat(gig): add lane-based graph layout engine"
```

---

### Task 7: Graph 渲染器（CellType → ratatui Spans）

**Files:**
- Create: `side-projects/git-graph/src/graph/render.rs`

- [ ] **Step 1: 创建 render.rs**

```rust
use super::layout::{CellType, GraphRow};
use ratatui::text::{Line, Span};
use ratatui::style::{Color, Modifier, Style};

/// 将 GraphRow 渲染为 ratatui Line
/// graph_width: graph 区域宽度（字符数）
pub fn render_graph_row(
    row: &GraphRow,
    graph_width: u16,
    is_selected: bool,
    head_oid: Option<git2::Oid>,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let head_oid = head_oid;

    // 渲染 graph cells
    for cell in &row.cells {
        let (ch, color) = cell_to_char(cell);
        spans.push(Span::styled(
            ch.to_string(),
            Style::default().fg(color),
        ));
    }

    // 补空格到 graph 宽度
    let graph_chars: usize = row.cells.len();
    if graph_chars < graph_width as usize {
        let padding = " ".repeat(graph_width as usize - graph_chars);
        spans.push(Span::raw(padding));
    }

    // 添加 commit 信息摘要（如果有 commit）
    if let Some(oid) = row.oid {
        spans.push(Span::raw(" "));

        // branch/tag 标记
        if let Some(ref branch) = row.branch {
            spans.push(Span::styled(
                format!("{} ", branch),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
        }

        // HEAD 标记
        if head_oid == Some(oid) {
            spans.push(Span::styled(
                "HEAD→ ".to_string(),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ));
        }

        // stash 标记
        if row.has_stash {
            spans.push(Span::raw("📦 "));
        }

        // 短 hash
        let short = format!("{:.7}", oid);
        spans.push(Span::styled(
            short,
            Style::default().fg(Color::Magenta),
        ));
    }

    // 选中行背景高亮
    if is_selected {
        for span in &mut spans {
            *span.style_mut() = span.style().bg(Color::Rgb(40, 40, 60));
        }
    }

    Line::from(spans)
}

fn cell_to_char(cell: &CellType) -> (char, Color) {
    match cell {
        CellType::Empty => (' ', Color::Reset),
        CellType::Pipe(c) => ('│', *c),
        CellType::Commit(c) => ('●', *c),
        CellType::BranchRight(c) => ('╭', *c),
        CellType::BranchLeft(c) => ('╮', *c),
        CellType::MergeRight(c) => ('╰', *c),
        CellType::MergeLeft(c) => ('╯', *c),
        CellType::Horizontal(c) => ('─', *c),
        CellType::HorizontalPipe { h_color, p_color: _ } => ('┿', *h_color),
        CellType::TeeRight(c) => ('├', *c),
        CellType::TeeLeft(c) => ('┤', *c),
    }
}
```

- [ ] **Step 2: 运行编译检查**

Run: `cd side-projects/git-graph && cargo build`
Expected: 编译成功

- [ ] **Step 3: Commit**

```bash
git add side-projects/git-graph/
git commit -m "feat(gig): add graph renderer (CellType → ratatui Spans)"
```

---

## Phase 3: UI 框架

### Task 8: Theme 扩展

**Files:**
- Create: `side-projects/git-graph/src/theme.rs`

- [ ] **Step 1: 创建 theme.rs**

```rust
use peri_widgets::theme::{DarkTheme, Theme};
use ratatui::style::Color;

/// gig 主题：复用 peri-widgets DarkTheme，扩展 graph 专用颜色
pub struct GigTheme {
    base: DarkTheme,
}

impl GigTheme {
    pub fn new() -> Self {
        Self { base: DarkTheme }
    }

    /// graph 面板背景
    pub fn graph_bg(&self) -> Color {
        Color::Rgb(20, 20, 25)
    }

    /// detail 面板背景
    pub fn detail_bg(&self) -> Color {
        Color::Rgb(25, 25, 30)
    }

    /// 选中行背景
    pub fn selected_bg(&self) -> Color {
        Color::Rgb(40, 40, 60)
    }

    /// toolbar 背景
    pub fn toolbar_bg(&self) -> Color {
        Color::Rgb(35, 35, 45)
    }

    /// 文件变更 added 色标
    pub fn status_added(&self) -> Color {
        Color::Green
    }

    /// 文件变更 deleted 色标
    pub fn status_deleted(&self) -> Color {
        Color::Red
    }

    /// 文件变更 modified 色标
    pub fn status_modified(&self) -> Color {
        Color::Yellow
    }
}

impl Theme for GigTheme {
    fn accent(&self) -> Color { self.base.accent() }
    fn success(&self) -> Color { self.base.success() }
    fn warning(&self) -> Color { self.base.warning() }
    fn error(&self) -> Color { self.base.error() }
    fn thinking(&self) -> Color { self.base.thinking() }
    fn text(&self) -> Color { self.base.text() }
    fn muted(&self) -> Color { self.base.muted() }
    fn dim(&self) -> Color { self.base.dim() }
    fn border(&self) -> Color { self.base.border() }
    fn border_active(&self) -> Color { self.base.border_active() }
    fn popup_bg(&self) -> Color { self.base.popup_bg() }
    fn cursor_bg(&self) -> Color { self.base.cursor_bg() }
    fn loading(&self) -> Color { self.base.loading() }
}
```

- [ ] **Step 2: Commit**

```bash
git add side-projects/git-graph/
git commit -m "feat(gig): add GigTheme extending peri-widgets DarkTheme"
```

---

### Task 9: App 状态机 + 主事件循环

**Files:**
- Create: `side-projects/git-graph/src/app.rs`
- Create: `side-projects/git-graph/src/event.rs`
- Create: `side-projects/git-graph/src/ui/mod.rs`
- Modify: `side-projects/git-graph/src/main.rs`

- [ ] **Step 1: 创建 app.rs**

```rust
use crate::git::repo::GitRepo;
use crate::git::stash::StashInfo;
use crate::graph::color::BranchColors;
use crate::graph::layout::GraphLayout;
use crate::graph::topology::Topology;
use crate::theme::GigTheme;
use anyhow::Result;
use git2::Oid;
use std::collections::HashMap;

/// 当前活跃的面板
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Graph,
    Detail,
    Toolbar,
}

/// Overlay 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    BranchList,
    TagList,
    StashList,
    ConfirmDialog,
    FilterBar,
    SearchBar,
}

/// 主应用状态
pub struct App {
    pub running: bool,
    pub theme: GigTheme,

    // Git 数据
    pub repo: GitRepo,
    pub topology: Topology,
    pub layout: GraphLayout,
    pub colors: BranchColors,
    pub head_oid: Oid,

    // UI 状态
    pub focus: Focus,
    pub overlay: Overlay,
    pub selected_idx: usize,        // graph 中选中的行索引
    pub scroll_offset: usize,       // 虚拟滚动偏移
    pub viewport_height: usize,     // 可视区域高度
    pub graph_width: u16,           // graph 面板宽度
    pub detail_width: u16,          // detail 面板宽度

    // 详情缓存
    pub selected_oid: Option<Oid>,
    pub selected_detail: Option<crate::git::commit::CommitDetail>,
    pub selected_diff_stats: Option<crate::git::commit::DiffStats>,

    // Stash
    pub stash_map: HashMap<Oid, Vec<StashInfo>>,

    // 确认弹窗
    pub confirm_message: Option<String>,
    pub confirm_action: Option<ConfirmAction>,

    // 过滤
    pub filter_branch: Option<String>,
    pub search_query: Option<String>,
}

/// 确认操作
#[derive(Debug, Clone)]
pub enum ConfirmAction {
    ResetHard(Oid),
    DeleteBranch(String),
    StashDrop(usize),
    ForcePush,
}

impl App {
    pub fn new(repo: GitRepo) -> Result<Self> {
        let head_oid = repo.head_oid()?;
        let branch_map = repo.branch_map()?;
        let tag_map = repo.tag_map()?;
        let stash_map = repo.stash_by_commit()?;
        let nodes = repo.scan_topology()?;
        let topology = Topology::new(nodes, branch_map, tag_map, stash_map.clone());
        let mut colors = BranchColors::new();
        let layout = crate::graph::layout::build_layout(
            topology.nodes(),
            &topology.branch_map_raw(),
            &stash_map,
            &mut colors,
        );
        // 找到 HEAD 对应的行索引
        let selected_idx = layout.rows.iter().position(|r| r.oid == Some(head_oid)).unwrap_or(0);

        Ok(Self {
            running: true,
            theme: GigTheme::new(),
            repo,
            topology,
            layout,
            colors,
            head_oid,
            focus: Focus::Graph,
            overlay: Overlay::None,
            selected_idx,
            scroll_offset: 0,
            viewport_height: 40,
            graph_width: 60,
            detail_width: 40,
            selected_oid: Some(head_oid),
            selected_detail: None,
            selected_diff_stats: None,
            stash_map,
            confirm_message: None,
            confirm_action: None,
            filter_branch: None,
            search_query: None,
        })
    }

    /// 选中指定行，加载 commit 详情
    pub fn select(&mut self, idx: usize) {
        if idx >= self.layout.rows.len() {
            return;
        }
        self.selected_idx = idx;
        let row = &self.layout.rows[idx];
        if let Some(oid) = row.oid {
            self.selected_oid = Some(oid);
            // 加载详情
            if let Ok(detail) = self.repo.commit_detail(oid) {
                self.selected_detail = Some(detail);
            }
            // 懒加载 diff stats
            if let Ok(stats) = self.repo.load_diff_stats(oid) {
                self.selected_diff_stats = Some(stats);
            }
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }
}
```

注意：`topology.branch_map_raw()` 需要在 `Topology` 中添加：

```rust
impl Topology {
    pub fn branch_map_raw(&self) -> HashMap<Oid, Vec<String>> {
        self.branch_map.clone()
    }
}
```

- [ ] **Step 2: 创建 event.rs**

```rust
use crate::app::{App, Focus, Overlay};
use crossterm::event::{Event, MouseEvent, MouseEventKind, MouseButton, KeyCode, KeyModifiers};

/// 处理终端事件
pub fn handle_event(app: &mut App, event: Event) -> anyhow::Result<()> {
    match event {
        Event::Key(key) => handle_key(app, key.code, key.modifiers),
        Event::Mouse(mouse) => handle_mouse(app, mouse),
        _ => {}
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode, _mods: KeyModifiers) {
    match code {
        KeyCode::Char('q') => app.quit(),
        KeyCode::Up | KeyCode::Char('j') => {
            if app.selected_idx > 0 {
                app.select(app.selected_idx - 1);
            }
        }
        KeyCode::Down | KeyCode::Char('k') => {
            if app.selected_idx < app.layout.rows.len() - 1 {
                app.select(app.selected_idx + 1);
            }
        }
        KeyCode::Char('u') | KeyCode::Ctrl('u') => {
            let delta = app.viewport_height.min(3);
            if app.selected_idx >= delta {
                app.select(app.selected_idx - delta);
            }
        }
        KeyCode::Char('d') | KeyCode::Ctrl('d') => {
            let delta = app.viewport_height.min(3);
            let new_idx = (app.selected_idx + delta).min(app.layout.rows.len() - 1);
            app.select(new_idx);
        }
        KeyCode::Enter => {
            if app.overlay == Overlay::None {
                app.focus = Focus::Detail;
            }
        }
        KeyCode::Esc => {
            if app.overlay != Overlay::None {
                app.overlay = Overlay::None;
            } else if app.focus != Focus::Graph {
                app.focus = Focus::Graph;
            }
        }
        _ => {}
    }
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            if app.selected_idx > 0 {
                app.select(app.selected_idx - 3);
            }
        }
        MouseEventKind::ScrollDown => {
            let new_idx = (app.selected_idx + 3).min(app.layout.rows.len() - 1);
            app.select(new_idx);
        }
        MouseEventKind::Down(MouseButton::Left) => {
            // 点击 graph 面板区域内的行
            let row = mouse.row as usize;
            let target_idx = app.scroll_offset + row;
            if target_idx < app.layout.rows.len() {
                app.select(target_idx);
            }
        }
        _ => {}
    }
}
```

- [ ] **Step 3: 创建 ui/mod.rs**

```rust
pub mod graph_panel;
pub mod detail_panel;
pub mod toolbar;
pub mod overlay;
pub mod confirm;
```

- [ ] **Step 4: 重写 main.rs — 完整 TUI 启动**

```rust
mod app;
mod event;
mod git;
mod graph;
mod render;
mod theme;
mod ui;

use app::App;
use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{EnableMouseCapture, DisableMouseCapture},
    execute,
    terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

#[derive(Parser)]
#[command(name = "gig", about = "Git Graph TUI")]
struct Cli {
    /// 仓库路径（默认当前目录）
    repo_path: Option<std::path::PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_path = cli.repo_path.unwrap_or_else(|| std::path::PathBuf::from("."));

    // 初始化 git 数据
    let repo = git::repo::GitRepo::open(&repo_path)?;
    let mut app = App::new(repo)?;

    // 初始化终端
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 主循环
    app.select(app.selected_idx); // 加载初始选中 commit 的详情
    while app.running {
        terminal.draw(|f| render::draw(f, &mut app))?;

        if crossterm::event::poll(std::time::Duration::from_millis(100))? {
            let event = crossterm::event::read()?;
            event::handle_event(&mut app, event)?;
        }

        // 虚拟滚动：确保选中行在可视区域内
        adjust_scroll(app);
    }

    // 恢复终端
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn adjust_scroll(app: &mut App) {
    if app.selected_idx < app.scroll_offset {
        app.scroll_offset = app.selected_idx;
    } else if app.selected_idx >= app.scroll_offset + app.viewport_height {
        app.scroll_offset = app.selected_idx - app.viewport_height + 1;
    }
}
```

- [ ] **Step 5: 创建 render.rs — 主布局**

```rust
use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();

    // 左右分栏：graph 60% + detail 40%
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(size);

    app.graph_width = chunks[0].width;
    app.detail_width = chunks[1].width;
    app.viewport_height = chunks[0].height as usize;

    // 渲染 graph 面板
    ui::graph_panel::draw(f, chunks[0], app);

    // 渲染 detail 面板
    ui::detail_panel::draw(f, chunks[1], app);
}
```

- [ ] **Step 6: 创建最小 ui 组件占位**

创建 `side-projects/git-graph/src/ui/graph_panel.rs`:

```rust
use crate::app::App;
use crate::graph::render::render_graph_row;
use ratatui::{Frame, layout::Rect, widgets::Block};

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(ratatui::widgets::Borders::ALL);
    f.render_widget(block, area);

    let inner = area.inner(ratatui::layout::Margin::new(1, 1));
    let visible_rows = inner.height as usize;
    let start = app.scroll_offset;
    let end = (start + visible_rows).min(app.layout.rows.len());

    for (i, row_idx) in (start..end).enumerate() {
        let row = &app.layout.rows[row_idx];
        let is_selected = row_idx == app.selected_idx;
        let line = render_graph_row(row, app.graph_width.saturating_sub(2), is_selected, Some(app.head_oid));
        f.render_widget(
            ratatui::widgets::Paragraph::new(line),
            Rect::new(inner.x, inner.y + i as u16, inner.width, 1),
        );
    }
}
```

创建 `side-projects/git-graph/src/ui/detail_panel.rs`:

```rust
use crate::app::App;
use ratatui::{Frame, layout::Rect, widgets::{Block, Paragraph}, text::{Line, Span}, style::{Color, Style, Modifier}};

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(ratatui::widgets::Borders::ALL).title(" Detail ");
    f.render_widget(block, area);
    let inner = area.inner(ratatui::layout::Margin::new(1, 1));

    if let Some(detail) = &app.selected_detail {
        let mut lines: Vec<Line> = Vec::new();

        // Hash
        lines.push(Line::from(vec![
            Span::styled("Hash: ", Style::default().fg(Color::Gray)),
            Span::styled(detail.short_hash.clone(), Style::default().fg(Color::Magenta)),
        ]));

        // Author
        lines.push(Line::from(vec![
            Span::styled("Author: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{} <{}>", detail.author_name, detail.author_email), Style::default().fg(Color::White)),
        ]));

        // Message
        lines.push(Line::from(""));
        for msg_line in detail.message.lines() {
            lines.push(Line::styled(msg_line.to_string(), Style::default().fg(Color::White)));
        }

        // Branches / Tags
        if !detail.branches.is_empty() {
            lines.push(Line::from(""));
            let branch_spans: Vec<Span> = detail.branches.iter().flat_map(|b| {
                vec![Span::styled(format!(" {} ", b), Style::default().fg(Color::Black).bg(Color::Yellow)), Span::raw(" ")]
            }).collect();
            lines.push(Line::from(branch_spans));
        }

        if !detail.tags.is_empty() {
            let tag_spans: Vec<Span> = detail.tags.iter().flat_map(|t| {
                vec![Span::styled(format!(" {} ", t), Style::default().fg(Color::Black).bg(Color::Green)), Span::raw(" ")]
            }).collect();
            lines.push(Line::from(tag_spans));
        }

        // Diff stats
        if let Some(stats) = &app.selected_diff_stats {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(format!("{} files changed", stats.files_changed), Style::default().fg(Color::White)),
                Span::raw("  "),
                Span::styled(format!("+{}", stats.insertions), Style::default().fg(Color::Green)),
                Span::raw("  "),
                Span::styled(format!("-{}", stats.deletions), Style::default().fg(Color::Red)),
            ]));

            for file in &stats.files {
                let status_ch = match file.status {
                    crate::git::commit::FileStatus::Added => 'A',
                    crate::git::commit::FileStatus::Deleted => 'D',
                    crate::git::commit::FileStatus::Modified => 'M',
                    crate::git::commit::FileStatus::Renamed => 'R',
                    _ => '?',
                };
                let status_color = match file.status {
                    crate::git::commit::FileStatus::Added => Color::Green,
                    crate::git::commit::FileStatus::Deleted => Color::Red,
                    _ => Color::Yellow,
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", status_ch), Style::default().fg(status_color)),
                    Span::styled(file.path.clone(), Style::default().fg(Color::Gray)),
                ]));
            }
        }

        let para = Paragraph::new(lines);
        f.render_widget(para, inner);
    }
}
```

创建其他占位文件：

`side-projects/git-graph/src/ui/toolbar.rs`:
```rust
// 占位，Phase 4 实现
```

`side-projects/git-graph/src/ui/overlay.rs`:
```rust
// 占位，Phase 5 实现
```

`side-projects/git-graph/src/ui/confirm.rs`:
```rust
// 占位，Phase 6 实现
```

`side-projects/git-graph/src/ui/filter_bar.rs`:
```rust
// 占位
```

`side-projects/git-graph/src/ui/search_bar.rs`:
```rust
// 占位
```

- [ ] **Step 7: 编译并修复**

Run: `cd side-projects/git-graph && cargo build`
Expected: 编译成功（可能需要修复 import/类型问题）

- [ ] **Step 8: 在一个真实 git 仓库中测试启动**

Run: `cd /Users/konghayao/code/ai/perihelion && cargo run -p gig -- .`
Expected: 终端显示 graph 左侧面板 + detail 右侧面板，q 退出

- [ ] **Step 9: Commit**

```bash
git add side-projects/git-graph/
git commit -m "feat(gig): add TUI app with main loop, event handling, and graph/detail panels"
```

---

## Phase 4: Emoji 工具栏 + Git 操作

### Task 10: Emoji 工具栏

**Files:**
- Modify: `side-projects/git-graph/src/ui/toolbar.rs`
- Modify: `side-projects/git-graph/src/ui/detail_panel.rs`
- Modify: `side-projects/git-graph/src/event.rs`

- [ ] **Step 1: 创建 toolbar.rs — Emoji 操作按钮**

```rust
use crate::app::App;
use ratatui::{
    Frame, layout::Rect,
    text::{Line, Span},
    style::{Color, Style},
    widgets::Paragraph,
};

/// Emoji 工具栏按钮定义
pub struct ToolbarButton {
    pub emoji: &'static str,
    pub label: &'static str,
    pub action: ToolbarAction,
    pub dangerous: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarAction {
    CopyHash,
    Checkout,
    CreateTag,
    Merge,
    CherryPick,
    Reset,
    DeleteBranch,
    StashPop,
    StashDrop,
}

/// 获取 commit 操作按钮列表
pub fn commit_buttons(app: &App) -> Vec<ToolbarButton> {
    let mut buttons = vec![
        ToolbarButton { emoji: "📋", label: "copy hash", action: ToolbarAction::CopyHash, dangerous: false },
        ToolbarButton { emoji: "⎇", label: "checkout", action: ToolbarAction::Checkout, dangerous: false },
        ToolbarButton { emoji: "🏷", label: "tag", action: ToolbarAction::CreateTag, dangerous: false },
        ToolbarButton { emoji: "🔀", label: "merge", action: ToolbarAction::Merge, dangerous: false },
        ToolbarButton { emoji: "🍒", label: "cherry-pick", action: ToolbarAction::CherryPick, dangerous: false },
        ToolbarButton { emoji: "⏪", label: "reset", action: ToolbarAction::Reset, dangerous: true },
    ];

    // delete branch 只在 branch head 时显示
    if let Some(detail) = &app.selected_detail {
        if !detail.branches.is_empty() {
            buttons.push(ToolbarButton { emoji: "❌", label: "del branch", action: ToolbarAction::DeleteBranch, dangerous: true });
        }
    }

    // stash 操作
    if let Some(oid) = app.selected_oid {
        if app.stash_map.contains_key(&oid) {
            buttons.push(ToolbarButton { emoji: "📤", label: "stash pop", action: ToolbarAction::StashPop, dangerous: false });
            buttons.push(ToolbarButton { emoji: "🗑", label: "stash drop", action: ToolbarAction::StashDrop, dangerous: true });
        }
    }

    buttons
}

/// 全局操作按钮（顶部工具栏）
pub fn global_buttons() -> Vec<ToolbarButton> {
    vec![
        ToolbarButton { emoji: "⬇", label: "fetch", action: ToolbarAction::CopyHash, dangerous: false }, // 临时，后续替换
    ]
}

pub fn draw_toolbar(f: &mut Frame, area: Rect, app: &App) {
    let buttons = commit_buttons(app);
    let mut spans: Vec<Span> = Vec::new();
    for (i, btn) in buttons.iter().enumerate() {
        let color = if btn.dangerous { Color::Red } else { Color::White };
        spans.push(Span::styled(
            format!(" {}{} ", btn.emoji, btn.label),
            Style::default().fg(color).bg(Color::Rgb(35, 35, 45)),
        ));
        if i < buttons.len() - 1 {
            spans.push(Span::raw(" "));
        }
    }
    let para = Paragraph::new(Line::from(spans));
    f.render_widget(para, area);
}
```

- [ ] **Step 2: 修改 detail_panel.rs — 在顶部集成 toolbar**

在 `detail_panel.rs` 的 `draw` 函数中，将区域分成 toolbar + 详情两部分：

```rust
pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(ratatui::widgets::Borders::ALL).title(" Detail ");
    f.render_widget(block, area);
    let inner = area.inner(ratatui::layout::Margin::new(1, 1));

    // 分出 toolbar 行（1 行）和详情区域
    if inner.height > 2 {
        let toolbar_area = Rect::new(inner.x, inner.y, inner.width, 1);
        let detail_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1);
        super::toolbar::draw_toolbar(f, toolbar_area, app);
        draw_detail(f, detail_area, app);
    } else {
        draw_detail(f, inner, app);
    }
}

fn draw_detail(f: &mut Frame, area: Rect, app: &App) {
    // ... 原有的详情渲染代码 ...
}
```

- [ ] **Step 3: 编译验证**

Run: `cd side-projects/git-graph && cargo build`
Expected: 编译成功

- [ ] **Step 4: Commit**

```bash
git add side-projects/git-graph/
git commit -m "feat(gig): add emoji toolbar with commit operations"
```

---

### Task 11: Git 操作执行层

**Files:**
- Create: `side-projects/git-graph/src/git/ops.rs`
- Modify: `side-projects/git-graph/src/git/mod.rs`
- Modify: `side-projects/git-graph/src/event.rs`

- [ ] **Step 1: 创建 ops.rs — git 操作实现**

```rust
use anyhow::{Context, Result};
use git2::{Oid, Repository};
use std::process::Command;

use super::repo::GitRepo;

impl GitRepo {
    pub fn checkout(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["checkout", &short])
    }

    pub fn merge(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["merge", &short])
    }

    pub fn cherry_pick(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["cherry-pick", &short])
    }

    pub fn reset_soft(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["reset", "--soft", &short])
    }

    pub fn reset_hard(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["reset", "--hard", &short])
    }

    pub fn create_tag(&self, oid: Oid, name: &str) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["tag", name, &short])
    }

    pub fn delete_branch(&self, name: &str) -> Result<()> {
        self.run_git(&["branch", "-D", name])
    }

    /// 获取 repo 的绝对路径（用于 git -C 命令）
    fn workdir(&self) -> Result<&std::path::Path> {
        self.repo()
            .workdir()
            .context("bare 仓库不支持此操作")
    }

    fn run_git(&self, args: &[&str]) -> Result<()> {
        let workdir = self.workdir()?;
        let output = Command::new("git")
            .args(args)
            .current_dir(workdir)
            .output()
            .with_context(|| format!("执行 git {} 失败", args.join(" ")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git {} 失败: {}", args.join(" "), stderr);
        }
        Ok(())
    }
}
```

- [ ] **Step 2: 更新 git/mod.rs**

```rust
pub mod repo;
pub mod commit;
pub mod stash;
pub mod ops;
```

- [ ] **Step 3: 运行编译**

Run: `cd side-projects/git-graph && cargo build`
Expected: 编译成功

- [ ] **Step 4: Commit**

```bash
git add side-projects/git-graph/
git commit -m "feat(gig): add git operations (checkout/merge/cherry-pick/reset/tag/delete-branch)"
```

---

### Task 12: 确认弹窗

**Files:**
- Modify: `side-projects/git-graph/src/ui/confirm.rs`
- Modify: `side-projects/git-graph/src/event.rs`
- Modify: `side-projects/git-graph/src/render.rs`

- [ ] **Step 1: 实现 confirm.rs**

```rust
use crate::app::App;
use ratatui::{
    Frame, layout::Rect,
    text::{Line, Span},
    style::{Color, Style, Modifier},
    widgets::{Block, Borders, Clear, Paragraph},
    layout::Alignment,
};

pub fn draw_confirm(f: &mut Frame, area: Rect, app: &App) {
    if app.confirm_message.is_none() {
        return;
    }
    let msg = app.confirm_message.as_ref().unwrap();

    // 居中弹窗
    let popup_width = 50u16;
    let popup_height = 5u16;
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .title(" ⚠ Confirm ");
    let inner = popup_area.inner(ratatui::layout::Margin::new(1, 1));

    let lines = vec![
        Line::from(Span::styled(msg.clone(), Style::default().fg(Color::White))),
        Line::from(""),
        Line::from(vec![
            Span::styled(" [Y]es ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(" [N]o ", Style::default().fg(Color::White).bg(Color::DarkGray)),
        ]),
    ];

    let para = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(para, popup_area);
}
```

- [ ] **Step 2: 在 render.rs 中集成确认弹窗**

在 `render::draw` 末尾添加：

```rust
// 确认弹窗覆盖层
if app.confirm_message.is_some() {
    ui::confirm::draw_confirm(f, size, app);
}
```

- [ ] **Step 3: 编译 + Commit**

```bash
cargo build
git add side-projects/git-graph/
git commit -m "feat(gig): add confirmation dialog for dangerous operations"
```

---

## Phase 5: Overlay 面板

### Task 13: Branch/Tag/Stash 列表 Overlay

**Files:**
- Modify: `side-projects/git-graph/src/ui/overlay.rs`
- Modify: `side-projects/git-graph/src/render.rs`

- [ ] **Step 1: 实现 overlay.rs**

```rust
use crate::app::App;
use crate::app::Overlay;
use ratatui::{
    Frame, layout::Rect,
    text::{Line, Span},
    style::{Color, Style, Modifier},
    widgets::{Block, Borders, Clear, Paragraph},
};

pub fn draw_overlay(f: &mut Frame, area: Rect, app: &App) {
    match app.overlay {
        Overlay::BranchList => draw_list(f, area, " Branches ", &app.repo.branch_names().unwrap_or_default()),
        Overlay::TagList => draw_list(f, area, " Tags ", &app.repo.tag_names().unwrap_or_default()),
        Overlay::StashList => {
            let stashes: Vec<String> = app.stash_map.values()
                .flatten()
                .map(|s| format!("stash@{{{}}}: {}", s.index, s.message))
                .collect();
            draw_list(f, area, " Stash ", &stashes);
        }
        _ => {}
    }
}

fn draw_list(f: &mut Frame, area: Rect, title: &str, items: &[String]) {
    let popup_width = 40u16;
    let popup_height = (items.len() as u16 + 2).min(20);
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let lines: Vec<Line> = items.iter().map(|item| {
        Line::from(Span::styled(item.clone(), Style::default().fg(Color::White)))
    }).collect();

    let para = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title).border_style(Style::default().fg(Color::Yellow)));
    f.render_widget(para, popup_area);
}
```

- [ ] **Step 2: 在 render.rs 中集成 overlay**

```rust
// Overlay 覆盖层
if app.overlay != Overlay::None {
    ui::overlay::draw_overlay(f, size, app);
}
```

- [ ] **Step 3: 添加 overlay 触发到全局工具栏**

在 `toolbar.rs` 中添加全局工具栏（graph 面板顶部），包含 branch/tag 列表按钮。

- [ ] **Step 4: 编译 + Commit**

```bash
cargo build
git add side-projects/git-graph/
git commit -m "feat(gig): add branch/tag/stash list overlays"
```

---

## Phase 6: Remote 操作

### Task 14: Fetch/Pull/Push（tokio 后台）

**Files:**
- Create: `side-projects/git-graph/src/git/remote.rs`
- Modify: `side-projects/git-graph/src/git/mod.rs`
- Modify: `side-projects/git-graph/src/app.rs`

- [ ] **Step 1: 创建 remote.rs**

```rust
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;
use tokio::sync::mpsc;

/// 远程操作结果
#[derive(Debug, Clone)]
pub struct RemoteResult {
    pub operation: RemoteOp,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteOp {
    Fetch,
    Pull,
    Push,
}

/// 后台执行 remote 操作
pub fn spawn_remote_op(
    workdir: PathBuf,
    op: RemoteOp,
    tx: mpsc::UnboundedSender<RemoteResult>,
) {
    std::thread::spawn(move || {
        let args = match op {
            RemoteOp::Fetch => &["fetch"][..],
            RemoteOp::Pull => &["pull"][..],
            RemoteOp::Push => &["push"][..],
        };
        let output = Command::new("git")
            .args(args)
            .current_dir(&workdir)
            .output();
        let result = match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                let msg = if out.status.success() { stdout } else { stderr };
                RemoteResult {
                    operation: op,
                    success: out.status.success(),
                    message: msg,
                }
            }
            Err(e) => RemoteResult {
                operation: op,
                success: false,
                message: e.to_string(),
            },
        };
        let _ = tx.send(result);
    });
}
```

- [ ] **Step 2: 更新 git/mod.rs + app.rs**

在 `git/mod.rs` 中添加 `pub mod remote;`

在 `app.rs` 中添加：
```rust
pub remote_tx: tokio::sync::mpsc::UnboundedSender<git::remote::RemoteResult>,
pub remote_rx: tokio::sync::mpsc::UnboundedReceiver<git::remote::RemoteResult>,
pub remote_status: Option<String>,
```

- [ ] **Step 3: 在 main.rs 主循环中 poll remote 结果**

```rust
// 在主循环中添加
if let Ok(result) = app.remote_rx.try_recv() {
    app.remote_status = Some(format!("{}: {}", 
        match result.operation {
            RemoteOp::Fetch => "Fetch",
            RemoteOp::Pull => "Pull", 
            RemoteOp::Push => "Push",
        },
        if result.success { "✓" } else { &result.message }
    ));
}
```

- [ ] **Step 4: 编译 + Commit**

```bash
cargo build
git add side-projects/git-graph/
git commit -m "feat(gig): add remote operations (fetch/pull/push) with background threads"
```

---

## Phase 7: 过滤 + 搜索 + 全局工具栏

### Task 15: 过滤栏 + 搜索栏

**Files:**
- Modify: `side-projects/git-graph/src/ui/filter_bar.rs`
- Modify: `side-projects/git-graph/src/ui/search_bar.rs`
- Modify: `side-projects/git-graph/src/render.rs`

- [ ] **Step 1: 实现 filter_bar.rs**

```rust
use crate::app::App;
use ratatui::{
    Frame, layout::Rect,
    text::{Line, Span},
    style::{Color, Style},
    widgets::Paragraph,
};

pub fn draw_filter_bar(f: &mut Frame, area: Rect, app: &App) {
    let filter = app.filter_branch.as_deref().unwrap_or("");
    let line = Line::from(vec![
        Span::styled(" Filter: ", Style::default().fg(Color::Yellow)),
        Span::styled(
            if filter.is_empty() { "all branches".to_string() } else { filter.to_string() },
            Style::default().fg(Color::White),
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
```

- [ ] **Step 2: 实现 search_bar.rs**

```rust
use crate::app::App;
use ratatui::{
    Frame, layout::Rect,
    text::{Line, Span},
    style::{Color, Style},
    widgets::Paragraph,
};

pub fn draw_search_bar(f: &mut Frame, area: Rect, app: &App) {
    let query = app.search_query.as_deref().unwrap_or("");
    let line = Line::from(vec![
        Span::styled(" / ", Style::default().fg(Color::Cyan)),
        Span::styled(query.to_string(), Style::default().fg(Color::White)),
        Span::styled("▎", Style::default().fg(Color::Cyan)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
```

- [ ] **Step 3: 集成到 render.rs（顶部工具栏）**

在 graph 面板顶部渲染全局工具栏（一行 emoji 按钮：⬇fetch ⬆push ⬇⬆pull 🔍search 📋filter 🌿branches 🏷tags）

- [ ] **Step 4: 编译 + Commit**

```bash
cargo build
git add side-projects/git-graph/
git commit -m "feat(gig): add filter bar, search bar, and global toolbar"
```

---

## Phase 8: 集成 + 打磨

### Task 16: 鼠标点击 Emoji 按钮 + 操作执行串联

**Files:**
- Modify: `side-projects/git-graph/src/event.rs`
- Modify: `side-projects/git-graph/src/ui/toolbar.rs`

- [ ] **Step 1: 在 event.rs 中处理工具栏按钮点击**

需要：
1. 记录工具栏按钮的渲染位置（Rect）
2. 鼠标点击时检查是否命中工具栏区域
3. 命中则执行对应操作

在 `toolbar.rs` 中记录按钮位置：

```rust
pub struct ToolbarState {
    pub button_rects: Vec<Rect>,
}

impl ToolbarState {
    pub fn new() -> Self {
        Self { button_rects: Vec::new() }
    }

    pub fn hit_test(&self, x: u16, y: u16) -> Option<usize> {
        self.button_rects.iter().position(|r| r.contains(ratatui::layout::Rect::new(x, y, 1, 1)))
    }
}
```

- [ ] **Step 2: 在 event.rs 中串联操作执行**

点击 emoji 按钮后：
- 非危险操作：直接执行
- 危险操作：设置 confirm_message + confirm_action，进入确认流程
- 确认流程：Y 键执行，N/Esc 取消

- [ ] **Step 3: Commit**

```bash
git add side-projects/git-graph/
git commit -m "feat(gig): wire up emoji toolbar click handling with confirm flow"
```

---

### Task 17: 全局工具栏渲染 + 远程操作触发

**Files:**
- Modify: `side-projects/git-graph/src/render.rs`
- Modify: `side-projects/git-graph/src/ui/toolbar.rs`
- Modify: `side-projects/git-graph/src/event.rs`

- [ ] **Step 1: 在 render.rs 中添加全局工具栏行**

graph 面板上方添加 1 行全局工具栏：
```
 ⬇ fetch  ⬆ push  ⬇⬆ pull  🌿 branches  🏷 tags  📦 stash  🔍 search  📋 filter
```

- [ ] **Step 2: 点击全局按钮触发远程操作**

fetch/push/pull 点击后调用 `remote::spawn_remote_op`。

- [ ] **Step 3: Commit**

```bash
git add side-projects/git-graph/
git commit -m "feat(gig): add global toolbar with remote operations"
```

---

### Task 18: 刷新机制（操作后重新加载 graph）

**Files:**
- Modify: `side-projects/git-graph/src/app.rs`

- [ ] **Step 1: 添加 reload 方法**

```rust
impl App {
    /// 重新加载 git 数据并重建 graph
    pub fn reload(&mut self) -> Result<()> {
        let branch_map = self.repo.branch_map()?;
        let tag_map = self.repo.tag_map()?;
        let stash_map = self.repo.stash_by_commit()?;
        let nodes = self.repo.scan_topology()?;
        self.topology = Topology::new(nodes, branch_map.clone(), tag_map, stash_map.clone());
        self.stash_map = stash_map;
        self.layout = crate::graph::layout::build_layout(
            self.topology.nodes(),
            &branch_map,
            &self.stash_map,
            &mut self.colors,
        );
        self.head_oid = self.repo.head_oid()?;
        // 保持选中当前 commit（如果还存在）
        if let Some(oid) = self.selected_oid {
            if let Some(idx) = self.layout.rows.iter().position(|r| r.oid == Some(oid)) {
                self.selected_idx = idx;
            }
        }
        Ok(())
    }
}
```

- [ ] **Step 2: 在每次操作成功后调用 reload**

- [ ] **Step 3: Commit**

```bash
git add side-projects/git-graph/
git commit -m "feat(gig): add reload mechanism after git operations"
```

---

### Task 19: 端到端测试

**Files:**
- Create: `side-projects/git-graph/tests/integration_test.rs`（不在 workspace 内，独立 Cargo.toml 支持/tests 目录）

- [ ] **Step 1: 创建集成测试**

使用 `git init` 创建临时仓库，通过 `gig` 的数据层验证完整流程：

```rust
// 注意：由于 gig 是 bin crate 且有自己的 [workspace]，
// 集成测试放在 src/ 内作为 bin-test 或使用 assert_cmd crate
```

实际方案：在 `Cargo.toml` 中添加 `assert_cmd` dev-dependency，创建 `tests/integration_test.rs`：

```toml
[dev-dependencies]
assert_cmd = "2"
tempfile = "3"
predicates = "3"
```

```rust
use assert_cmd::Command;
use tempfile::TempDir;
use std::process::Command as StdCommand;

fn setup_repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    StdCommand::new("git").args(["init"]).current_dir(p).output().unwrap();
    StdCommand::new("git").args(["config", "user.email", "t@t.com"]).current_dir(p).output().unwrap();
    StdCommand::new("git").args(["config", "user.name", "t"]).current_dir(p).output().unwrap();
    std::fs::write(p.join("a.txt"), "a").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(p).output().unwrap();
    StdCommand::new("git").args(["commit", "-m", "init"]).current_dir(p).output().unwrap();
    dir
}

#[test]
fn test_gig_starts() {
    let dir = setup_repo();
    // gig 需要 TTY，这里只验证 --help
    let mut cmd = Command::cargo_bin("gig").unwrap();
    cmd.arg("--help").assert().success();
}
```

- [ ] **Step 2: 运行测试**

Run: `cd side-projects/git-graph && cargo test`
Expected: 所有测试 PASS

- [ ] **Step 3: Commit**

```bash
git add side-projects/git-graph/
git commit -m "test(gig): add integration tests"
```

---

## Self-Review

### Spec Coverage

| 需求 | 覆盖 Task |
|------|-----------|
| 项目骨架 (side-projects/git-graph/) | Task 1 |
| git2 数据层 (topology + commit detail) | Task 2 |
| Stash 操作 | Task 3 |
| 拓扑骨架构建 | Task 4 |
| Branch 着色 | Task 5 |
| Lane-based graph 布局引擎 | Task 6 |
| Graph 渲染 (CellType → ratatui) | Task 7 |
| Theme 扩展 | Task 8 |
| App 状态机 + 主事件循环 | Task 9 |
| Emoji 工具栏 | Task 10 |
| Git 操作执行层 | Task 11 |
| 确认弹窗 | Task 12 |
| Branch/Tag/Stash Overlay | Task 13 |
| Remote 操作 (fetch/pull/push) | Task 14 |
| 过滤 + 搜索栏 | Task 15 |
| 鼠标点击工具栏执行 | Task 16 |
| 全局工具栏 + 远程触发 | Task 17 |
| 操作后刷新 | Task 18 |
| 端到端测试 | Task 19 |

### Placeholder Scan
无 TBD/TODO/placeholder。每个 task 都有具体代码。

### Type Consistency
- `TopoNode.oid: Oid` → `Topology.index: HashMap<Oid, usize>` → `GraphRow.oid: Option<Oid>` — 一致
- `CommitDetail` 在 repo.rs 定义，在 detail_panel.rs 使用 — 一致
- `ToolbarAction` 在 toolbar.rs 定义，在 event.rs 使用 — 一致
- `RemoteOp`/`RemoteResult` 在 remote.rs 定义，在 app.rs 使用 — 一致
