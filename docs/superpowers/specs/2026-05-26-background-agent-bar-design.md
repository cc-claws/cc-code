# 后台 SubAgent 统一管理栏 — 设计文档

**日期**：2026-05-26
**关联 Issue**：`spec/issues/2026-05-26-background-agent-management-bar.md`

## 概述

�� TUI 状态栏下方新增一个后台 SubAgent 列表栏（Background Agent Bar），让用户能看到正在运行的后台 agent、查看每个 agent 的实时输出、并在它们之间切换。聚焦模式下消息区域只显示选中 agent 的输出，输入框只读锁定并通过边框颜色和标签标识当前 agent。

## 方案选择

采用**方案 A：独立 Bar 组件 + Pipeline 过滤标记**。

| 方案 | 选择理由 |
|------|----------|
| A: 独立 Bar + Pipeline 过滤 | 职责清晰，bar/pipeline/rendering 各管各的；改动集中在 TUI 层 |
| B: 复用 Panel 系统 | Panel 是覆盖中间区域的弹层，不是「状态栏下方固定 bar」 |
| C: 扩展 Status Bar | 水平排列无法显示 task_summary，status bar 已过于拥挤 |

## 1. 数据模型

```rust
// chat_session.rs

struct RunningBgAgent {
    agent_id: String,       // 如 "code-reviewer"
    instance_id: String,    // 调用级唯一 ID
    task_preview: String,   // Agent 工具传入的 task 描述
    started_at: Instant,    // 用于显示耗时
    task_id: Option<String>,// BackgroundTaskCompleted 的匹配键（实现时验证是否与 instance_id 一致）
}
```

**ChatSession 字段变更：**
- 删除 `background_task_count: usize`
- 新增 `background_agents: Vec<RunningBgAgent>`
- 新增 `focused_instance_id: Option<String>`（None = main 视图，Some(id) = 聚焦该 agent）

**颜色分配**：不持久化在 `RunningBgAgent` 中。渲染时根据 agent 在列表中的索引从固定调色板取值：`[Color::Cyan, Color::Magenta, Color::Yellow, Color::Green, ...]` 循环。agent 增减时颜色自然重分配。

**全局影响**：所有引用 `background_task_count` 的代码点改为 `background_agents.len()`。涉及：
- `chat_session.rs` — 字段定义 + 初始化
- `agent_ops/subagent.rs:75` — `handle_subagent_start`
- `agent_events_bg.rs:62` — `handle_background_task_completed`
- `agent_ops/lifecycle.rs:100,329` — Done 时的 pending_bg 检查
- `agent_ops/polling.rs:135-149` — BackgroundTaskCompleted 事件处理
- `ui/main_ui/status_bar.rs:170` — 状态栏渲染
- `panel_ops.rs:101` — session 拆分时的初始化

## 2. UI 布局

修改 `render_session_column()` 的底部约束链，在 `status_bar` 之后新增 `bg_agent_bar`：

```
[sticky_header]
[messages (Min)]
[attachment bar]
[panel_height]
[queued_height]
[input_height]
[status_bar (Length 3)]
[bg_agent_bar]  ← Length(1 + N) 有 agent 时，Length(0) 无 agent 时
```

**高度**：`(1 + background_agents.len()).min(5)` 行。第 1 行固定为 "main"，后续每行一个后台 agent，最多显示 4 个 agent，超出末尾显示 `…+N`。

**每行格式**：`● agent_id  task_preview…  00:12`

- 第 1 行固定显示 `● main`（代表主会话），选中并按 Enter = 回到 main 视图（清空 focused_instance_id）
- 后续行：每个 RunningBgAgent 一行
- 状态点：绿色=运行中，红色=stale（超时无完成事件）
- 选中行反色高亮
- 当前聚焦行（main 或某个 agent）的左侧边框用对应颜色标记（main 用默认主题色）

**Bar 焦点**：
- bar 获得焦点时，输入框视觉变暗（降低对比度），表示不可编辑
- bar 无焦点时仅展示信息，不响应键盘事件

## 3. 键盘交互

**快捷键**：`Ctrl+B` 从输入框跳转到 BgBar。

**焦点状态机**：

```
Input(编辑) ──Ctrl+B──→ BgBar(聚焦, cursor=0)
BgBar ──Esc────→ Input(编辑)
BgBar ──↑/↓───→ 移动选中行
BgBar ──Enter──→ 聚焦选中 agent → Input(只读)
Input(只读) ──Esc──→ 取消聚焦 → Input(编辑)
```

