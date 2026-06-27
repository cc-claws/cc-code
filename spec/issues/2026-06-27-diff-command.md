# `/diff` 命令 — 查看未提交变更与每轮 Diff

## Status
- [ ] Phase 1: 实现 DiffCommand（TUI Command，打开 DiffPanel）
- [ ] Phase 2: 实现 GitWorkingTreeDiff 数据源
- [ ] Phase 3: 实现 DiffPanel UI（文件列表 + 详情视图）
- [ ] Phase 4: 实现 TurnDiff 数据源（从 AgentEvent 提取每轮变更）
- [ ] Phase 5: 双源切换（Current ↔ Turn N）
- [ ] Phase 6: 注册 + 快捷键
- [ ] Phase 7: 单元测试

## Created
2026-06-27

## Severity
Feature — 核心代码审查能力

## Platform
全平台 (Windows / macOS / Linux)

## Problem

peri 缺少 `/diff` 命令。用户无法在 TUI 中查看当前工作区的未提交变更，也无法回顾 agent 每轮对话修改了哪些文件。需要退出 TUI 切到终端执行 `git diff`，或依赖 agent 主动展示——两种方式都不高效。

Claude Code（TS 版）已实现 `/diff` 命令（`C:\Work\open-cladue\src\commands\diff\`，~700 行组件）。核心设计：**local-jsx 类型，渲染一个双源 diff 面板——"Current"（实时 `git diff HEAD`）和"Turn N"（从对话历史中提取 Edit/Write 工具的 structuredPatch）**。用户通过 `←/→` 切换数据源，`↑/↓` 选择文件，`Enter` 查看 word-level diff 详情。

## 参考实现 — Claude Code `/diff`

**源码目录**：`C:\Work\open-cladue\src\commands\diff\` + `src\components\diff\`

### 数据源

| 源 | 数据获取方式 | 内容 |
|----|-------------|------|
| **Current** | `fetchGitDiff()` + `fetchGitDiffHunks()` — 子进程执行 `git diff HEAD` | 工作区所有未提交变更（staged + unstaged + untracked） |
| **Turn N** | `useTurnDiffs(messages)` — 内存解析 `toolUseResult` 中的 `structuredPatch` | 第 N 轮对话中 FileEditTool/FileWriteTool 修改的文件 |

### UI 布局

```
┌─ DiffDialog ────────────────────────────────────┐
│  Title: "Uncommitted changes (git diff HEAD)"   │
│        or "Turn 3 \"fix the bug\""               │
│                                                   │
│  ◀ Current · T3 · T2 ▶   (source selector pills) │
│  3 files changed +12 -5  (subtitle stats)        │
│                                                   │
│  ┌─ File List ─────────────────────────────┐    │
│  │  ► src/foo.rs                    +5 -2  │    │
│  │    src/bar.rs                    +7 -3  │    │
│  │    src/baz.rs                    (binary)│    │
│  └──────────────────────────────────────────┘    │
│                                                   │
│  ── OR in detail mode ──                          │
│                                                   │
│  ┌─ Detail View ───────────────────────────┐    │
│  │  @@ -10,5 +10,7 @@                      │    │
│  │  - old line                              │    │
│  │  + new line 1                            │    │
│  │  + new line 2   (word-level diff)        │    │
│  └──────────────────────────────────────────┘    │
│                                                   │
│  ←/→ source  ↑/↓ select  Enter view  Esc close   │
└───────────────────────────────────────────────────┘
```

### 快捷键

| 键 | List 模式 | Detail 模式 |
|----|-----------|-------------|
| `←/→` | 切换数据源（Current / Turn N） | — |
| `↑/↓` | 选择文件 | — |
| `Enter` | 进入文件 diff 详情 | — |
| `Backspace` | — | 返回列表 |
| `Esc` | 关闭面板 | 返回列表 |

## 现有基础设施（可直接复用）

### `peri-widgets::diff` — 生产级 Diff 渲染库

| 组件 | 说明 |
|------|------|
| `DiffInput` | 输入结构：old_text + new_text + is_new_file |
| `compute_diff()` | 计算 DiffResult，LRU 缓存 64 条 |
| `render_diff()` | 渲染 `Vec<Line>`，word-level 高亮，CJK 安全，LRU 缓存 64 条 |
| `Theme` trait | 8 个 diff 颜色槽位已就绪 |

### TUI 内联 Diff（已集成）

`peri-tui/src/ui/message_view/mod.rs:24` 的 `build_diff_input()` 已能从 Edit/Write 工具的 `ToolStart` 事件中提取 `old_string`/`new_string`/`content` 构造 `DiffInput`。可直接复用此模式提取 TurnDiff。

### 文本 Diff 高亮（备选）

`peri-widgets/src/message_block/highlight.rs` — 对纯文本 unified diff 做轻量着色（`+`/`-`/`@@` 前缀行）。

## Fix Proposal

### 映射到 peri 架构

Claude Code 的 `type: 'local-jsx'` = peri 的 **TUI Command**（`Command` trait，同步执行在 App 上，打开面板）。

参考 `/model`、`/history` 等面板命令的模式：command 的 `execute()` 调用 `app.open_panel(PanelKind::Diff)`。

### Phase 1: DiffCommand（TUI Command）

**新建** `peri-tui/src/command/panel/diff.rs`：

```rust
//! `/diff` 命令 — 打开 Diff 面板查看未提交变更和每轮 diff。

