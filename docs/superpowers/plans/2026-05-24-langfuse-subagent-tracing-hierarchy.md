# Langfuse SubAgent 层级追踪实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Langfuse 遥测中为同步 SubAgent 调用生成层级正确的 Agent-type observation，使其内部 LLM/工具调用正确挂载到 SubAgent observation 下。

**Architecture:** 不用修改事件系统或 executor 泵——利用现有的 ToolStart/ToolEnd 事件。在 `LangfuseTracer` 中，当检测到 name=="Agent" 的工具调用时，在 `on_tool_start` 中创建 SubAgent observation（parent 指向该 Tool 的 span_id）并压入 `subagent_stack`；在 `on_tool_end` 中弹出栈并更新 SubAgent observation。`current_agent_id()` / `current_tools_context()` 自动将后续子事件路由到正确的 SubAgent 上下文。

**Tech Stack:** Rust, `langfuse-client` OTLP ingestion, `parking_lot::Mutex`, `uuid`, `chrono`

**Scope:** 同步 SubAgent（普通/Fork）。Background SubAgent 另案处理（其事件不经过主事件泵）。

---

## File Structure

| 文件 | 操作 | 职责 |
|------|------|------|
| `peri-acp/src/langfuse/tracer.rs` | Modify | 在 `on_tool_start`/`on_tool_end` 中增加 Agent 工具检测 + SubAgent observation 创建/完成逻辑；移除死代码 `on_subagent_start`/`on_subagent_end`，替换为私有 helper |
| `peri-acp/src/langfuse/tracer_test.rs` | Create | Tracer 单元测试：验证 subagent_stack 状态转换及层级路由 |

**不修改的文件**：`executor.rs`（事件泵）、`events.rs`（AgentEvent）、`define.rs`（SubAgentTool）——当前事件流已经携带足够信息。

---

### Task 1: 修改 `on_tool_start` —— Agent 工具调用时创建 SubAgent observation

**Files:**
- Modify: `peri-acp/src/langfuse/tracer.rs:303-326`

- [ ] **Step 1: 添加 `begin_subagent` 私有 helper 方法**

在 `impl LangfuseTracer` 块中，`flush_tools_batch` 方法之前添加：

```rust
/// 从 Agent 工具的输入 JSON 中提取 subagent 标识（用于 Langfuse 显示名称）
fn subagent_identity(input: &serde_json::Value) -> String {
    input
        .get("subagent_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            input
                .get("fork")
                .and_then(|v| v.as_bool())
                .filter(|&f| f)
                .map(|_| "fork".to_string())
        })
        .unwrap_or_else(|| "fork".to_string())
}

/// 创建 SubAgent observation 并将上下文压入 subagent_stack
///
/// parent_observation_id 应为 Agent 工具调用的 Tool span_id，
/// 这样 SubAgent 作为 Tool 的子节点出现在 Langfuse 树中。
fn begin_subagent(&mut self, parent_tool_span_id: &str, input: &serde_json::Value) {
    let agent_id = Self::subagent_identity(input);
    let task_preview: String = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .map(|s| s.chars().take(200).collect())
        .unwrap_or_default();

    let observation_id = uuid::Uuid::now_v7().to_string();
    let start_time =
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let body = ObservationBody {
        id: Some(observation_id.clone()),
        trace_id: Some(self.trace_id.clone()),
        r#type: ObservationType::Agent,
        name: Some(format!("subagent:{}", agent_id)),
        start_time: Some(start_time.clone()),
        parent_observation_id: Some(parent_tool_span_id.to_string()),
        input: Some(serde_json::json!(task_preview)),
        version: Some(VERSION.to_string()),
        session_id: Some(self.session_id.clone()),
        ..Default::default()
    };
    let event = IngestionEvent::ObservationCreate {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: start_time,
        body,
        metadata: None,
    };
    if let Err(e) = self.session.batcher.try_add(event) {
        tracing::warn!(
            error = %e, trace_id = %self.trace_id, subagent = %agent_id,
            "langfuse: subagent observation create 入队失败（背压丢弃）"
        );
    }

    self.subagent_stack.push(SubAgentContext {
        observation_id,
        agent_id,
        tools_batch_span_id: None,
        tools_batch_start_time: None,
        tools_batch_end_time: None,
        pending_tools: HashMap::new(),
    });
}
```

- [ ] **Step 2: 修改 `on_tool_start` 在创建 PendingTool 后调用 `begin_subagent`**

