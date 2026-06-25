# HITL 类型安全化 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 HITL 审批和 AskUser 的请求解析/响应构造从手动 JSON 操作替换为 ACP schema 类型安全结构体，消除 camelCase 字段名拼写错误的根因。

**Architecture:** 当前 `hitl_ops.rs` 用 `serde_json::json!()` 宏手动构造 `RequestPermissionResponse`（硬编码 `"outcome"`、`"optionId": "allow_once"` 等字段），`agent_ops.rs` 用 `params.get("toolCall")` 手动解析 `RequestPermissionRequest`。两者都应使用已有的 `agent-client-protocol-schema` 类型。broker 端（`transport_broker.rs`）已经正确使用了这些类型，TUI 端应保持一致。

**Tech Stack:** Rust, agent-client-protocol-schema 0.12, agent-client-protocol 0.11, serde_json

---

## 当前问题

### HITL 响应构造（hitl_ops.rs:104-116）

```rust
// 手动构造 JSON — 字段名拼写错误不会被编译器捕获
let response = if is_approved {
    serde_json::json!({
        "outcome": {
            "outcome": "selected",
            "optionId": "allow_once"
        }
    })
} else {
    serde_json::json!({
        "outcome": {
            "outcome": "cancelled"
        }
    })
};
```

**风险**：`"optionId"` 拼写错误（如 `"option_id"`）会导致 broker 解析失败，但编译不会报错。这正是之前 HITL 显示 "tool id" 和 "null" bug 的同类问题。

### HITL 请求解析（agent_ops.rs:110-118）

```rust
// 手动提取字段 — 与 ACP schema 的 camelCase 序列化规则紧耦合
let tool_call = params.get("toolCall").or_else(|| params.get("tool_call"));
let tool_name = tool_call
    .and_then(|tc| tc.get("title").and_then(|v| v.as_str()))
    .unwrap_or("unknown")
    .to_string();
let tool_input = tool_call
    .and_then(|tc| tc.get("rawInput").or_else(|| tc.get("raw_input")))
    .cloned()
    .unwrap_or(serde_json::Value::Null);
```

**风险**：字段名 `toolCall`、`rawInput` 必须与 `RequestPermissionRequest` 的 `#[serde(rename_all = "camelCase")]` 输出完全一致。`serde` 更新版本或 schema 变更会静默破坏。

### 可用的类型安全 API

`agent-client-protocol-schema` crate（TUI 已依赖）提供：

```rust
// 构造响应
RequestPermissionResponse::new(
    RequestPermissionOutcome::Selected(
        SelectedPermissionOutcome::new("allow_once")
    )
)
RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)

// 解析请求
let req: RequestPermissionRequest = serde_json::from_value(params)?;
// req.tool_call — ToolCallUpdate (含 tool_call_id + #[serde(flatten)] ToolCallUpdateFields)
// req.tool_call.title — Option<String>
// req.tool_call.raw_input — Option<Value>
```

## File Structure

| 文件 | 操作 | 职责变更 |
|------|------|----------|
| `peri-tui/src/app/hitl_ops.rs` | 修改 | 用 `RequestPermissionResponse` 替换 `json!()` 构造 |
| `peri-tui/src/app/agent_ops.rs` | 修改 | 用 `RequestPermissionRequest` 替换手动 JSON 解析 |

---

### Task 1: 类型安全化 HITL 响应构造

**Files:**
- Modify: `peri-tui/src/app/hitl_ops.rs:88-122`

- [ ] **Step 1: 添加 import 并替换 `send_acp_hitl_response` 中的手动 JSON 构造**

在 `hitl_ops.rs` 顶部添加 import（在 `use super::*;` 之后）：

```rust
use agent_client_protocol::schema::{
    RequestPermissionOutcome, RequestPermissionResponse, SelectedPermissionOutcome,
};
```

替换 `send_acp_hitl_response` 方法（第 88-122 行）中的响应构造部分。