use crate::app::App;
use crate::command::{Command, CommandResult};

pub struct DiffCommand;

impl Command for DiffCommand {
    fn name(&self) -> &str { "diff" }
    fn aliases(&self) -> Vec<&str> { vec!["changes"] }
    fn description(&self) -> &str { "View uncommitted changes and per-turn diffs" }

    fn execute(&self, app: &mut App, _args: &str) -> CommandResult {
        app.open_panel(crate::app::panel_manager::PanelKind::Diff);
        CommandResult::Ok
    }
}
```

注册到 TUI `default_registry()`（`peri-tui/src/command/mod.rs`）。

### Phase 2: GitWorkingTreeDiff 数据源

**新建** `peri-tui/src/diff/git_diff_source.rs`：

执行 `git diff HEAD` + `git diff --cached` + `git ls-files --others --exclude-standard` 获取完整工作区变更。

```rust
pub struct GitDiffSource;

pub struct GitDiffData {
    pub files: Vec<DiffFileEntry>,
    pub stats: DiffStats,
}

pub struct DiffFileEntry {
    pub path: String,
    pub status: FileStatus,        // Modified / Added / Deleted / Untracked / Binary
    pub additions: usize,
    pub deletions: usize,
    pub old_text: Option<String>,   // Deleted/Modified 的原始内容
    pub new_text: Option<String>,   // Added/Modified 的新内容
    pub is_binary: bool,
    pub is_truncated: bool,         // 超过 MAX_FILE_BYTES
}

pub struct DiffStats {
    pub files_count: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
}

const MAX_FILE_BYTES: usize = 1_000_000;   // 1MB，超过标记为 truncated
const MAX_DIFF_LINES: usize = 400;          // 超过行数截断
```

**数据获取**：

```rust
impl GitDiffSource {
    /// 获取完整工作区变更。在 tokio::spawn_blocking 中执行（git 是阻塞 IO）。
    pub async fn fetch(cwd: &str) -> Result<GitDiffData> {
        // 1. git diff HEAD --numstat → 获取文件列表 + 行数统计
        // 2. git diff HEAD -- <path> → 逐文件获取 diff 内容
        // 3. git ls-files --others --exclude-standard → untracked 文件
        // 4. 对每个文件：读取 old (git show HEAD:<path>) 和 new (磁盘读取)
        // 5. 构造 DiffFileEntry
    }
}
```

**跨平台注意**：
- Windows 下 `StdCommand::new("git")` 直接调用（不需要 `cmd /C`）
- 路径用 `/` 分隔（git 输出统一用 `/`）
- 二进制文件检测：`git diff --numstat` 中 `- -` 表示二进制

**复用点**：`run_git()` 函数与 `/commit` 命令共用，可提取到 `peri-acp` 或 `peri-agent` 的公共模块。

### Phase 3: DiffPanel UI

**新建** `peri-tui/src/ui/panels/diff_panel.rs`：

```rust
pub struct DiffPanel {
    state: DiffPanelState,
    selected_tab: usize,          // 0 = Current, 1..N = Turn 1..N
    selected_file: usize,
    view_mode: ViewMode,
    scroll_offset: usize,
    // 缓存
    git_diff_cache: Option<GitDiffData>,
    turn_diffs: Vec<TurnDiff>,
}

enum DiffPanelState {
    Loading,          // git diff 正在执行
    Ready,
    Error(String),    // git 不可用 / 非仓库
}

