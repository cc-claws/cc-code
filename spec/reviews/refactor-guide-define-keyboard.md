# define.rs + keyboard.rs 重构指南

## 1. `define.rs`（1242 行）

### 当前结构

```
 29- 41  DeregisterGuard             (~13行, RAII guard)
 43- 74  AGENT_DESCRIPTION           (~32行, doc string const)
 76-112  SubAgentTool struct          (~37行, 18 fields)
114-942  impl SubAgentTool            (~829行!)
         ├ 114-139   new()                      builder
         ├ 142-209   11 个 with_xxx()            builder
         ├ 211-235   load_agent_def()           加载
         ├ 237-260   overrides + hook           辅助
         ├ 262-269   filter_tools()             工具过滤
         ├ 270-448   invoke_fork()              fork 执行(~179行)
         ├ 450-718   invoke_background()         bg 执行(~269行)
         └ 720-941   invoke_background_fork()   bg fork 执行(~222行)
945-1242 impl BaseTool for SubAgentTool (~298行)
         ├ 945-993   name/desc/parameters       元数据
         └ 995-1242  invoke()                   调度逻辑(~247行)
```

### 问题分析

| 区块 | 行数 | 可拆分度 | 说明 |
|------|------|---------|------|
| 11 个 `with_xxx()` 方法 | ~70 | ✅ 高 | 纯 builder，无逻辑 |
| `invoke_fork()` | ~179 | ✅ 高 | 独立执行路径，与其他路径无共享状态 |
| `invoke_background()` | ~269 | ✅ 高 | 独立执行路径 |
| `invoke_background_fork()` | ~222 | ✅ 高 | 独立执行路径 |
| `BaseTool::invoke()` | ~247 | ⚠️ 中 | 调度+参数解析+结果格式化 |
| `SubAgentTool` struct | ~37 | ❌ 低 | 字段多但没必要拆 |

### 方案：拆为 4 个子模块

```
subagent/tool/
├── mod.rs              → 保留 define.rs 的公共类型重导出
├── define.rs           → 缩减至 ~250 行（struct + builder + BaseTool impl）
├── execute_fork.rs     → invoke_fork() (~180行)
├── execute_bg.rs       → invoke_background() + invoke_background_fork() (~490行)
└── execute_normal.rs   → 从 BaseTool::invoke() 中拆出同步/普通异步路径 (~200行)
```

**步骤：**

#### Step 1: `execute_fork.rs` ← `invoke_fork()`

```rust
// subagent/tool/execute_fork.rs
use super::define::{SubAgentTool, DeregisterGuard};
use super::{build_subagent_middlewares, SourceAgentIdHandler};

impl SubAgentTool {
    pub(crate) async fn invoke_fork(
        &self, prompt: String, cwd: String, subagent_type: Option<String>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // ... 原 L270-448
    }
}
```

- 纯移动，无逻辑变更
- 依赖：`build_subagent_middlewares`, `SubAgentMiddlewareConfig`, `AgentState`, `ReActAgent`

#### Step 2: `execute_bg.rs` ← `invoke_background()` + `invoke_background_fork()`

```rust
// subagent/tool/execute_bg.rs
use super::define::SubAgentTool;
use super::{build_subagent_middlewares, fire_subagent_lifecycle_hooks_static, format_subagent_result};

impl SubAgentTool {
    pub(crate) async fn invoke_background(
        &self, prompt: String, agent_id: String, cwd: String,
        task_id: String, registry: &Arc<BackgroundTaskRegistry>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> { ... }

    pub(crate) async fn invoke_background_fork(
        &self, prompt: String, cwd: String,
        task_id: String, registry: &Arc<BackgroundTaskRegistry>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> { ... }
}
```

- `invoke_background` 和 `invoke_background_fork` 共享大量模式（child thread 创建、register_runtime、hooks、bg_event_sender）
- **可选优化**：提取公共的 `spawn_background_task()` 辅助函数消除 ~80 行重复代码
- ⚠️ 但这是逻辑变更，非纯移动。建议第一步先纯移动，第二步再 DRY