当前代码：
```rust
let response = if is_approved {
    serde_json::json!({
        "outcome": {
            "outcome": "selected",
            "optionId": "allow_once"
        }
    })
} else {
    serde_json::json!({
        "outcome": {
            "outcome": "cancelled"
        }
    })
};
tokio::spawn(async move {
    if let Err(e) = acp_client.send_response(request_id, Ok(response)).await {
        tracing::error!(error = %e, "ACP HITL response send failed");
    }
});
```

替换为：
```rust
let response = if is_approved {
    RequestPermissionResponse::new(RequestPermissionOutcome::Selected(
        SelectedPermissionOutcome::new("allow_once"),
    ))
} else {
    RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)
};
let response_value = serde_json::to_value(&response).unwrap_or_else(|e| {
    tracing::error!(error = %e, "Failed to serialize RequestPermissionResponse");
    serde_json::json!({})
});
tokio::spawn(async move {
    if let Err(e) = acp_client.send_response(request_id, Ok(response_value)).await {
        tracing::error!(error = %e, "ACP HITL response send failed");
    }
});
```

注意 `send_response` 签名是 `fn send_response(id: RequestId, result: Result<Value, AcpError>)`，所以需要 `serde_json::to_value` 将类型化响应转为 `Value`。

- [ ] **Step 2: 验证编译通过**

Run: `cargo build -p peri-tui 2>&1 | head -30`
Expected: 无错误

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/hitl_ops.rs
git commit -m "refactor(tui): use typed RequestPermissionResponse for HITL approval"
```

---

### Task 2: 类型安全化 HITL 请求解析

**Files:**
- Modify: `peri-tui/src/app/agent_ops.rs:99-139`

- [ ] **Step 1: 用 `RequestPermissionRequest` 反序列化替换手动字段提取**

在 `agent_ops.rs` 顶部检查是否已有 `agent_client_protocol` import，如果没有则添加：

```rust
use agent_client_protocol::schema::RequestPermissionRequest;
```

替换 `handle_acp_request_permission` 方法（第 100-139 行）。

当前代码：
```rust
fn handle_acp_request_permission(
    &mut self,
    id: RequestId,
    params: serde_json::Value,
) -> (bool, bool, bool) {
    use tokio::sync::oneshot;

    // Manual JSON parsing...
    let tool_call = params.get("toolCall").or_else(|| params.get("tool_call"));
    let tool_name = tool_call
        .and_then(|tc| tc.get("title").and_then(|v| v.as_str()))
        .unwrap_or("unknown")
        .to_string();
    let tool_input = tool_call
        .and_then(|tc| tc.get("rawInput").or_else(|| tc.get("raw_input")))
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let batch_items = vec![BatchItem {
        tool_name,
        input: tool_input,
    }];
    // ... rest unchanged
```

替换为：
```rust
fn handle_acp_request_permission(
    &mut self,
    id: RequestId,
    params: serde_json::Value,
) -> (bool, bool, bool) {
    use tokio::sync::oneshot;

    let req = match serde_json::from_value::<RequestPermissionRequest>(params) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "Failed to parse RequestPermissionRequest");
            return (false, false, false);
        }
    };

    let tool_name = req.tool_call.title.unwrap_or_else(|| "unknown".to_string());
    let tool_input = req.tool_call.raw_input.unwrap_or(serde_json::Value::Null);

    let batch_items = vec![BatchItem {
        tool_name,
        input: tool_input,
    }];

    // Create oneshot bridge — the confirm() handler will call bridge_tx.send(decisions)
    let (bridge_tx, _bridge_rx) = oneshot::channel::<Vec<HitlDecision>>();

    // Store ACP request id for response dispatch in hitl_ops.rs
    self.session_mgr.sessions[self.session_mgr.active]
        .agent
        .pending_acp_request_id = Some(id);

    let prompt = HitlBatchPrompt::new(batch_items, bridge_tx);
    self.session_mgr.sessions[self.session_mgr.active]
        .agent
        .interaction_prompt = Some(InteractionPrompt::Approval(prompt));

    (true, true, false) // pause event consumption, wait for user confirmation
}
```

关键变化：
- `params.get("toolCall").and_then(...)` → `serde_json::from_value::<RequestPermissionRequest>(params)` + `req.tool_call.title`
- `params.get("rawInput").or_else(...)` → `req.tool_call.raw_input`
- camelCase/snake_case 双重 fallback 不再需要——`#[serde(rename_all = "camelCase")]` 自动处理
- 解析失败提前返回而不是 `unwrap_or` 静默降级

