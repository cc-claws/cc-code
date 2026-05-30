# TUI 帧率限制 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 TUI 主事件循环的 loading 路径中引入 30 FPS 帧率限制，消除 agent 执行期间的 CPU 不必要开销。非 loading 路径（用户交互、缓存更新、agent 事件）不受影响。

**Architecture:** 单一修改点——`main.rs` 事件循环中的 `None` 分支（poll 超时路径）。添加 `TARGET_FRAME_INTERVAL` 常量 + `last_render: Instant` 追踪变量。loading 为 true 时检查距上次渲染的时间间隔，不足则跳过 `terminal.draw()`。`Some(action)` 分支（用户交互）始终立即渲染。

**Tech Stack:** Rust, ratatui (Terminal::draw), std::time::{Duration, Instant}

**Issue:** `spec/issues/2026-05-30-no-explicit-frame-rate-limit.md`

---

### Task 1: 添加帧率限制常量和追踪变量

**Files:**
- Modify: `peri-tui/src/main.rs`

- [ ] **Step 1: 添加 `use std::time::{Duration, Instant}` import**

在 `peri-tui/src/main.rs` 顶部 import 区域（第 15 行 `use std::io;` 之后）添加：

```rust
use std::time::{Duration, Instant};
```

- [ ] **Step 2: 定义帧率限制常量**

在事件循环开始前（第 686 行 `// 初始全量绘制一次` 之前）添加常量：

```rust
/// loading 动画帧率限制间隔（约 30 FPS）。
/// 仅在 loading=true 且无用户事件的 poll 超时路径生效，
/// 用户交互（键盘/鼠标/resize）始终立即渲染。
const TARGET_FRAME_INTERVAL: Duration = Duration::from_millis(33);
```

- [ ] **Step 3: 初始化 `last_render` 追踪变量**

将第 684-685 行：

```rust
    // 初始全量绘制一次
    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
```

改为：

```rust
    // 初始全量绘制一次
    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
    let mut last_render = Instant::now();
```

`last_render` 在初始渲染后立即记录，确保首次事件循环迭代的时间戳基准正确。

- [ ] **Step 4: 在事件循环的 `None` 分支中引入帧率限制**

将第 717-735 行的 `None` 分支：

```rust
            None => {
                // 无用户事件（poll 超时）：在阻塞结束后重新读取缓存版本
                // 这样能捕获渲染线程在等待期间发出的更新
                let cache_version = app.session_mgr.sessions[app.session_mgr.active]
                    .messages
                    .render_cache
                    .read()
                    .version;
                let cache_updated = cache_version
                    != app.session_mgr.sessions[app.session_mgr.active]
                        .messages
                        .last_render_version;
                if cache_updated
                    || agent_updated
                    || bg_updated
                    || app.session_mgr.sessions[app.session_mgr.active].ui.loading
                {
                    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
                }
            }
```

改为：

```rust
            None => {
                // 无用户事件（poll 超时）：在阻塞结束后重新读取缓存版本
                // 这样能捕获渲染线程在等待期间发出的更新
                let cache_version = app.session_mgr.sessions[app.session_mgr.active]
                    .messages
                    .render_cache
                    .read()
                    .version;
                let cache_updated = cache_version
                    != app.session_mgr.sessions[app.session_mgr.active]
                        .messages
                        .last_render_version;
                let loading = app.session_mgr.sessions[app.session_mgr.active].ui.loading;
                let should_render = cache_updated || agent_updated || bg_updated || loading;
                if should_render {
                    let now = Instant::now();
                    // loading 路径：限制帧率到 TARGET_FRAME_INTERVAL，降低 CPU 开销
                    // 非 loading 路径（cache_updated/agent_updated/bg_updated）始终立即渲染
                    if !loading || now.duration_since(last_render) >= TARGET_FRAME_INTERVAL {
                        terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
                        last_render = now;
                    }
                }
            }
```

关键设计决策：
- **`!loading` 分支不受限制**：`cache_updated`/`agent_updated`/`bg_updated` 触发的渲染始终立即执行。这些事件本身就是事件驱动的（有实际数据变化），不存在空转问题。
- **`loading` 分支帧率限制**：loading spinner 动画在无数据变化时仅需要 30 FPS 刷新，超过此频率的 draw 调用全是浪费。
- **`Some(action)` 分支不受影响**：用户交互（`Action::Submit`/`Action::Redraw`）在第 706-715 行直接调用 `terminal.draw()`，不经过 `should_render` 逻辑，始终立即渲染。
- **`last_render` 在 `Some(action)` 分支中不更新**：用户交互后 `last_render` 保持旧值，下一个 `None` 分支如果 loading 为 true，会按间隔正常触发。这避免了用户交互后 loading 帧率限制被"重置"导致延迟。