#### Step 3: `define.rs` 缩减

保留：
- `DeregisterGuard`（13行）
- `AGENT_DESCRIPTION`（32行）
- `SubAgentTool` struct（37行）
- 所有 builder 方法（~70行）
- `load_agent_def()` / `overrides_from_agent_def()` / `fire_subagent_lifecycle_hook()` / `filter_tools()`（~55行）
- `impl BaseTool for SubAgentTool`（~250行）

移除：三个 execute 方法 → 各自子模块

**预估结果**：define.rs 从 1242 → ~450 行，三个子模块各 ~200-500 行。

---

## 2. `keyboard.rs`（1222 行）

### 当前结构

```
 11- 99  macOS KeyBinding layer       (~89行, 5 static SHORTCUT_*)
102-113  cycle_*_label()               (~12行)
114-1061 handle_key_event()            (~948行!)
         ├ 118-121    Release filter
         ├ 123-148    Bar focus / focused-only mode
         ├ 150-158    BackTab (permission cycle)
         ├ 160-171    Ctrl+B (bg bar)
         ├ 173-290    模型/Provider 切换 (Ctrl+T/Ctrl+Shift+T/Alt+M)
         ├ 292-361    Setup wizard
         ├ 363-446    PanelManager dispatch
         ├ 448-452    OAuth prompt
         ├ 454-507    AskUser popup
         ├ 509-560+   HITL popup
         ├ 560-700+   通用输入编辑 (textarea/history/command)
         └ 700+       Enter/Submit 逻辑
1062-1108 update_at_mention_detection() (~47行)
1109-1163 inject_at_mention_path()      (~55行)
1164-1210 handle_bar_key_event()        (~47行)
1165-1222 cleanup
```

### 问题分析

`handle_key_event()` 是典型的"开关森林"（switch-in-a-function pattern）。每个 if-block 处理一种交互模式，但全部塞在一个函数里。

**关键约束**：
- 所有 handler 共享 `&mut App` 借用
- 每个 if-block 命中后 `return Ok(Some(Action::Redraw))`，未命中则 fallthrough
- 执行顺序有依赖（focus mode > shortcuts > setup wizard > panels > popups > textarea）

### 方案：按优先级分层，每层独立文件

```
event/
├── mod.rs                 → 已有
├── keyboard.rs            → 缩减至 ~150 行（KeyBinding + 入口分发）
├── keyboard/
│   ├── mod.rs             → re-export
│   ├── shortcuts.rs       → model/provider 切换 + Ctrl+B/BackTab (~130行)
│   ├── setup_wizard.rs    → Setup wizard 拦截 (~70行)
│   ├── panels.rs          → PanelManager dispatch (~85行)
│   ├── popups.rs          → OAuth / AskUser / HITL 弹窗 (~150行)
│   ├── textarea.rs        → 通用输入编辑逻辑 (~200行)
│   └── submit.rs          → Enter/Submit + @mention 注入 (~100行)
```

**步骤：**

#### Step 1: 提取 `shortcuts.rs`

```rust
// event/keyboard/shortcuts.rs
use super::super::KeyBinding;  // 或移到 keyboard.rs 顶层
use crate::app::App;

pub(super) fn handle_shortcuts(app: &mut App, key_event: &KeyEvent) -> Option<Action> {
    // Shift+Tab → permission cycle
    // Ctrl+B → bg bar
    // Ctrl+T / Alt+M → model cycle
    // Ctrl+Shift+T / Alt+Shift+M → provider cycle
    // 命中返回 Some(Action)，未命中返回 None
}
```

`handle_key_event()` 改为：

```rust
if let Some(action) = keyboard::shortcuts::handle_shortcuts(app, &key_event) {
    return Ok(Some(action));
}
```