- [ ] **Step 2: 验证编译通过**

Run: `cargo build -p peri-tui 2>&1 | head -30`
Expected: 无错误

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/agent_ops.rs
git commit -m "refactor(tui): use typed RequestPermissionRequest for HITL parsing"
```

---

### Task 3: 类型安全化 Elicitation（AskUser）请求解析

**Files:**
- Modify: `peri-tui/src/app/agent_ops.rs:141-214`

- [ ] **Step 1: 用 `CreateElicitationRequest` 反序列化替换手动字段提取**

当前 `handle_acp_elicitation`（第 142-214 行）手动从 JSON 中提取 `mode`、`requestedSchema`、`properties`、`oneOf` 等字段。可以用 `CreateElicitationRequest` 类型反序列化。

在 `agent_ops.rs` 顶部添加 import：
```rust
use agent_client_protocol_schema::CreateElicitationRequest;
```

替换 `handle_acp_elicitation` 中的手动解析部分。

当前代码（第 154-192 行）：
```rust
let mut questions = Vec::new();
let is_form = params.get("mode").and_then(|m| m.as_str()) == Some("form");
if is_form {
    if let Some(schema) = params
        .get("requestedSchema")
        .or_else(|| params.get("requested_schema"))
    {
        if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
            for (prop_id, prop) in props {
                // Manual field extraction...
            }
        }
    }
}
```

替换为：
```rust
let mut questions = Vec::new();

let req = match serde_json::from_value::<CreateElicitationRequest>(params) {
    Ok(r) => r,
    Err(e) => {
        tracing::error!(error = %e, "Failed to parse CreateElicitationRequest");
        return (false, false, false);
    }
};

