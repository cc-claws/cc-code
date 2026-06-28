# Ctrl+B Background Shell 机制实现

## Status
- [ ] Phase 1: 数据模型与存储层
- [ ] Phase 2: Shell 执行改造
- [ ] Phase 3: UI — BackgroundTasksPanel
- [ ] Phase 4: Stall Watchdog
- [ ] Phase 5: 任务完成通知
- [ ] Phase 6: Ctrl+B 快捷键改造
- [ ] Phase 7: 通知集成

## Created
2026-06-27

## Severity
Feature — 核心交互能力

## Platform
全平台 (Windows / macOS / Linux)

## Problem

当前 peri (cc-code) 的 Shell 命令执行是阻塞式的：用户或 agent 发起一个 shell 命令后，UI 被锁定直到命令结束。长命令（如 `npm test`、`cargo build`）会阻塞整个交互，用户无法继续对话。

TS 版 Claude Code 已实现完整的 Ctrl+B Background Shell 机制（参考 PRD：`C:\Work\open-cladue\docs\ctrl-b-background-shell.html`，已验证 37/37 项声明与源码一致）。

## Root Cause — 现有架构限制

### 限制 1：单命令阻塞

`ShellCommandRuntime`（`shell_command.rs:11-22`）只支持 1 个运行中的 shell 命令：

```rust
pub struct ShellCommandRuntime {
    pub stdin_tx: Option<mpsc::Sender<String>>,          // L13
    pub running_record_id: Option<String>,                // L14
    pub abort_handle: Option<AbortHandle>,                // L16
    pub command: String,                                  // L17
    pub cwd: String,                                      // L18
    pub started_at: Option<chrono::DateTime<Utc>>,        // L20
    // ...
}
```

`run_shell_command()`（`shell_command.rs:31`）中 L38 的 `loading` guard 直接阻塞并发：

```rust
if self.session_mgr.current().ui.loading {
    return;  // 有命令在跑，拒绝新命令
}
```

### 限制 2：输出无持久化

`execute_shell_command_with_stdin()`（`shell_exec.rs:25-29`）中 stdout/stderr 通过 `tokio::spawn(read_to_end)`（L68-75）一次性读取到内存，进程退出后才返回 `CommandOutput`。**无流式输出、无磁盘持久化**。后台化后输出会丢失。

### 限制 3：Ctrl+B 功能有限

当前 Ctrl+B（`shortcuts.rs:36-42`）仅聚焦 bg agent bar：

```rust
if SHORTCUT_BG_BAR.matches(key_event) {
    if !app.session_mgr.current_mut().background_agents.is_empty() {
        app.session_mgr.current_mut().ui.bg_bar_cursor = Some(0);
    }
    return Some(Action::Redraw);
}
```

不支持 shell 命令后台化。

### 限制 4：无任务生命周期管理

无 running/completed/failed/killed 状态机，无 stall watchdog，无完成通知。

---

## Fix Proposal

### Phase 1: 数据模型与存储层

**目标**：定义后台 Shell 核心数据结构和磁盘输出存储

#### 1.1 `BackgroundShell` 状态模型

**新建** `peri-tui/src/app/background_shell.rs`：

```rust
pub struct BackgroundShell {
    pub id: String,                         // uuid7
    pub command: String,
    pub cwd: PathBuf,
    pub status: ShellStatus,
    pub is_backgrounded: bool,
    pub started_at: Instant,
    pub ended_at: Option<Instant>,
    pub exit_code: Option<i32>,
    pub notified: bool,                     // 是否已通知 agent
    pub output_path: PathBuf,               // 磁盘输出文件路径
    pub result_rx: Option<oneshot::Receiver<Result<CommandOutput>>>,  // 进程退出时 resolve
    pub stall_watchdog: Option<tokio::task::JoinHandle<()>>,
}

pub enum ShellStatus { Running, Completed, Failed, Killed }
```

**注意**：`result_rx` 是 `oneshot::Receiver`（进程退出时 resolve），不是 `JoinHandle`。项目中 `langfuse_state.rs:11` 用 `JoinHandle` 是因为它直接持有 tokio task，这里用 oneshot 是因为进程退出信号来自 `execute_shell_command_streaming()` 的 result channel。

