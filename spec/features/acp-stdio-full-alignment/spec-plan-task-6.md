### Task 6: stdio 新增 session/close + session/clear + session/resume handler

**背景：** 三个简单 handler——业务逻辑均为 1-2 行，直接内联实现，不额外提取到 dispatch 层。

**参考实现：**
- `session/close`: `requests.rs:328-343` — 取消 token + 移除 sessions entry
- `session/clear`: `requests.rs:345-353` — 清空 history
- `session/resume`: `requests.rs:356-387` — 检查并创建 session entry

#### 执行步骤

- [ ] **Step 6.1**: 确认 import

在 `acp_stdio.rs` 的 import 区域（行 240-247）确认已有以下类型，如缺失则添加：
- `CloseSessionRequest`, `CloseSessionResponse`
- `ResumeSessionRequest`, `ResumeSessionResponse`
- `ClearSessionRequest` 或类似的 clear 类型

**检查方法：** 搜索 `agent_client_protocol_schema` 是否有这些类型。

- [ ] **Step 6.2**: 在 `acp_stdio.rs` builder 链中添加 `session/close` handler

在现有 `session/cancel` notification handler（行 631）之后添加：

```rust
// ── session/close ──
.on_receive_request(
    {
        let ctx = ctx_clone.clone();
        async move |req: CloseSessionRequest, responder, _cx: ConnectionTo<Client>| {
            let sid = req.session_id.0.to_string();
            let mut sessions = ctx.sessions.write();
            if let Some(s) = sessions.remove(&sid) {
                if let Some(ref token) = s.cancel_token {
                    token.cancel();
                }
                tracing::info!(session_id = %sid, "Session closed");
            }
            let _ = responder.respond(CloseSessionResponse::new());
            Ok(())
        }
    },
    agent_client_protocol::on_receive_request!(),
)
```

- [ ] **Step 6.3**: 添加 `session/clear` handler

在 close handler 之后添加：

```rust
// ── session/clear ──
.on_receive_request(
    {
        let ctx = ctx_clone.clone();
        async move |req: ClearSessionRequest, responder, _cx: ConnectionTo<Client>| {
            let sid = req.session_id.0.to_string();
            let mut sessions = ctx.sessions.write();
            if let Some(s) = sessions.get_mut(&sid) {
                s.history.clear();
                tracing::info!(session_id = %sid, "Session history cleared");
            }
            let _ = responder.respond(serde_json::json!({ "ok": true }));
            Ok(())
        }
    },
    agent_client_protocol::on_receive_request!(),
)
```

**⚠️ 如果 `agent_client_protocol_schema` 没有 `ClearSessionRequest` 类型：** 使用 `serde_json::Value` 作为请求类型，手动提取 `sessionId`，然后用 `serde_json::to_value(json!({...}))` 返回。如果框架不支持非 schema 类型的 handler，跳过 clear（此方法非 ACP 标准，TUI 专有）。

- [ ] **Step 6.4**: 添加 `session/resume` handler

在 clear handler 之后添加：

```rust
// ── session/resume ──
.on_receive_request(
    {
        let ctx = ctx_clone.clone();
        async move |req: ResumeSessionRequest, responder, _cx: ConnectionTo<Client>| {
            let sid = req.session_id.0.to_string();
            let cwd = req.cwd.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| ".".to_string());
            let mut sessions = ctx.sessions.write();
            if !sessions.contains_key(&sid) {
                sessions.insert(
                    sid.clone(),
                    SessionInfo {
                        session_id: sid.clone(),
                        thread_id: sid.clone(),
                        cwd,
                        history: Vec::new(),
                        cancel_token: None,
                        frozen_system_prompt: None,
                        frozen_claude_md: None,
                        frozen_claude_local_md: None,
                        frozen_skill_summary: None,
                        frozen_date: None,
                    },
                );
                tracing::info!(session_id = %sid, "Session resumed (new)");
            } else {
                tracing::info!(session_id = %sid, "Session resumed (existing)");
            }
            let _ = responder.respond(ResumeSessionResponse::new());
            Ok(())
        }
    },
    agent_client_protocol::on_receive_request!(),
)
```

#### 检查步骤

- [ ] `cargo build -p peri-tui` 编译通过
- [ ] `cargo clippy -p peri-tui` 通过
- [ ] 确认 handler 位置在 builder 链中与其他 handler 不冲突

---
