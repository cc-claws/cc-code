# 实施计划: 20260512_F001 - subagent-display-colors

## 依赖关系

```
Task 1 (数据模型)
  ├── Task 2 (Pipeline)
  ├── Task 3 (渲染)
  ├── Task 4 (后台完成处理)
  └── Task 5 (持久化恢复)
Task 6 (测试) ← 依赖 Task 1-5 全部完成
```

---

## Task 1: 扩展 SubAgentGroup 数据模型

**文件**: `peri-tui/src/ui/message_view.rs`

**改动**:

1. `SubAgentGroup` 变体新增字段 `is_background: bool` 和 `bg_hash: Option<String>`

2. `subagent_group()` 构造函数新增 `is_background` 参数

3. `Hash` impl 新增 `is_background` 和 `bg_hash` 参与 hash

4. `PartialEq` impl 新增 `is_background` 和 `bg_hash` 参与比较

5. 所有其他构造 `SubAgentGroup` 的位置补上新字段（`is_background: false, bg_hash: None`）：
   - `from_base_message_with_cwd` 中 Agent 工具恢复路径
   - `drain_subagent_stack` 中异常残留路径
   - `message_pipeline.rs` 中所有 `SubAgentGroup` 构造点

**验证**: `cargo build -p peri-tui` 编译通过
- [x] 扩展 SubAgentGroup 数据模型 (编译通过)

---

## Task 2: Pipeline SubAgentState + 事件处理

**文件**: `peri-tui/src/app/message_pipeline.rs`

**改动**:

1. `SubAgentState` 新增 `is_background: bool` 和 `bg_hash: Option<String>` 字段

2. 新增 helper 函数 `parse_bg_hash(result: &str) -> Option<String>`：
   - 从 `"Background task bg-{uuid} started..."` 中提取 task_id 前 8 位
   - 使用 `strip_prefix("Background task bg-")` + `split(' ')` + `chars().take(8)`

3. `SubAgentStart` 处理器：传递 `is_background` 到 `SubAgentState`（当前被 `_` 忽略）

4. `tool_end_internal` 核心变更：
   - 检查 `sub.is_background`
   - 后台路径：不调用 finalize_vm，保持 `is_running=true`；调用 `parse_bg_hash` 设置 `bg_hash`；跳过推入 `frozen_subagent_vms`
   - 前台路径：行为不变

5. 所有 `SubAgentGroup` 构造点补上新字段：
   - `build_tail_vms` 中从 `subagent_stack` 构建的路径
   - `drain_subagent_stack` 中异常残留路径

**验证**: `cargo build -p peri-tui` 编译通过
- [x] Pipeline SubAgentState + 事件处理 (编译通过)

---

## Task 3: 渲染格式变更

**文件**: `peri-tui/src/ui/message_render.rs`

**改动**:

1. SubAgentGroup 分支的颜色逻辑：
   ```rust
   let agent_color = if *is_error {
       theme::ERROR
   } else if *is_running && *is_background {
       theme::WARNING
   } else {
       theme::SAGE
   };
   ```

2. Header 行格式从 `● {agent_id}` 改为：
   ```
   Agent(type) #hash
   ```
   - `"Agent"` — BOLD + agent_color
   - `"(type)"` — MUTED
   - `"#hash"` — MUTED（仅 bg_hash 有值时显示）

3. 折叠和展开状态的 header 统一使用新格式

4. 函数签名需匹配 SubAgentGroup 的新字段（`is_background`, `bg_hash`）

**验证**: `cargo build -p peri-tui` 编译通过
- [x] 渲染格式变更 (编译通过)

---

## Task 4: 后台完成处理变更

**文件**: `peri-tui/src/app/agent_events_bg.rs`

**改动**:

1. `handle_background_task_completed` 新逻辑：
   - 遍历 `view_messages`，找第一个 `SubAgentGroup` 满足：`is_background == true && is_running == true && agent_id == agent_name`
   - 找到时：克隆 VM → 更新 `is_running=false, final_result=Some(output), is_error=!success` → 替换原 VM → `request_rebuild()`
   - 未找到时：回退到创建 ToolBlock `bg:{agent_name}`（兼容现有行为）

2. 移除原有的 ToolBlock 创建逻辑（仅在成功匹配 SubAgentGroup 时）

3. 保留 `agent_state_messages` 通知推送（供 LLM 上下文使用，不变）

4. 保留 continuation 流程（`agent_done_pending_bg` + `pending_bg_continuation`，不变）

**验证**: `cargo build -p peri-tui` 编译通过
- [x] 后台完成处理变更 (编译通过)

---

## Task 5: 持久化恢复路径

**文件**: `peri-tui/src/ui/message_view.rs`

**改动**:

1. `from_base_message_with_cwd` 中 Agent 工具恢复路径：
   - 从 `raw_content` 检测 `"Background task"` 前缀 → 设置 `is_background = true/false`
   - 调用 `parse_bg_hash(&raw_content)` 提取 `bg_hash`（复用 Task 2 的 helper）
   - 注意：`parse_bg_hash` 定义在 `message_pipeline.rs`（私有），需要在 `message_view.rs` 中复制一个同名 helper 或提升为公共函数