集成点：`ChatSession`（`chat_session.rs`）新增 `background_shells: Vec<BackgroundShell>` 字段。

#### 1.2 `DiskOutput` 磁盘输出模块

**新建** `peri-agent/src/task_output.rs`：

```rust
pub struct DiskOutput {
    file: tokio::fs::File,
    queue: Vec<Vec<u8>>,
    written_bytes: u64,
}

const MAX_OUTPUT_BYTES: u64 = 5 * 1024 * 1024 * 1024;  // 5 GB
const MAX_READ_BYTES: u64 = 8 * 1024 * 1024;            // 8 MB
```

API：
- `write(id, data)` — 追加到队列，批量 drain 到 `file.write_all()`
- `read_tail(id, bytes)` — 读取文件末尾 N 字节
- `read_delta(id, from_offset)` — 从 offset 读取新字节
- `flush(id)` — 等待队列 drain 完成
- `evict(id)` — 释放内存句柄（`outputs.delete(id)`），保留文件
- `cleanup(id)` — `tokio::fs::remove_file()` 删除文件

输出路径：`{temp_dir}/peri-{uid}/{sanitize(cwd)}/{session_id}/tasks/{id}.output`

**验证**：单元测试写入/读取/增量读取/evict/cleanup

---

### Phase 2: Shell 执行改造

**目标**：Shell 命令支持后台化，解除单命令限制

#### 2.1 引入 `ShellCommandPool`

**改造** `peri-tui/src/app/shell_command.rs`：

保留 `ShellCommandRuntime` 不变（不破坏 `!command` 流程），在外层包装 pool：

```rust
pub struct ForegroundShell {
    pub runtime: ShellCommandRuntime,
    pub output_rx: mpsc::Receiver<Vec<u8>>,                   // 流式输出 channel，background_foreground() 时取出
    pub result_rx: oneshot::Receiver<Result<CommandOutput>>, // 进程退出时 resolve
    pub accumulated_output: Vec<u8>,                        // 前台模式累积输出（用于 UI 渲染）
    pub reader_abort: AbortHandle,                          // 停止前台 output_rx 消费 task
}

pub struct ShellCommandPool {
    pub foreground: Option<ForegroundShell>,              // 当前前台命令（最多 1 个）
    pub background: HashMap<String, BackgroundShell>,     // 后台命令（多个）
}
```

`ChatSession.shell_command: ShellCommandRuntime` → `ChatSession.shell_pool: ShellCommandPool`

`loading` 标志改为 `shell_pool.foreground.is_some()`。

**关键设计**：`output_rx` 存在 `ForegroundShell` 中。前台模式下，tokio task 循环读取 `output_rx` 并累积到 `accumulated_output`（兼容现有 UI 渲染）。Ctrl+B 时 `background_foreground()` 从 `foreground` 中取出 `output_rx`，切换给 `DiskOutputWriter` 消费。**同一个 channel 接收端，切换消费者，不重跑进程。**

#### 2.2 实现 `background_foreground()`

当 Ctrl+B 触发时：

**核心原则：绝不重跑进程。** 进程继续运行，只切换输出目标。

**前置依赖**：Phase 2.3 的流式输出改造必须先完成。`execute_shell_command_streaming()` 返回 `ShellExecution`（含 `output_rx: mpsc::Receiver<Bytes>`），进程 stdout/stderr 通过 channel 流式推送。

**`background_foreground()` 操作步骤**：

1. 取出 `pool.foreground`（`ForegroundShell`），从中提取 `runtime`、`output_rx`、`accumulated_output`
2. 停止前台模式的 `output_rx` 消费 tokio task（通过 AbortHandle）
3. 创建 `DiskOutputWriter`，spawn 新的 tokio task 将 `output_rx` 的每个 chunk 写入磁盘文件
4. 创建 `BackgroundShell`，`is_backgrounded = true`，`result_rx` 从 `ForegroundShell` 继承
5. 启动 stall watchdog（Phase 4）
6. `pool.foreground = None`，`set_loading(false)`
7. push 到 `pool.background`

**进程全程不中断**，只是输出从"渲染到 UI"切换为"写入磁盘文件"。