**只读输入框行为**：
- 边框颜色 → 该 agent 的调色板色
- 上边框右侧渲染 `[agent_id]` 标签
- 输入框内文字置灰 + 显示提示 "按 Esc 退出聚焦"
- 所有按键静默消费，仅 Esc 可退出

## 4. 聚焦模式 & 消息过滤

**触发**：BgBar 中 Enter 选中 agent → 设 `focused_instance_id = Some(instance_id)` → 焦点返回输入框（只读锁定）。

**过滤逻辑**：在 `MessagePipeline` 中增加 `should_show_vm()` 过滤器：

```rust
fn should_show_vm(vm: &MessageViewModel, focused_instance_id: Option<&str>) -> bool {
    match focused_instance_id {
        None => true,
        Some(id) => {
            vm.matches_instance_id(id) || !vm.is_subagent_group()
        }
    }
}
```

在 `build_tail_vms()` 和 `messages_to_view_models()` 中调用此过滤器。聚焦模式下：
- 显示该 agent 的 SubAgentGroup（instance_id 匹配）及其包含的 ToolCallGroup
- 显示主 agent 的 Human 消息（上下文锚点）
- 显示 SystemNote 等非 agent 消息
- 隐藏其他 agent 的 SubAgentGroup

**自动退出**：
- `handle_background_task_completed` 中检查完成的是否为当前 `focused_instance_id`
- 是 → 清空 `focused_instance_id`，恢复 main 视图，输入框恢复可编辑
- 否 → 仅从列表移除，不影响聚焦

## 5. 事件流

```
SubAgentStart { agent_id, instance_id, task_preview, is_background: true }
    → handle_subagent_start():
        - 构建 RunningBgAgent push 到 background_agents
        - RebuildAll

BackgroundTaskCompleted { task_id, agent_name, success, ... }
    → handle_background_task_completed():
        - 按 task_id 匹配并移除 background_agents 中的项
        - 若匹配 focused_instance_id → 自动退出聚焦
        - background_agents 为空 → bar 隐藏
        - RebuildAll
```

**task_id 与 instance_id 映射**：实现时验证两者是否为同一标识符。若不一致，`RunningBgAgent` 预留 `task_id: Option<String>` 字段，在 `SubAgentStart` 时暂为 None，在后续事件中回填。

**Bar 显示/隐藏**：不需要 `bar_visible` 标志，直接判断 `background_agents.is_empty()`。

## 6. 边界情况

| 场景 | 处理 |
|------|------|
| Ctrl+B 无后台 agent 时按 | 静默忽略 |
| 分屏多 session | 每个 session 独立维护 background_agents + focused_instance_id |
| compact 触发时处于聚焦模式 | 先退出聚焦再执行 compact |
| 后台 agent 异常退出无完成事件 | 依赖现有 agent_done_pending_bg 超时机制，bar 显示红色 stale 状态点 |
| 多个 agent 同时完成 | 逐个处理，每个检查 focused_instance_id |

## 7. 实现范围（YAGNI）

**不做**：
- 向后台 agent 发送消息
- 后台 agent 历史记录浏览（完成后从列表消失）
- Agent 进度百分比
- Bar 内滚动
- 持久化 bar 状态（session 恢复时列表为空）

## 涉及文件

| 文件 | 改动类型 |
|------|----------|
| `peri-tui/src/app/chat_session.rs` | 数据模型：替换 counter → Vec + focused 字段 |
| `peri-tui/src/app/agent_ops/subagent.rs` | handle_subagent_start 改为 push RunningBgAgent |
| `peri-tui/src/app/agent_events_bg.rs` | handle_background_task_completed 改为移除 + 聚焦检查 |
| `peri-tui/src/app/agent_ops/lifecycle.rs` | background_task_count → background_agents.len() |
| `peri-tui/src/app/agent_ops/polling.rs` | 同上 |
| `peri-tui/src/ui/main_ui/mod.rs` | 布局约束新增 bg_agent_bar |
| `peri-tui/src/ui/main_ui/status_bar.rs` | 状态栏计数 → 移除或简化（bar 已展示详情） |
| `peri-tui/src/ui/main_ui/` 新增 `bg_agent_bar.rs` | Bar 渲染组件 |
| `peri-tui/src/event/keyboard.rs` | Ctrl+B 快捷键注册 + BgBar 键盘处理 |
| `peri-tui/src/app/message_pipeline.rs` | should_show_vm 过滤器 + focused_instance_id 集成 |
| `peri-tui/src/app/panel_ops.rs` | session 拆分时 background_agents 初始化 |
| `peri-tui/src/ui/headless_test.rs` | 现有 background_task_count 测试更新 |