2. 考虑将 `parse_bg_hash` 放到 `tool_display.rs` 作为公共函数（两个文件都可引用）

**验证**: `cargo build -p peri-tui` 编译通过
- [x] 持久化恢复路径 (编译通过)

---

## Task 6: 测试更新与验证

**文件**:
- `peri-tui/src/app/message_pipeline_test.rs`
- `peri-tui/src/ui/headless.rs`

**改动**:

1. 更新 `message_pipeline_test.rs` 中所有构造 `SubAgentGroup` 的测试代码，补上新字段

2. 新增测试场景：
   - 前台 SubAgent 的完整生命周期（SubAgentStart → SubAgentEnd → 完成）
   - 后台 SubAgentStart + SubAgentEnd 不冻结（is_running 保持 true）
   - BackgroundTaskCompleted 更新 SubAgentGroup（is_running → false）
   - bg_hash 从 result 字符串正确解析
   - 多同名后台 agent 的 FIFO 匹配

3. 新增 `parse_bg_hash` 单元测试

4. Headless 测试确认渲染不变（如果有的话）

**验证**: `cargo test -p peri-tui` 全部通过
- [x] 测试更新与验证 (全部 393 个测试通过)

**额外修复**: 修改 `in_subagent()` 方法，只检查前台 agent（后台 agent 不阻塞 Done 事件）

**文件**: `peri-tui/src/ui/message_render.rs`

**改动**:

1. SubAgentGroup 分支的颜色逻辑：
   ```rust
   let agent_color = if *is_error {
       theme::ERROR
   } else if *is_running && *is_background {
       theme::WARNING
   } else {
       theme::SAGE
   };
   ```

2. Header 行格式从 `● {agent_id}` 改为：
   ```
   Agent(type) #hash
   ```
   - `"Agent"` — BOLD + agent_color
   - `"(type)"` — MUTED
   - `"#hash"` — MUTED（仅 bg_hash 有值时显示）

3. 折叠和展开状态的 header 统一使用新格式

4. 函数签名需匹配 SubAgentGroup 的新字段（`is_background`, `bg_hash`）

**验证**: `cargo build -p peri-tui` 编译通过

---

## Task 4: 后台完成处理变更

**文件**: `peri-tui/src/app/agent_events_bg.rs`

**改动**:

1. `handle_background_task_completed` 新逻辑：
   - 遍历 `view_messages`，找第一个 `SubAgentGroup` 满足：`is_background == true && is_running == true && agent_id == agent_name`
   - 找到时：克隆 VM → 更新 `is_running=false, final_result=Some(output), is_error=!success` → 替换原 VM → `request_rebuild()`
   - 未找到时：回退到创建 ToolBlock `bg:{agent_name}`（兼容现有行为）

2. 移除原有的 ToolBlock 创建逻辑（仅在成功匹配 SubAgentGroup 时）

3. 保留 `agent_state_messages` 通知推送（供 LLM 上下文使用，不变）

4. 保留 continuation 流程（`agent_done_pending_bg` + `pending_bg_continuation`，不变）

**验证**: `cargo build -p peri-tui` 编译通过

---

## Task 5: 持久化恢复路径

**文件**: `peri-tui/src/ui/message_view.rs`

**改动**:

1. `from_base_message_with_cwd` 中 Agent 工具恢复路径：
   - 从 `raw_content` 检测 `"Background task"` 前缀 → 设置 `is_background = true/false`
   - 调用 `parse_bg_hash(&raw_content)` 提取 `bg_hash`（复用 Task 2 的 helper）
   - 注意：`parse_bg_hash` 定义在 `message_pipeline.rs`（私有），需要在 `message_view.rs` 中复制一个同名 helper 或提升为公共函数

2. 考虑将 `parse_bg_hash` 放到 `tool_display.rs` 作为公共函数（两个文件都可引用）

**验证**: `cargo build -p peri-tui` 编译通过

---

## Task 6: 测试更新与验证

**文件**:
- `peri-tui/src/app/message_pipeline_test.rs`
- `peri-tui/src/ui/headless.rs`

**改动**:

1. 更新 `message_pipeline_test.rs` 中所有构造 `SubAgentGroup` 的测试代码，补上新字段

2. 新增测试场景：
   - 前台 SubAgent 的完整生命周期（SubAgentStart → SubAgentEnd → 完成）
   - 后台 SubAgentStart + SubAgentEnd 不冻结（is_running 保持 true）
   - BackgroundTaskCompleted 更新 SubAgentGroup（is_running → false）
   - bg_hash 从 result 字符串正确解析
   - 多同名后台 agent 的 FIFO 匹配

3. 新增 `parse_bg_hash` 单元测试

4. Headless 测试确认渲染不变（如果有的话）

**验证**: `cargo test -p peri-tui` 全部通过
