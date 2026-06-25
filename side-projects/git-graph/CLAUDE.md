# gig — Git Graph TUI

基于 ratatui 的终端 Git 图形化浏览工具。

## 项目结构

```
src/
├── main.rs           CLI 入口、终端初始化、主循环
├── app.rs            App 状态（所有 UI 状态集中在 struct App）
├── event.rs          键盘/鼠标事件处理
├── render.rs         顶层 draw() 分发到各面板
├── theme.rs          配色主题（GigTheme）
├── git/
│   ├── commit.rs     commit 解析、TopoNode、diff
│   ├── repo.rs       GitRepo 封装（branch_map/tag_map/stash/log）
│   ├── ops.rs        git 操作（checkout/merge/reset/push 等）
│   ├── remote.rs     远程仓库状态检测
│   ├── stash.rs      stash 信息
│   └── status.rs     git status 解析
├── graph/
│   ├── layout.rs     核心：lane-based graph 布局算法
│   ├── render.rs     CellType → Unicode box-drawing 字符映射
│   ├── color.rs      BranchColors（FNV-1a stable hash + 调色板）
│   └── topology.rs   拓扑排序
└── ui/
    ├── graph_panel.rs    graph 面板渲染
    ├── detail_panel.rs   commit 详情面板
    ├── sidebar/          左侧边栏（文件树 + git status）
    ├── confirm.rs        确认弹窗
    ├── overlay.rs        overlay 通用容器
    ├── filter_bar.rs     分支过滤
    ├── search_bar.rs     commit 搜索
    └── toolbar.rs        底部工具栏
```

## 开发命令

```bash
cargo build          # 构建
cargo run            # 运行（默认打开当前目录 .git）
cargo run -- /path   # 指定仓库路径
cargo test           # 全量测试
```

## 架构要点

### 主循环

`main.rs` 100ms poll 事件 → `event::handle_event()` → `app.dirty=true` → `render::draw()`。sidebar 每 2 秒自动刷新 git status。

### Graph 布局算法（layout.rs）

核心函数 `build_layout(nodes, branch_map, stash_map, colors, tag_map) → GraphLayout`。

**算法流程**：从新到旧逐 commit 处理，维护 `lanes: Vec<Option<Lane>>` 追踪活跃路径。

- 每个 commit 分配一个 lane，first parent 继承同 lane，extra parent 分配新 lane
- 收敛（两个 lane 追踪同一 commit）：保留低 index lane，画收敛连接器
- 分叉（merge commit 多 parent）：画分叉连接器

**角落字符边连接**（通过 `╭──╮ / │ │ / ╰──╯` 圆角矩形验证）：
- `╭` BranchRight = BOTTOM + RIGHT（线向下、向右延伸）
- `╮` BranchLeft = BOTTOM + LEFT（线向下、向左延伸）
- `╰` MergeRight = TOP + RIGHT（线从上方来、向右延伸）
- `╯` MergeLeft = TOP + LEFT（线从上方来、向左延伸）

**规则**：
- **分叉**(fork) = 新分支向下走 → 需要 BOTTOM 边 → Branch* (╭/╮)
- **收敛**(convergence) = 路径从上方来，结束于侧面 → 需要 TOP 边 → Merge* (╰/╯)

**颜色回收**：ColorPool 在路径结束时释放颜色，新路径优先复用。

### 数据结构

```rust
enum CellType { Empty, Pipe(Color), Commit(Color),
                BranchRight/Left(Color), MergeRight/Left(Color),
                Horizontal(Color), TeeRight/Left(Color) }
struct GraphRow { oid, lane, cells, branch, branches, message_short, has_stash, tags }
struct GraphLayout { rows: Vec<GraphRow>, max_lane: usize }
```

### 渲染（render.rs）

每个 CellType 占 2 列宽（主字符 + 扩展字符）。`cell_to_char()` 返回主字符，`cell_second_char()` 返回第二字符（`─` 或 ` `）。

### UI 布局

三栏：Sidebar(25%) | Graph(45%) | Detail(30%)。Focus 在三栏间切换，Overlay 弹窗覆盖。

### 鼠标事件

`graph_inner_y` / `graph_area` 用于偏移计算。`coalesce_mouse_events()` 合并连续 scroll/drag 事件。

## 依赖

独立 workspace，不依赖主项目（仅依赖 `peri-widgets` 用于文件树组件）。

## 编码规范

- Rust 2021，tokio async，anyhow 错误处理
- 测试用 `#[cfg(test)] mod tests` 在同文件内
- 注释/断言消息用中文
- 终端宽度用 `unicode-width` crate