```
前台模式：  process.stdout → output_rx → 渲染到 UI (ratatui Paragraph)
                ↓ Ctrl+B
后台模式：  process.stdout → output_rx → DiskOutputWriter → .output 文件
```

**⚠️ 已废弃方案（abort + re-spawn）**：对有副作用的命令（`rm -rf`、`git push`、数据库 migration）会导致重复执行，绝对不可用。

#### 2.3 改造 `execute_shell_command_with_stdin()` — 流式输出（Phase 2.2 前置依赖）

**改造** `peri-tui/src/shell_exec.rs`：

当前 stdout/stderr 通过 `read_to_end` 一次性读取（L68-75）。**必须**改为流式，因为 `background_foreground()` 需要在进程运行中途切换输出目标。

```rust
// 现有（L68-75）：
let stdout_task = tokio::spawn(async move {
    let mut buf = Vec::new();
    stdout.read_to_end(&mut buf).await.map(|_| buf)
});

// 改为流式读取 loop：
let (stdout_tx, stdout_rx) = mpsc::channel::<Vec<u8>>(256);
tokio::spawn(async move {
    let mut buf = [0u8; 8192];
    loop {
        match stdout.read(&mut buf).await {
            Ok(0) => break, // EOF
            Ok(n) => { let _ = stdout_tx.send(buf[..n].to_vec()).await; }
            Err(_) => break,
        }
    }
});
// stderr 同理
```

返回值改为持有句柄的 struct：

```rust
pub struct ShellExecution {
    pub result: oneshot::Receiver<Result<CommandOutput>>,  // 进程退出时 resolve
    pub abort: AbortHandle,
    pub stdin_tx: Option<mpsc::Sender<String>>,
    pub output_rx: mpsc::Receiver<Vec<u8>>,  // 流式输出 channel
}
```

**前台模式消费 `output_rx`**：在 `run_shell_command()` 中 spawn tokio task 循环读取 `output_rx`，累积到内存，进程退出后渲染到 UI（兼容现有行为）。

**后台模式消费 `output_rx`**：`background_foreground()` 将 `output_rx` 接管给 `DiskOutputWriter`，写入磁盘文件。

**验证**：启动 `sleep 5 && echo done`，Ctrl+B 后台化，确认进程不中断、输出写入磁盘、输出内容完整无丢失

#### 2.4 实现 `spawn_shell_task()` — 直接后台路径

参考 PRD §9，agent 工具（如 BashTool）可以直接创建后台任务，跳过前台阶段。直接使用 `execute_shell_command_streaming()` 的流式输出，`output_rx` 直接交给 `DiskOutputWriter`：

```rust
pub fn spawn_shell_task(&mut self, command: String, cwd: String) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let execution = execute_shell_command_streaming(&command, &cwd, None);
    let output_path = task_output_path(&id);
    // output_rx 直接写磁盘，不经过前台
    spawn_disk_writer(execution.output_rx, &output_path);
    let bg = BackgroundShell {
        id: id.clone(), command, cwd: PathBuf::from(&cwd),
        status: ShellStatus::Running, is_backgrounded: true,
        started_at: Instant::now(), ended_at: None, exit_code: None,
        notified: false, output_path,
        result_rx: Some(execution.result),  // oneshot::Receiver，进程退出时 resolve
        stall_watchdog: None,
    };
    self.register_completion_and_watchdog(&bg);  // spawn tokio task poll result_rx + 启动 watchdog
    self.shell_pool.background.insert(id.clone(), bg);
    id
}
```

---

### Phase 3: UI — BackgroundTasksPanel

**目标**：新建后台任务列表面板 + Shell 详情视图

#### 3.1 新增 `PanelKind::BackgroundTasks`

**改造** `peri-tui/src/app/panel_manager.rs`：

```rust
// PanelKind enum（L47-62）新增：
pub enum PanelKind {
    // ... 现有 13 个 variant ...
    BackgroundTasks,  // 新增
}

// PanelState enum（L139-153）新增：
pub enum PanelState {
    // ... 现有 13 个 variant ...
    BackgroundTasks(BackgroundTasksPanel),  // 新增
}
```