用 block 分隔可变借用，确保 `current_tools_context()` 返回的引用在调用 `begin_subagent` 前已释放。

替换 `on_tool_start` 方法体（第 304-326 行）：

```rust
/// 工具调用开始
pub fn on_tool_start(&mut self, tool_call_id: &str, name: &str, input: &serde_json::Value) {
    let is_agent = name == "Agent";
    let tool_span_id;

    // Block 限定 current_tools_context 的可变借用范围
    {
        let current_agent_id = self.current_agent_id();
        let (batch_id_ref, start_time_ref, _, pending_tools) = self.current_tools_context();
        if pending_tools.is_empty() {
            *batch_id_ref = Some(uuid::Uuid::now_v7().to_string());
            *start_time_ref =
                Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        }
        let parent_span_id = batch_id_ref.clone().unwrap_or(current_agent_id);

        tool_span_id = uuid::Uuid::now_v7().to_string();
        let start_time =
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        pending_tools.insert(
            tool_call_id.to_string(),
            PendingTool {
                span_id: tool_span_id.clone(),
                name: name.to_string(),
                input: input.clone(),
                start_time,
                parent_span_id,
            },
        );
    } // 可变借用在此释放

    // Agent 工具：创建 SubAgent observation，push 到栈
    if is_agent {
        self.begin_subagent(&tool_span_id, input);
    }
}
```

- [ ] **Step 3: 编译检查**

```bash
cargo build -p peri-acp 2>&1 | head -20
```

Expected: 编译成功，无 borrow checker 错误。

---

### Task 2: 修改 `on_tool_end` —— Agent 工具结束时完成 SubAgent observation

**Files:**
- Modify: `peri-acp/src/langfuse/tracer.rs:328-387`

- [ ] **Step 1: 添加 `end_subagent` 私有 helper 方法**

在 `begin_subagent` 之后添加：

```rust
/// 完成当前 SubAgent observation：flush 工具批次，弹出栈，发送 ObservationUpdate
///
/// 必须在 `subagent_stack.pop()` 之前调用 `flush_tools_batch()`，
/// 否则 subagent 的工具批次会 flush 到错误的 parent。
fn end_subagent(&mut self, result: &str, is_error: bool) {
    // flush subagent 下的 tools batch（pop 前）
    self.flush_tools_batch();

    let (subagent_id, subagent_name) = match self.subagent_stack.pop() {
        Some(ctx) => (ctx.observation_id, ctx.agent_id),
        None => {
            tracing::warn!(
                "langfuse: end_subagent 调用时 subagent_stack 为空，忽略"
            );
            return;
        }
    };

    let end_time =
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let status_message = if is_error {
        Some("error".to_string())
    } else {
        None
    };

    let obs_body = ObservationBody {
        id: Some(subagent_id.clone()),
        trace_id: Some(self.trace_id.clone()),
        r#type: ObservationType::Agent,
        name: Some(format!("subagent:{}", subagent_name)),
        output: Some(serde_json::json!(result)),
        end_time: Some(end_time.clone()),
        status_message,
        version: Some(VERSION.to_string()),
        ..Default::default()
    };
    let obs_event = IngestionEvent::ObservationUpdate {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: end_time,
        body: obs_body,
        metadata: None,
    };
    if let Err(e) = self.session.batcher.try_add(obs_event) {
        tracing::warn!(
            error = %e, trace_id = %self.trace_id, subagent = %subagent_name,
            "langfuse: subagent observation update 入队失败（背压丢弃）"
        );
    }
}
```

- [ ] **Step 2: 修改 `on_tool_end` 在 Tool observation 创建后调用 `end_subagent`**

在现有 `on_tool_end` 中，`let tool_name = tool.name.clone();` 之后立即捕获 `is_agent`，然后在 `let _ = end_time_ref; let _ = pending_tools;` 之后、batcher.try_add 之前插入 subagent 收尾逻辑。

替换 `on_tool_end` 方法体（第 329-387 行）：

