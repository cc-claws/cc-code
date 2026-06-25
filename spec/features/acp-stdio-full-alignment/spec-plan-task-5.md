### Task 5: stdio 新增 session/set_thinking + $/cancel_request handler

**背景：** TUI 路径在 `requests.rs:204-231` 实现了 `session/set_thinking`，在 `notify.rs:24-38` 实现了 `$/cancel_request`。

**⚠️ 框架限制：** `agent_client_protocol` builder 通过 Rust 类型匹配 ACP method 名。`SetSessionThinkingRequest` 不在 `agent_client_protocol_schema` 的已知导出类型中；`$/cancel_request` 由 JSON-RPC 框架层处理（标准取消通知），不需要手动注册 handler。

**执行前确认：** 在 `agent_client_protocol_schema` crate 中搜索 `SetSessionThinking` 和 `set_thinking` 关键字。

#### 执行步骤

- [ ] **Step 5.1**: 搜索 `SetSessionThinking*` 类型是否存在

```bash
grep -rn "SetSessionThinking\|set_thinking" /Users/konghayao/.cargo/registry/src/*/agent-client-protocol-schema-0.12*/src/ 2>/dev/null
```

如果存在 `SetSessionThinkingRequest` / `SetSessionThinkingResponse` 类型，继续 Step 5.2。如果不存在，**跳过 set_thinking**（stdio 路径通过 `session/set_config_option` 的 `thinking_effort` 已覆盖此功能），在 `ACP_COMPATIBLE.csv` 中标注为 "TUI only (equiv: set_config_option thinking_effort)"。

- [ ] **Step 5.2** (条件): 如果类型存在，在 `acp_stdio.rs` builder 链的 `session/set_config_option` handler 之后添加：

```rust
// ── session/set_thinking ──
.on_receive_request(
    {
        let ctx = ctx_clone.clone();
        async move |req: SetSessionThinkingRequest, responder, cx: ConnectionTo<Client>| {
            // ... implementation
        }
    },
    agent_client_protocol::on_receive_request!(),
)
```

**实现逻辑**（参考 `requests.rs:204-231`）：
1. 从 req 获取 effort 和 enabled
2. 调用 `apply_thinking_effort(&ctx.peri_config, effort)`
3. 更新 `ctx.peri_config.write().config.thinking.enabled`
4. 构建 config_options
5. 发送 ConfigOptionUpdate 通知
6. 返回响应

- [ ] **Step 5.3**: 确认 `$/cancel_request` 不需要手动注册

`$/cancel_request` 是 JSON-RPC 2.0 标准的取消通知。`agent_client_protocol` 框架在处理时应该：
- 取消正在进行的请求处理
- 或者至少不会崩溃

**验证方式：** 搜索 `agent_client_protocol` 是否有内置 cancel 支持：
```bash
grep -rn "cancel_request\|CancelRequest\|cancel" /Users/konghayao/.cargo/registry/src/*/agent-client-protocol-0.11*/src/jsonrpc.rs 2>/dev/null | head -10
```

如果框架有内置处理，不再添加 handler。如果框架没有，标记为已知限制（现有 `session/cancel` notification handler 已覆盖场景）。

#### 检查步骤

- [ ] `cargo build -p peri-tui` 编译通过
- [ ] 确认 `session/set_config_option` (configId="thinking_effort") 已作为 set_thinking 的等效替代

---