- `scope()`: `PanelScope::Session`（与当前 session 绑定）
- `mutex_group()`: `None`（overlay 式，不与其它面板互斥）
- `dispatch_key/paste/scroll/mouse()`（L381-475）新增 match arm

#### 3.2 实现 `BackgroundTasksPanel`

**新建** `peri-tui/src/ui/panels/background_tasks.rs`：

```rust
pub struct BackgroundTasksPanel {
    view: ViewState,
    selected_index: usize,
}

enum ViewState {
    List,
    Detail { item_id: String },
}
```

**渲染**（实现 `PanelComponent` trait）：

- **List 模式**：分组顺序（参考 PRD §4，7 组）：
  1. **Teammates**（agents）— 包含 in-process teammate 和 leader
  2. **Shells**（local_bash）— `shell_pool.background` 中的 `BackgroundShell`
  3. **Monitors**（monitor_mcp）— ⚠️ peri 当前未实现，预留空分组
  4. **Remote agents**（remote_agent）— ACP 远程 agent
  5. **Local agents**（local_agent）— SubAgent 后台任务（`background_agents`）
  6. **Workflows**（local_workflow）— ⚠️ peri 当前未实现，预留空分组
  7. **Dreams** — ⚠️ peri 当前未实现，预留空分组
  - **MVP 实现**：只渲染有数据的分组（Shells + Local agents），空分组不显示
  - 每项：名称 + status badge（`running`/`completed`/`failed`/`killed`）+ elapsed time
  - 选中项高亮（`Style::default().bg(theme::SURFACE_2)`）
  - 副标题行显示汇总："1 active shell · 1 completed agent"
- **Detail 模式**：
  - Status 行、Runtime 行、Command 行
  - Output 框：`Block::bordered().title("Output")`，`height=12`（参考 PRD §5 的 `height=12`）
  - 读取常量：`SHELL_DETAIL_TAIL_BYTES = 8192`（读取文件末尾 8192 字节），取最后 10 行显示
  - 底部显示 "Showing N lines of X.X KB"
  - 运行中任务每 1000ms 刷新输出（tokio `interval` + dirty flag，渲染时只读缓存避免卡帧）
  - 如果只有 1 个后台任务，自动跳到 Detail 视图（参考 PRD §4 的 ViewState 自动跳转逻辑）

**快捷键**（`handle_key()`）：

| 键 | 行为 |
|----|------|
| `↑/↓` | 选择任务 |
| `Enter` | 进入 Detail 视图 |
| `x` | kill 选中的运行中任务 |
| `←/Esc` | List 模式关闭面板；Detail 模式返回 List |
| `Space` | Detail 模式关闭面板 |

#### 3.3 扩展 Footer Status Bar

**改造** `peri-tui/src/ui/main_ui/status_bar.rs`：

现有 bg agent 指示器（L292-307）显示 `· N bg agents`。扩展为同时显示 shell 计数：

```rust
// 现有（L292-307）：
if !app.session_mgr.current().background_agents.is_empty() {
    left_spans.push(Span::styled(" · N bg agents", WARNING));
}

// 扩展为：
let bg_shell_count = app.session_mgr.current().shell_pool.background_count();
let bg_agent_count = app.session_mgr.current().background_agents.len();
if bg_shell_count + bg_agent_count > 0 {
    // 标签生成：参考 TS 版 pillLabel.ts
    // "1 shell" / "2 shells" / "1 shell, 1 agent" 等
    left_spans.push(Span::styled(format!(" · {}", pill_label), WARNING));
    // "· ↓ to view" 提示
    left_spans.push(Span::styled(" · ↓ to view", MUTED));
}
```

**验证**：启动后台任务 → 状态栏显示 pill → `↓` 打开面板

---

### Phase 4: Stall Watchdog

**目标**：检测后台命令卡在等待用户输入

**追加**到 `peri-tui/src/app/background_shell.rs`：

```rust
const STALL_CHECK_INTERVAL_MS: u64 = 5_000;
const STALL_THRESHOLD_MS: u64 = 45_000;
const STALL_TAIL_BYTES: u64 = 1024;
```

逻辑：