- [ ] **Step 5: 在 `Action::Redraw` 和 `Action::Submit` 分支中同步更新 `last_render`**

将第 706-715 行：

```rust
            Some(action) => match action {
                event::Action::Quit => break 'event_loop,
                event::Action::Submit(input) => {
                    app.submit_message(input);
                    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
                }
                event::Action::Redraw => {
                    // 有用户交互（键盘/鼠标/resize）→ 始终重绘
                    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
                }
            },
```

改为：

```rust
            Some(action) => match action {
                event::Action::Quit => break 'event_loop,
                event::Action::Submit(input) => {
                    app.submit_message(input);
                    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
                    last_render = Instant::now();
                }
                event::Action::Redraw => {
                    // 有用户交互（键盘/鼠标/resize）→ 始终重绘
                    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
                    last_render = Instant::now();
                }
            },
```

同步 `last_render` 的原因：用户交互触发的渲染是最新的，`last_render` 应反映这个时间点。如果不更新，可能出现场景——用户操作后立即进入 loading，`last_render` 还是旧值，导致 loading 首帧被跳过（但实际上用户操作刚渲染完，间隔极短，33ms 内的跳过是合理的）。**更新 `last_render` 更保守正确**——确保帧率间隔从最后一次实际渲染开始计算，无论触发来源。

- [ ] **Step 6: 构建验证**

```bash
cargo build -p peri-tui
```

预期：编译通过，无 warning。

- [ ] **Step 7: Commit**

```bash
git add peri-tui/src/main.rs
git commit -m "perf(tui): add 30 FPS frame rate limit for loading animation

Loading spinner no longer redraws at uncapped frame rate. Only the
poll-timeout path (no user event) is rate-limited to 33ms intervals;
user interactions and data-driven updates remain immediate.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 2: 手动验证帧率限制效果

**Files:** 无代码修改

- [ ] **Step 1: 启动 TUI 并触发 agent 执行**

```bash
cargo run -p peri-tui
```

输入一个需要 agent 执行的 prompt（如 "list files in current directory"），观察 loading 动画是否流畅（30 FPS 足以看起来流畅）。

- [ ] **Step 2: 观察流式文本渲染**

在 agent 回复流式文本时，验证文本即时出现（不受帧率限制影响）。流式文本由 `cache_updated` 触发，走 `!loading` 分支。

- [ ] **Step 3: 快速输入验证响应性**

在 loading 期间快速输入键盘/鼠标操作，验证 UI 响应无延迟。用户交互走 `Some(action)` 分支，始终立即渲染。

---

## Self-Review

### Spec coverage

| 需求 | Task |
|------|------|
| 添加 TARGET_FRAME_INTERVAL = 33ms 常量 | Task 1 Step 2 |
| last_render: Instant 追踪 | Task 1 Step 3, 5 |
| loading 路径帧率限制 | Task 1 Step 4 |
| 非 loading 路径不受限制 | Task 1 Step 4 (`!loading` 分支) |
| 首次渲染不延迟 | Task 1 Step 3 (初始渲染后记录) |
| 验证 CPU 占用下降 | Task 2 |

### Placeholder scan

无 TBD/TODO/占位符。所有步骤包含具体代码或命令。

### Type consistency

- `TARGET_FRAME_INTERVAL: Duration` 与 `Instant::duration_since` 返回类型匹配
- `last_render: Instant` 在所有 `terminal.draw()` 调用后更新，类型一致

### 边界条件分析

| 场景 | 行为 | 正确性 |
|------|------|--------|
| 首次进入事件循环，loading=true | `last_render` = 初始渲染时间，正常帧率限制 | 正确 |
| loading=true + 33ms 内多次 poll 超时 | 后续超时跳过 draw，CPU 节省 | 正确 |
| loading=true + 用户操作 | `Some(action)` 立即渲染，`last_render` 更新 | 正确 |
| loading=false + agent_updated | `!loading` 条件满足，立即渲染 | 正确 |
| loading=false + cache_updated | `!loading` 条件满足，立即渲染 | 正确 |
| loading=true → loading=false 转变 | 首次 cache_updated/agent_updated 立即渲染 | 正确 |
| 多 session 切换 | `last_render` 是全局的，但影响仅限 loading 路径 | 可接受（多 session 共享同一 terminal，帧率限制全局合理） |