```rust
/// 工具调用结束：同步创建 tool observation
pub fn on_tool_end(&mut self, tool_call_id: &str, output: &str, is_error: bool) {
    let session_id = self.session_id.clone();
    let trace_id = self.trace_id.clone();
    let trace_id_for_log = self.trace_id.clone();
    let output_owned = output.to_string();

    let (_, _, end_time_ref, pending_tools) = self.current_tools_context();
    let Some(tool) = pending_tools.remove(tool_call_id) else {
        return;
    };
    let end_time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let is_agent = tool.name == "Agent";
    let tool_name = tool.name.clone();
    let span_id = tool.span_id;
    let tool_name_for_body = tool.name.clone();
    let tool_input = tool.input;
    let tool_start_time = tool.start_time;
    let tool_parent_id = tool.parent_span_id;

    let status_msg = if is_error {
        Some("error".to_string())
    } else {
        None
    };

    let body = ObservationBody {
        id: Some(span_id),
        trace_id: Some(trace_id),
        r#type: ObservationType::Tool,
        name: Some(tool_name_for_body),
        input: Some(tool_input),
        output: Some(serde_json::json!(output)),
        start_time: Some(tool_start_time),
        end_time: Some(end_time.clone()),
        completion_start_time: None,
        parent_observation_id: Some(tool_parent_id),
        metadata: None,
        model: None,
        model_parameters: None,
        level: None,
        status_message: status_msg,
        version: Some(VERSION.to_string()),
        environment: None,
        session_id: Some(session_id),
    };
    let event = IngestionEvent::ObservationCreate {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: end_time.clone(),
        body,
        metadata: None,
    };
    // 释放 current_tools_context 的可变借用
    let _ = end_time_ref;
    let _ = pending_tools;

    // Agent 工具：先完成 SubAgent observation（flush + pop + update）
    if is_agent {
        self.end_subagent(&output_owned, is_error);
    }

    if let Err(e) = self.session.batcher.try_add(event) {
        tracing::warn!(
            error = %e, trace_id = %trace_id_for_log, tool = %tool_name,
            "langfuse: tool observation 入队失败（背压丢弃）"
        );
    }

    // 重新获取可变借用
    let (_, _, end_time_ref, _) = self.current_tools_context();
    *end_time_ref = Some(end_time);
}
```

- [ ] **Step 3: 编译检查**

```bash
cargo build -p peri-acp 2>&1 | head -20
```

Expected: 编译成功。

---

### Task 3: 移除死代码（`on_subagent_start` / `on_subagent_end`）

**Files:**
- Modify: `peri-acp/src/langfuse/tracer.rs:431-519`

- [ ] **Step 1: 删除两个 dead public 方法**

删除 `pub fn on_subagent_start(...)` （第 431-476 行）和 `pub fn on_subagent_end(...)` （第 478-519 行）。它们的逻辑已被 `begin_subagent` / `end_subagent` 私有 helper 替代。

- [ ] **Step 2: 编译检查**

```bash
cargo build -p peri-acp 2>&1 | head -20
```

Expected: 编译成功，无 "method not found" 错误（证明这两个方法确实无外部调用者）。

---

### Task 4: 添加单元测试

**Files:**
- Create: `peri-acp/src/langfuse/tracer_test.rs`

- [ ] **Step 1: 创建 test helper —— 构造测试用 Tracer**

在 `tracer_test.rs` 中：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use langfuse_client::{BackpressurePolicy, Batcher, BatcherConfig, LangfuseClient};
    use std::sync::Arc;
    use std::time::Duration;

    /// 构造一个测试用 Tracer，batcher 队列足够大不会触发背压。
    /// Batcher 后台 flush 到无效 URL 会静默失败（仅日志），不影响状态测试。
    fn make_tracer() -> LangfuseTracer {
        let client = LangfuseClient::new("pk-test", "sk-test", "http://127.0.0.1:1", 0);
        let config = BatcherConfig {
            max_events: 1000,
            flush_interval: Duration::from_secs(600),
            backpressure: BackpressurePolicy::DropNew,
            max_retries: 0,
        };
        let batcher = Arc::new(Batcher::new(client, config));
        let session = Arc::new(LangfuseSession {
            client: Arc::new(LangfuseClient::new("pk", "sk", "http://127.0.0.1:1", 0)),
            batcher,
        });
        LangfuseTracer::new(session, "test-session".to_string())
    }

    fn agent_tool_input(subagent_type: &str, prompt: &str) -> serde_json::Value {
        serde_json::json!({
            "subagent_type": subagent_type,
            "prompt": prompt,
            "description": "test task"
        })
    }