// CreateElicitationRequest has mode field; we check if it's form mode
// and extract properties from the schema
if let Some(mode) = req.mode.as_form() {
    if let Some(schema) = mode.requested_schema {
        for (prop_id, prop) in &schema.properties {
            let options: Vec<AskUserOption> = prop
                .one_of
                .as_ref()
                .map(|opts| {
                    opts.iter()
                        .map(|o| AskUserOption {
                            label: o.title.clone().unwrap_or_default(),
                            description: None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            questions.push(AskUserQuestionData {
                tool_call_id: prop_id.clone(),
                question: prop.description.clone().unwrap_or_default(),
                header: prop.title.clone().unwrap_or_default(),
                multi_select: false,
                options,
            });
        }
    }
}
```

**注意**：`CreateElicitationRequest` 和 `ElicitationFormMode` 的具体字段访问方式可能因版本不同而变化。实施时需要检查 `agent-client-protocol-schema 0.12` 的实际 API：

```bash
# 检查 ElicitationFormMode 的实际字段结构
grep -A 30 "pub struct ElicitationFormMode\|pub struct ElicitationSchema\|pub enum ElicitationMode" \
  ~/.cargo/registry/src/*/agent-client-protocol-schema-0.12.*/src/*.rs
```

如果 `CreateElicitationRequest` 的类型化 API 不够完善（例如 `mode` 不是 enum 而是 raw string），则保留部分手动解析，但至少用 `serde_json::from_value` 反序列化外层结构体以获得字段重命名保护。

- [ ] **Step 2: 验证编译通过**

Run: `cargo build -p peri-tui 2>&1 | head -30`
Expected: 无错误

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/agent_ops.rs
git commit -m "refactor(tui): use typed CreateElicitationRequest for AskUser parsing"
```

---

### Task 4: 移除 `RequestPermissionRequest` 相关的手动 JSON 字段名注释

**Files:**
- Modify: `peri-tui/src/app/agent_ops.rs`

- [ ] **Step 1: 清理 Task 2 和 Task 3 替换后残留的手动解析注释**

在 `handle_acp_request_permission` 和 `handle_acp_elicitation` 中，移除描述 camelCase 字段名映射的注释（它们不再需要——类型安全 API 自动处理序列化）。

需要移除的注释示例：
```rust
// ACP protocol serializes with camelCase: toolCall, title, rawInput
// ToolCallUpdate: { toolCallId, title (tool name), rawInput, status, content, ... }
// ToolCallUpdateFields is #[serde(flatten)] so all fields are at the same level.
```

```rust
// ACP CreateElicitationRequest serializes as:
//   {"mode": "form", "requestedSchema": {"properties": {...}}, "message": "..."}
// ElicitationMode uses #[serde(tag = "mode", rename_all = "snake_case")]
// StringPropertySchema uses #[serde(rename_all = "camelCase")]: oneOf
```

这些注释在类型安全化后是误导性的——它们暗示代码依赖手动 JSON 字段名映射。

- [ ] **Step 2: 验证编译通过**

Run: `cargo build -p peri-tui 2>&1 | head -30`
Expected: 无错误

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/agent_ops.rs
git commit -m "chore: remove manual JSON field name comments after type-safe refactoring"
```

---

### Task 5: 全量构建和测试

**Files:** 无修改

- [ ] **Step 1: 全量构建**

Run: `cargo build 2>&1 | tail -20`
Expected: 成功

- [ ] **Step 2: 运行相关 crate 测试**

Run: `cargo test -p peri-tui 2>&1 | tail -20`
Expected: 所有测试通过

Run: `cargo test -p peri-acp 2>&1 | tail -20`
Expected: 所有测试通过

- [ ] **Step 3: 运行 pre-commit hooks**

Run: `lefthook run pre-commit 2>&1 | tail -20`
Expected: 全部通过（typos、fmt、check、clippy）

- [ ] **Step 4: 手动集成测试**

启动 TUI，触发 HITL 审批和 AskUser 功能：
1. 发送需要 Bash 工具的消息 → 验证审批弹窗显示正确的工具名和参数
2. 批准操作 → 验证操作正常执行
3. 拒绝操作 → 验证被正确拒绝
4. 测试 Esc 键拒绝

- [ ] **Step 5: 最终 Commit（如有遗漏的修复）**

```bash
git add -A
git commit -m "fix: follow-up fixes from HITL type-safe refactoring"
```

---

## Self-Review

### Spec Coverage
- ✅ HITL 响应构造类型安全化：Task 1
- ✅ HITL 请求解析类型安全化：Task 2
- ✅ Elicitation 请求解析类型安全化：Task 3
- ✅ 过时注释清理：Task 4
- ✅ 全量测试：Task 5

### Placeholder Scan
- Task 3 的 Elicitation 解析需要实施时确认 `CreateElicitationRequest` 的具体 API。已在步骤中注明检查方法（`grep` 命令），这不是 placeholder 而是实施前提条件。
- 所有其他步骤包含完整代码。

### Type Consistency
- `RequestPermissionResponse::new(outcome: RequestPermissionOutcome)` — 与 schema crate 第 682 行一致
- `SelectedPermissionOutcome::new(option_id: impl Into<PermissionOptionId>)` — 与 schema crate 第 738 行一致
- `RequestPermissionRequest` 反序列化后通过 `.tool_call.title` 和 `.tool_call.raw_input` 访问 — 与 `ToolCallUpdate`（`#[serde(flatten)] ToolCallUpdateFields`）的字段结构一致
- `send_response(id: RequestId, result: Result<Value, AcpError>)` — 与 `AcpTuiClient` 第 282 行签名一致