#### Step 2: 提取 `setup_wizard.rs`、`panels.rs`、`popups.rs`

同样模式——每个模块暴露 `pub(super) fn handle_xxx(app: &mut App, input: Input) -> Option<Action>`。

#### Step 3: 提取 `textarea.rs` → 通用编辑逻辑

这是最复杂的部分。将 non-popup、non-panel 模式下所有 textarea 编辑逻辑（history 上下翻页、Ctrl+U/Ctrl+D、Enter 提交判断、`/` 命令前缀）移入独立模块。

**注意**：这部分 fallthrough 到最后的 Submit 逻辑，不能用 `Option<Action>` 模式（因为 textarea 编辑后不是 always return）。需要用 `Result<ControlFlow, Action>` 或拆分两个阶段：（1）编辑处理（2）提交判断。

#### Step 4: `keyboard.rs` 缩减为入口 + KeyBinding

```rust
// event/keyboard.rs (~150行)
pub fn handle_key_event(app: &mut App, key_event: KeyEvent) -> Result<Option<Action>> {
    if key_event.kind == KeyEventKind::Release { return Ok(Some(Action::Redraw)); }

    // Bar focus mode
    if let Some(action) = handle_bar_focus(app, &key_event) { return Ok(Some(action)); }
    // Focused-only mode
    if let Some(action) = handle_focused_mode(app, &key_event) { return Ok(Some(action)); }
    // Shortcuts
    if let Some(action) = keyboard::shortcuts::handle(app, &key_event) { return Ok(Some(action)); }

    let input = Input::from(key_event);
    // Setup wizard
    if let Some(action) = keyboard::setup_wizard::handle(app, input.clone()) { return Ok(Some(action)); }
    // Panels
    if let Some(action) = keyboard::panels::handle(app, input.clone()) { return Ok(Some(action)); }
    // Popups (OAuth > AskUser > HITL — 优先级链)
    if let Some(action) = keyboard::popups::handle(app, input.clone()) { return Ok(Some(action)); }
    // Textarea + submit
    keyboard::textarea::handle(app, input)
}
```

---

## 3. 执行顺序

| 优先级 | 文件 | 难度 | 风险 | 预估收益 |
|--------|------|------|------|---------|
| 1 | `define.rs` Step 1: 拆出 `execute_fork.rs` | 低 | 低 | -179行 |
| 2 | `define.rs` Step 2: 拆出 `execute_bg.rs` | 中 | 中 | -490行 |
| 3 | `keyboard.rs` Step 1: 拆出 `shortcuts.rs` | 低 | 低 | -130行 |
| 4 | `keyboard.rs` Step 2: 拆出 `panels.rs` + `popups.rs` | 低 | 低 | -230行 |
| 5 | `keyboard.rs` Step 3: 拆出 `textarea.rs` | 高 | 高 | -200行 |

**建议先做 define.rs 的全部（纯移动，无逻辑变更），再用 define.rs 的经验做 keyboard.rs。**

---

## 4. 注意事项

- **define.rs 的 invoke_background + invoke_background_fork 有大量重复代码**（child thread 创建、register_runtime、DeregisterGuard、hooks 触发、bg_event_sender 发送）。拆出来后可以提取 `spawn_and_run_child_agent()` 公共辅助函数，再削 80+ 行。
- **keyboard.rs 的 popup 优先级链**：OAuth > AskUser > HITL，且 AskUser 和 HITL 共享 `InteractionPrompt` 枚举。拆分时注意保持这个顺序。
- **keyboard.rs 中 textarea 编辑和 Submit 逻辑深度交织**（如 `@` 检测触发 at_mention popup、`/` 触发命令补全、Enter 在 loading 时的行为）。这是最需要仔细重构的部分——建议先写测试覆盖现有行为再动。
- **所有 `with_xxx()` builder 方法可以合并为一个 `SubAgentConfig` struct**，但这是 API 变更，可能需要更新调用方。作为独立的第二步优化。