```

- [ ] **Step 2: 测试 —— Agent 工具 on_tool_start 创建 SubAgent 并压栈**

```rust
    #[test]
    fn test_agent_tool_start_pushes_subagent_stack() {
        let mut tracer = make_tracer();
        let main_agent_id = tracer.agent_observation_id.clone();

        // Before: stack empty, current = main agent
        assert!(tracer.subagent_stack.is_empty());
        assert_eq!(tracer.current_agent_id(), main_agent_id);

        // Start Agent tool
        let input = agent_tool_input("code-reviewer", "review this code");
        tracer.on_tool_start("tc-1", "Agent", &input);

        // After: stack has 1 entry, current = subagent obs id
        assert_eq!(tracer.subagent_stack.len(), 1);
        let subagent_obs_id = tracer.subagent_stack[0].observation_id.clone();
        assert_ne!(subagent_obs_id, main_agent_id);
        assert_eq!(tracer.current_agent_id(), subagent_obs_id);
        assert_eq!(tracer.subagent_stack[0].agent_id, "code-reviewer");
    }
```

- [ ] **Step 3: 测试 —— 非 Agent 工具不影响 subagent_stack**

```rust
    #[test]
    fn test_non_agent_tool_does_not_push_subagent_stack() {
        let mut tracer = make_tracer();

        let input = serde_json::json!({"file_path": "/tmp/test.rs"});
        tracer.on_tool_start("tc-1", "Read", &input);

        assert!(tracer.subagent_stack.is_empty());
    }
```

- [ ] **Step 4: 测试 —— Agent 工具 on_tool_end 弹出栈**

```rust
    #[test]
    fn test_agent_tool_end_pops_subagent_stack() {
        let mut tracer = make_tracer();

        // Start + end Agent tool
        tracer.on_tool_start("tc-1", "Agent", &agent_tool_input("explorer", "find files"));
        assert_eq!(tracer.subagent_stack.len(), 1);

        tracer.on_tool_end("tc-1", "found 3 files", false);

        assert!(tracer.subagent_stack.is_empty());
    }
```

- [ ] **Step 5: 测试 —— 非 Agent 工具 on_tool_end 不影响栈**

```rust
    #[test]
    fn test_non_agent_tool_end_does_not_pop_subagent_stack() {
        let mut tracer = make_tracer();

        // Push Agent
        tracer.on_tool_start("tc-agent", "Agent", &agent_tool_input("plan", "plan this"));
        assert_eq!(tracer.subagent_stack.len(), 1);

        // End non-Agent tool (concurrent or unrelated)
        let input = serde_json::json!({"pattern": "*.rs"});
        tracer.on_tool_start("tc-glob", "Glob", &input);
        tracer.on_tool_end("tc-glob", "file1.rs", false);

        // Stack should still have the Agent entry
        assert_eq!(tracer.subagent_stack.len(), 1);

        // End Agent
        tracer.on_tool_end("tc-agent", "plan done", false);
        assert!(tracer.subagent_stack.is_empty());
    }
```

- [ ] **Step 6: 测试 —— SubAgent 生命周期中内部事件路由到正确 parent**

```rust
    #[test]
    fn test_subagent_internal_events_use_subagent_context() {
        let mut tracer = make_tracer();
        let main_agent_id = tracer.agent_observation_id.clone();

        // Start Agent tool
        tracer.on_tool_start("tc-1", "Agent", &agent_tool_input("code-reviewer", "review"));
        let subagent_obs_id = tracer.current_agent_id();
        assert_ne!(subagent_obs_id, main_agent_id);

        // SubAgent 内部 LLM 调用：parent 应为 subagent obs
        // (通过检查 current_agent_id 间接验证)
        tracer.on_llm_start(0, &[], &[]);
        assert_eq!(tracer.current_agent_id(), subagent_obs_id);

        // SubAgent 内部工具调用：使用 subagent 的 tools context
        tracer.on_tool_start("tc-inner", "Read", &serde_json::json!({"file_path": "x.rs"}));
        // subagent 有自己的 pending_tools
        assert_eq!(tracer.subagent_stack[0].pending_tools.len(), 1);

        tracer.on_tool_end("tc-inner", "content", false);
        // subagent tools batch end_time 被记录
        assert!(tracer.subagent_stack[0].tools_batch_end_time.is_some());

        // End Agent
        tracer.on_tool_end("tc-1", "review done", false);
        assert!(tracer.subagent_stack.is_empty());
        assert_eq!(tracer.current_agent_id(), main_agent_id);
    }