```
每 5 秒 tokio::interval tick：
  ├─ tokio::fs::metadata(output_path).size
  │   ├─ size > last_size → last_size = size, stall_since = None
  │   └─ size == last_size →
  │       ├─ stall_since 未设置 → stall_since = now
  │       └─ stall_since 已设置 →
  │           ├─ now - stall_since < 45s → continue
  │           └─ now - stall_since >= 45s →
  │               ├─ tail_file(output_path, 1024)
  │               ├─ 末行匹配 PROMPT_PATTERNS →
  │               │   发送通知给 agent，cancelled = true，break
  │               └─ 不匹配 → continue
```

PROMPT_PATTERNS（末行匹配，与 TS 版一致）：

```rust
const PROMPT_PATTERNS: &[&str] = &[
    r"\(y/n\)", r"\[y/n\]", r"\(yes/no\)",
    r"Do you|Would you|Shall I|Are you sure|Ready to",
    r"Press (Enter|any key)", r"Continue\?", r"Overwrite\?",
];
```

- 触发后 one-shot：`cancelled = true; break;`
- 对 `monitor` 类型跳过（`if kind == "monitor" { return; }`）

**验证**：启动 `read -p "Continue? " _` 后台化 → 45 秒后收到 watchdog 通知

**单元测试加速**：`STALL_THRESHOLD_MS` 通过 `#[cfg(test)]` 注入为 500ms，测试时 1 秒内触发。示例：

```rust
#[cfg(test)]
const STALL_THRESHOLD_MS: u64 = 500;  // 测试时加速
#[cfg(not(test))]
const STALL_THRESHOLD_MS: u64 = 45_000;
```

---

### Phase 5: 任务完成通知

**目标**：后台命令结束时自动通知 agent

#### 5.1 `enqueue_shell_notification()`

**追加**到 `peri-tui/src/app/background_shell.rs`：

进程退出时（`shell_handle` resolve）：

1. 更新 `status → Completed/Failed`，记录 `exit_code`、`ended_at`
2. 生成结构化通知消息：
   ```
   [Background Task Completed]
   task_id: b8f3a2c1
   command: npm test
   status: completed (exit 0)
   output: /tmp/peri-1000/.../tasks/b8f3a2c1.output
   ```
3. 通过 `bg_event_tx.send(AgentEvent::BackgroundShellCompleted { ... })` 发送到主循环

#### 5.2 新增 AgentEvent 变体

**改造** `peri-tui/src/app/events.rs`（L10-158）：

```rust
// 在 BackgroundTaskCompleted（L129-138）之后新增：
BackgroundShellCompleted {
    id: String,
    command: String,
    exit_code: Option<i32>,
    output_path: PathBuf,
},
```

**验证**：后台运行 `echo hello && sleep 1` → 退出后 agent 自动收到通知

---

### Phase 6: Ctrl+B 快捷键改造

**目标**：有前台 shell 时后台化，否则保留原行为

**改造** `peri-tui/src/event/keyboard/shortcuts.rs:36-42`：

```rust
// 现有：
if SHORTCUT_BG_BAR.matches(key_event) {
    if !app.session_mgr.current_mut().background_agents.is_empty() {
        app.session_mgr.current_mut().ui.bg_bar_cursor = Some(0);
    }
    return Some(Action::Redraw);
}

// 改为：
if SHORTCUT_BG_BAR.matches(key_event) {
    let session = app.session_mgr.current_mut();
    if session.shell_pool.foreground.is_some() {
        // 有前台 shell → 后台化
        session.shell_pool.background_foreground();
    } else if !session.background_agents.is_empty() {
        // 无前台 shell → 保留原行为：聚焦 bg agent bar
        session.ui.bg_bar_cursor = Some(0);
    }
    return Some(Action::Redraw);
}
```

#### 2 秒阈值提示

**改造** `peri-tui/src/ui/main_ui/` 中 shell 命令输出的渲染区域：

```rust
const PROGRESS_THRESHOLD_MS: i64 = 2000;

// shell 命令运行超过 2 秒后，在输出底部显示：
// "(Ctrl+B to run in background)"
```

---

### Phase 7: 通知集成

**目标**：后台任务完成消息注入 agent 对话流

**改造** `peri-tui/src/main.rs` 事件循环（L861-984）：

在 `poll_agent()`（L880）附近新增 `poll_background_shell_events()`：