enum ViewMode {
    FileList,
    Detail { file_index: usize },
}
```

**渲染**（实现 `PanelComponent` trait）：

**Tab 栏**：
```
◀ Current · T3 · T2 ▶
N files changed +X -Y
```

**FileList 视图**：
```
  ► src/foo.rs                    +5 -2
    src/bar.rs                    +7 -3
    src/baz.rs                    (binary)
    new_file.rs                   (new)
```

- 选中项高亮（`theme::SURFACE_2` 背景）
- 路径过长时从左侧截断（保留 `...` 前缀）
- 特殊状态标签：`(binary)`、`(truncated)`、`(new)`、`(deleted)`

**Detail 视图**：
- 直接调用 `peri_widgets::diff::render_diff()` 渲染 word-level diff
- 标题行显示文件路径 + 状态
- 滚动支持 `Ctrl+U`/`Ctrl+D`

**快捷键**（`PanelComponent::handle_key()`）：

| 键 | FileList | Detail |
|----|----------|--------|
| `↑/↓` | 移动选择 | — |
| `Enter` | 进入详情 | — |
| `←/→` | 切换 tab（Current ↔ Turn N） | — |
| `Backspace` | — | 返回列表 |
| `Esc` | 关闭面板 | 返回列表 |

### Phase 4: TurnDiff 数据源

**新建** `peri-tui/src/diff/turn_diff_source.rs`：

从 agent 事件流中提取每轮 diff。复用 `build_diff_input()`（`message_view/mod.rs:24`）的解析模式。

```rust
pub struct TurnDiff {
    pub turn_index: usize,
    pub label: String,               // 用户消息前 40 字符
    pub files: Vec<DiffFileEntry>,
    pub stats: DiffStats,
}

pub struct TurnDiffSource {
    cached_turns: Vec<TurnDiff>,
    last_processed_index: usize,      // 增量处理
}
```

**提取逻辑**：

```rust
impl TurnDiffSource {
    /// 从 AgentEvent 历史中提取 TurnDiff。
    /// 增量处理：只扫描 last_processed_index 之后的事件。
    pub fn extract(events: &[AgentEvent]) -> Vec<TurnDiff> {
        // 1. 按 Human 消息分组为 turns
        // 2. 每个 turn 内扫描 ToolStart 事件：
        //    - Edit 工具：提取 file_path + old_string + new_string
        //    - Write 工具：提取 file_path + content
        // 3. 构造 DiffInput → compute_diff() → DiffResult
        // 4. 聚合为 TurnDiff
    }
}
```

**注意**：TurnDiff 仅包含 agent 通过 Edit/Write 工具修改的文件，不包含 agent 通过 Bash 工具执行的 `rm`、`mv` 等操作。这是 Claude Code 的行为一致——`useTurnDiffs` 也只解析 `FileEditTool`/`FileWriteTool` 的 `structuredPatch`。

### Phase 5: 双源切换

Tab 栏数据源列表构建：

```rust
fn build_tabs(&self) -> Vec<TabInfo> {
    let mut tabs = vec![TabInfo { label: "Current".into(), source: Source::Git }];
    for (i, turn) in self.turn_diffs.iter().enumerate().rev() {
        tabs.push(TabInfo {
            label: format!("T{}", i + 1),
            source: Source::Turn(i),
        });
    }
    tabs
}
```

**默认选中**：
- 有 git 变更 → 默认选中 "Current"
- 无 git 变更但有 TurnDiff → 默认选中最新 Turn
- 都没有 → 显示 "No changes" 空状态

### Phase 6: 注册 + 快捷键

1. **TUI Command 注册**：`peri-tui/src/command/mod.rs` 的 `default_registry()` 中添加 `DiffCommand`
2. **PanelKind 注册**：`peri-tui/src/app/panel_manager.rs` 的 `PanelKind` enum 添加 `Diff`
3. **PanelState 注册**：`PanelState` enum 添加 `Diff(DiffPanel)`
4. **dispatch 路由**：`dispatch_key/paste/scroll/mouse()` 新增 match arm

### Phase 7: 单元测试

```rust
// git_diff_source 测试
#[tokio::test]
async fn test_git_diff_fetch_in_repo() { /* 在 peri 仓库中运行，验证返回 files > 0 */ }

#[tokio::test]
async fn test_git_diff_fetch_not_repo() { /* 在 /tmp 中运行，验证返回 Error */ }