```

- [ ] **Step 7: 测试 —— 嵌套 SubAgent（subagent 中再调 Agent）**

```rust
    #[test]
    fn test_nested_subagent_stack_depth() {
        let mut tracer = make_tracer();

        // 主 Agent 调用 SubAgent A
        tracer.on_tool_start("tc-a", "Agent", &agent_tool_input("planner", "plan"));
        assert_eq!(tracer.subagent_stack.len(), 1);
        let planner_obs_id = tracer.current_agent_id();

        // SubAgent A 内部调用 SubAgent B
        tracer.on_tool_start("tc-b", "Agent", &agent_tool_input("explorer", "find"));
        assert_eq!(tracer.subagent_stack.len(), 2);
        let explorer_obs_id = tracer.current_agent_id();
        assert_ne!(explorer_obs_id, planner_obs_id);

        // SubAgent B 内部 LLM 调用
        tracer.on_llm_start(0, &[], &[]);
        assert_eq!(tracer.current_agent_id(), explorer_obs_id);

        // SubAgent B 结束
        tracer.on_tool_end("tc-b", "found files", false);
        assert_eq!(tracer.subagent_stack.len(), 1);
        assert_eq!(tracer.current_agent_id(), planner_obs_id);

        // SubAgent A 结束
        tracer.on_tool_end("tc-a", "plan done", false);
        assert!(tracer.subagent_stack.is_empty());
    }
```

- [ ] **Step 8: 测试 —— Fork 类型识别**

```rust
    #[test]
    fn test_fork_subagent_identity() {
        // subagent_type 缺失 + fork=true → identity = "fork"
        let input = serde_json::json!({"prompt": "do something", "fork": true});
        assert_eq!(LangfuseTracer::subagent_identity(&input), "fork");

        // subagent_type 存在 → 优先使用 subagent_type
        let input2 = serde_json::json!({"subagent_type": "code-reviewer", "fork": true, "prompt": "x"});
        assert_eq!(LangfuseTracer::subagent_identity(&input2), "code-reviewer");

        // 两者都缺失 → fallback "fork"
        let input3 = serde_json::json!({"prompt": "x"});
        assert_eq!(LangfuseTracer::subagent_identity(&input3), "fork");
    }
```

- [ ] **Step 9: 运行测试**

```bash
cargo test -p peri-acp --lib -- tracer_test
```

Expected: 8/8 tests pass.

---

### Task 5: 集成验证

- [ ] **Step 1: 运行 peri-acp 全量测试**

```bash
cargo test -p peri-acp
```

Expected: 无 regression。

- [ ] **Step 2: 代码格式与 lint**

```bash
cargo fmt --check -p peri-acp
cargo clippy -p peri-acp -- -D warnings
```

Expected: 通过。

- [ ] **Step 3: 全量 workspace 编译**

```bash
cargo build
```

Expected: 所有 crate 编译成功。

---

## Self-Review

### 1. Spec coverage
- [x] SubAgent 创建 Agent-type observation → Task 1 (`begin_subagent`)
- [x] SubAgent 内部操作挂载到 SubAgent 下 → Task 1 (push stack) + Task 4 Step 6 测试
- [x] SubAgent observation 父级为 Agent Tool → Task 1 (`parent_observation_id: Some(parent_tool_span_id)`)
- [x] 保留 Agent Tool 的 Tool observation → Task 2（Tool observation 创建逻辑不变）

### 2. Placeholder scan
- 无 TBD/TODO
- 所有代码步骤提供完整实现
- 测试步骤包含完整测试代码

### 3. Type consistency
- `subagent_identity` 返回 `String`，在所有调用点一致
- `begin_subagent` / `end_subagent` 签名与调用点一致
- `ObservableBody` 字段使用 `..Default::default()` 覆盖，与现有模式一致
- `SubAgentContext` 字段不变

---

## Known Limitations

1. **Background SubAgent**: `SubAgentTool::invoke_background()` 不设置 event_handler，其内部事件不经过主事件泵，无法被 tracer 捕获。需要后续单独修复（在 background tokio::spawn 中注入 event_handler + 将事件转发到主泵）。
2. **SubagentStarted/SubagentStopped events**: 这些事件仍由 SubAgentTool 发出并泵到 TUI（用于 UI 显示），Tracer 不监听它们——Tracer 通过 ToolStart/ToolEnd 自行管理 subagent_stack。
3. **on_subagent_start/on_subagent_end 删除**: 确认无外部调用者后安全删除（已验证 executor.rs 中 `_ => {}` 丢弃了对应事件）。