```rust
// main.rs 事件循环 L878-887 附近：
agent_updated |= app.poll_agent();
agent_updated |= app.poll_at_mention();
let bg_updated = app.poll_background_events();
let shell_bg_updated = app.poll_background_shell_events();  // 新增
let panic_updated = app.poll_panic_notifications();
app.poll_cron_triggers();
```

`poll_background_shell_events()` 逻辑：
1. 从 `bg_event_rx` 接收 `AgentEvent::BackgroundShellCompleted`
2. 查找对应 `BackgroundShell`，标记 `notified = true`
3. 如果 agent idle（`!session.ui.loading`）→ 调用 `app.submit_message(notification_text)` 直接注入为 user message（`agent_submit.rs:4`）
4. 如果 agent 推理中（`session.ui.loading`）→ 暂存到 `session.pending_bg_shell_notifications: VecDeque<String>`
5. agent 完成当前轮次后（`AgentEvent::Done` 处理中）检查 `pending_bg_shell_notifications`，非空则自动 `submit_message()`

agent 可通过 `FileReadTool` 读取 `output_path` 获取完整输出。

渲染条件（L935-940）扩展：
```rust
let should_render = cache_updated || agent_updated || bg_updated || shell_bg_updated || panic_updated || loading || cursor_blinked;
```

---

## 实施顺序

```
Phase 1 (数据模型)
    ↓
Phase 2.3 (流式输出改造) ← 必须最先，是 2.2/2.4 的前置依赖
    ↓
Phase 2.1 (ShellCommandPool) + Phase 2.2 (background_foreground) + Phase 2.4 (spawn_shell_task)
    ↓
Phase 4 (Watchdog) + Phase 5 (完成通知) — 可并行
    ↓
Phase 3 (UI 面板)
    ↓
Phase 6 (快捷键) + Phase 7 (通知集成) — 可并行
```

理由：流式输出（2.3）是所有 shell 后台化的基础，必须最先完成。Pool + background + spawn（2.1-2.4）是一个完整的 PR。然后是 watchdog/通知（4、5），最后拼 UI（3、6）和 agent 集成（7）。

## 完整生命周期状态图

参考 PRD §9，一个后台 Shell 任务的完整状态转换：

```
register_foreground()          spawn_shell_task()
       ↓                              ↓
  is_backgrounded: false         is_backgrounded: true
  (前台，2 秒后显示提示)          (直接后台，agent 工具调用)
  output_rx → UI 渲染            output_rx → DiskOutput
       ↓                              ↓
       └────── Ctrl+B ───────────────┘
                  ↓
         background_foreground()
         is_backgrounded = true
         output_rx 从 UI 切换到 DiskOutput（进程不中断）
         stall watchdog 启动
                  ↓
    ┌─────────────┼─────────────┐
    ↓             ↓             ↓
  进程正常退出  进程异常退出   用户按 x kill
    ↓             ↓             ↓
 completed      failed        killed
 exit_code      exit_code     abort_handle.abort()
    ↓             ↓             ↓
    └───── 同样触发通知 ────────┘
                  ↓
   enqueue_shell_notification()
   AgentEvent::BackgroundShellCompleted
   notified = true
                  ↓
   下次 poll_background_shell_events() 清理
   从 shell_pool.background 中移除
   DiskOutput::cleanup(id) 删除文件
   result_rx 已 consumed（oneshot），无需额外清理
```

**两条创建路径**：
- `register_foreground()`：agent 执行 BashTool 时，运行满 2 秒后注册（`is_backgrounded: false`），等用户 Ctrl+B
- `spawn_shell_task()`：agent 工具直接创建后台任务（`is_backgrounded: true`），跳过前台阶段

## 快捷键速查表

参考 PRD §10，所有与后台任务相关的键盘操作：