#[test]
fn test_git_diff_truncates_large_file() { /* 超过 MAX_FILE_BYTES 的文件标记 is_truncated */ }

// turn_diff_source 测试
#[test]
fn test_turn_diff_extracts_edit_events() { /* 构造 Edit ToolStart → 提取 DiffFileEntry */ }

#[test]
fn test_turn_diff_extracts_write_events() { /* 构造 Write ToolStart → 提取 DiffFileEntry */ }

#[test]
fn test_turn_diff_empty_when_no_tool_events() { /* 无工具事件时返回空 */ }

// diff_panel 测试
#[test]
fn test_diff_panel_default_tab() { /* 有 git 变更 → 选中 Current */ }

#[test]
fn test_diff_panel_tab_switch() { /* ←→ 切换 tab */ }

#[test]
fn test_diff_panel_file_navigation() { /* ↑↓ 移动选择 */ }

#[test]
fn test_diff_panel_enter_detail_and_back() { /* Enter 进入详情，Esc 返回列表 */ }
```

## 使用方式

```
/diff           → 打开 Diff 面板，默认显示 "Current"（git diff HEAD）
/diff           → ←/→ 切换到 "Turn 3" 查看 agent 第 3 轮修改的文件
/changes        → 同上（别名）
```

## 前置条件

- **Current 源**：项目是 git 仓库 + git 在 PATH 中（不满足时显示 "Not a git repository" 空状态，不报错）
- **TurnDiff 源**：无前置条件，从内存事件中提取

## 与现有 Diff 渲染的关系

peri 已有两套 diff 渲染路径：

| 路径 | 用途 | 复用 |
|------|------|------|
| `peri_widgets::diff` | Agent 工具调用的 inline diff（Edit/Write） | **直接复用**：`/diff` Detail 视图调用 `render_diff()` |
| `highlight_diff_line()` | LLM 输出中的纯文本 diff 着色 | 不需要：`/diff` 用结构化 diff |

## 与 `/commit` 的关系

`/diff` 查看变更，`/commit` 提交变更。两者互补：
- 用户先 `/diff` 检查变更 → 确认后 `/commit` 提交
- 两者共享 `run_git()` 基础设施

## Affected Files

### 新建
| 文件 | 职责 |
|------|------|
| `peri-tui/src/command/panel/diff.rs` | `DiffCommand` TUI 命令 |
| `peri-tui/src/diff/git_diff_source.rs` | `GitDiffSource` — 执行 git diff 获取工作区变更 |
| `peri-tui/src/diff/turn_diff_source.rs` | `TurnDiffSource` — 从 AgentEvent 提取每轮 diff |
| `peri-tui/src/diff/mod.rs` | diff 模块入口 |
| `peri-tui/src/ui/panels/diff_panel.rs` | `DiffPanel` — 文件列表 + 详情双视图，`PanelComponent` 实现 |

### 改造
| 文件 | 行号 | 改动 |
|------|------|------|
| `peri-tui/src/command/mod.rs` | `default_registry()` | 注册 `DiffCommand` |
| `peri-tui/src/app/panel_manager.rs` | L47-62, L139-153, L381-475 | 新增 `PanelKind::Diff` + `PanelState::Diff(DiffPanel)` + dispatch |
| `peri-tui/src/ui/message_view/mod.rs` | L24 | 提取 `build_diff_input()` 为公共函数供 `turn_diff_source` 复用 |

## 风险点

| 风险 | 影响 | 缓解 |
|------|------|------|
| 大仓库 `git diff HEAD` 输出巨大 | 内存占用 + 渲染卡顿 | `MAX_DIFF_BYTES = 100KB` 截断；`MAX_FILE_BYTES = 1MB` 标记 truncated |
| 二进制文件（图片、lockfile） | `git diff` 输出乱码 | `--numstat` 检测 `- -` 标记二进制；Detail 视图显示 "(binary file)" |
| Windows 下 git 输出 GBK 编码 | 中文文件名乱码 | `String::from_utf8_lossy()` + 后续可加 `--encoding=utf-8` |
| TurnDiff 与实际文件不一致 | agent 通过 Bash 修改文件但 Edit/Write 未记录 | TurnDiff 只展示 Edit/Write 工具修改的文件，与 Claude Code 行为一致，文档说明即可 |
| `build_diff_input()` 耦合在 `message_view` 中 | 复用时循环依赖 | 提取到独立函数或 `peri-tui/src/diff/utils.rs` |