| 快捷键 | 上下文 | 作用 |
|--------|--------|------|
| `Ctrl+B` | 前台 shell 运行时 | 将所有前台任务转入后台 |
| `↓` | 主界面（有后台任务时） | 打开 BackgroundTasksPanel |
| `↑/↓` | BackgroundTasksPanel 列表 | 上下选择任务 |
| `Enter` | BackgroundTasksPanel 列表 | 进入任务详情 |
| `x` | BackgroundTasksPanel / 详情 | kill 选中的运行中任务 |
| `f` | BackgroundTasksPanel (teammate) | 前台化选中的 teammate 任务 |
| `←/Esc` | BackgroundTasksPanel 列表 | 关闭面板 |
| `←/Esc` | Detail 视图 | 返回列表（或关闭） |
| `Space` | Detail 视图 | 关闭面板 |
| `Ctrl+X Ctrl+K` | BackgroundTasksPanel | kill 所有运行中的 agent |

**注意**：peri 的 `f` 键和 `Ctrl+X Ctrl+K` 对应现有 bg agent bar 的操作（`shortcuts.rs` 中 `SHORTCUT_BG_BAR` 聚焦后的 bar_focus 模式）。BackgroundTasksPanel 需要复用或扩展这些快捷键。

## 风险点

| 风险 | 影响 | 缓解 |
|------|------|------|
| Windows 下 `cmd /C` 子进程管理 | `kill_on_drop(true)`（`shell_exec.rs:41`）只杀根进程，不杀进程树。子进程会被 orphan | 用 Windows Job Object（`CreateJobObject` + `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`）包裹子进程；或在 `cmd /C` 外层包装 `taskkill /T /F /PID` 命令手动杀树。MVP 阶段可接受 orphan 风险，后续优化 |
| `ShellCommandPool` 改造破坏现有 `!command` 流程 | 回归风险 | 保留 `ShellCommandRuntime` 不变，pool 是外层包装；`foreground` 字段语义等价于原有 `shell_command` |
| ratatui 渲染循环中读取磁盘输出 | 1000ms 轮询可能卡帧 | 用 tokio `interval` + dirty flag，渲染时只读缓存；参考现有 `render_cache.version` 模式 |
| `ChatSession` 持有 `JoinHandle` | borrow checker 生命周期问题 | 项目已有先例：`langfuse_state.rs:11` 用 `Option<JoinHandle<()>>` 存储后台任务。`background_shell.rs` 复用同一模式即可，无需引入新类型 |
| `execute_shell_command_with_stdin` 改为流式输出 | 现有 `read_to_end` 逻辑被破坏 | 新增 `execute_shell_command_streaming()` 函数，保留原 `execute_shell_command_with_stdin()` 不变（`!command` 用户命令仍用旧函数），按需调用 |

## Affected Files

### 新建
| 文件 | 职责 |
|------|------|
| `peri-tui/src/app/background_shell.rs` | `BackgroundShell` 状态模型 + watchdog + 通知生成 |
| `peri-agent/src/task_output.rs` | `DiskOutput` 磁盘输出存储（异步队列写入、tail 读取、增量读取） |
| `peri-tui/src/ui/panels/background_tasks.rs` | 后台任务面板（List/Detail 双视图，`PanelComponent` 实现） |

### 改造
| 文件 | 行号 | 改动 |
|------|------|------|
| `peri-tui/src/app/shell_command.rs` | L11-22, L31-131 | 引入 `ShellCommandPool` + `ForegroundShell`，新增 `background_foreground()`。**注意**：`session.shell_command` 有 30 处引用跨 12 个文件（详见 cross-check），rename 为 `shell_pool` 需同步修改 |
| `peri-tui/src/shell_exec.rs` | L25-85 | 新增 `execute_shell_command_streaming()`（流式 stdout/stderr → mpsc channel），保留原函数不变 |
| `peri-tui/src/app/events.rs` | L129-138 | 新增 `AgentEvent::BackgroundShellCompleted` variant |
| `peri-tui/src/app/panel_manager.rs` | L47-62, L139-153, L381-475 | 新增 `PanelKind::BackgroundTasks` + dispatch |
| `peri-tui/src/event/keyboard/shortcuts.rs` | L36-42 | Ctrl+B 逻辑：前台 shell 判断 + `background_foreground()` |
| `peri-tui/src/ui/main_ui/status_bar.rs` | L292-307 | 扩展 pill 显示 shell 计数 + `↓ to view` |
| `peri-tui/src/main.rs` | L878-887, L935-940 | 新增 `poll_background_shell_events()` + 渲染条件 |
