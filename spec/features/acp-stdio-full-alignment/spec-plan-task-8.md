### Task 8: stdio 新增 session/compact handler

**背景：** TUI 路径在 `compact.rs` 实现 `session/compact`——读取 history，调用 `full_compact()` + `re_inject()`，通过 `EventSink` 推送 `CompactStarted`/`CompactCompleted` 事件，最后更新 session history。

stdio 路径已有 `full_compact()` 和 `re_inject()` 的依赖（`peri_agent::agent::compact`），且 prompt path 已使用 `StdioEventSink`。可直接复用。

**⚠️ Compact 强依赖 provider + config + event_sink，不提取到 dispatch 层。**

#### 执行步骤

- [ ] **Step 8.1**: 确认 import

在 `acp_stdio.rs` 顶部添加 compact 相关的 import：

```rust
use peri_agent::agent::compact::{full_compact, re_inject};
use peri_agent::agent::events::AgentEvent as ExecutorEvent;
```

确认 `StdioEventSink` 已 import（行 251 已有 `use peri_acp::session::event_sink::StdioEventSink;`）。

确认 `CompactRequest` / `CompactResponse` 类型存在于 `agent_client_protocol_schema`。如果不存在，使用 `session/compact` 作为 raw method（参考 TUI 路径的匹配方式），返回值使用 `serde_json::json!({ "success": true })`。

- [ ] **Step 8.2**: 在 builder 链中添加 `session/compact` handler

在 session/fork handler 之后，或者 session/prompt handler 之前添加：

```rust
// ── session/compact ──
.on_receive_request(
    {
        let ctx = ctx_clone.clone();
        async move |req: serde_json::Value, responder, cx: ConnectionTo<Client>| {
            let sid = req.get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Read session data
            let (cwd, history) = {
                let sessions = ctx.sessions.read();
                let s = match sessions.get(&sid) {
                    Some(s) => s,
                    None => {
                        let _ = responder.respond(serde_json::json!({"error": "session not found"}));
                        return Ok(());
                    }
                };
                (s.cwd.clone(), s.history.clone())
            };

            if history.is_empty() {
                let _ = responder.respond(serde_json::json!({"error": "no history to compact"}));
                return Ok(());
            }

            // Get compact config
            let (compact_config, provider_clone) = {
                let cfg = ctx.peri_config.read();
                let p = ctx.provider.read();
                let cc = cfg.config.compact.clone().unwrap_or_default();
                (cc, p.clone())
            };
            let mut effective_config = compact_config.clone();
            effective_config.apply_env_overrides();

            // Get compact model
            let compact_model: std::sync::Arc<dyn peri_agent::llm::BaseModel> =
                provider_clone.into_model().into();

            let event_sink = std::sync::Arc::new(StdioEventSink::new(cx.clone(), SessionId::new(&*sid)));

            // Send CompactStarted
            event_sink.push_event(&sid, &ExecutorEvent::CompactStarted, 0).await;

            // Execute full_compact
            let compact_result = match full_compact(
                &history,
                compact_model.as_ref(),
                &effective_config,
                "",
            ).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(error = %e, "Manual compact: full_compact failed");
                    event_sink.push_event(
                        &sid,
                        &ExecutorEvent::CompactError { message: e.to_string() },
                        0,
                    ).await;
                    let _ = responder.respond(serde_json::json!({"error": format!("compact failed: {e}")}));
                    return Ok(());
                }
            };

            tracing::info!(
                summary_len = compact_result.summary.len(),
                "Manual compact: full_compact completed"
            );

            // Execute re_inject
            let re_inject_result = re_inject(&history, &effective_config, &cwd).await;

            // Build new messages (Claude Code alignment)
            let summary_content = format!(
                "{}\n\n[上下文已压缩，请根据摘要继续工作]",
                compact_result.summary
            );
            let mut new_messages = vec![peri_agent::messages::BaseMessage::human(summary_content)];
            new_messages.extend(re_inject_result.messages.clone());

            // Extract file/skill info (use same helper functions as TUI compact.rs)
            let files = extract_compact_file_info(&re_inject_result.messages);
            let skills = extract_compact_skill_names(&re_inject_result.messages);

            // Send CompactCompleted
            event_sink.push_event(
                &sid,
                &ExecutorEvent::CompactCompleted {
                    summary: compact_result.summary,
                    files,
                    skills,
                    micro_cleared: 0,
                    messages: new_messages.clone(),
                },
                0,
            ).await;

            // Update session history
            {
                let mut sessions = ctx.sessions.write();
                if let Some(s) = sessions.get_mut(&sid) {
                    s.history = new_messages;
                }
            }

            let _ = responder.respond(serde_json::json!({"success": true}));
            Ok(())
        }
    },
    // ⚠️ 如果 agent_client_protocol 不支持 serde_json::Value 类型的 handler，
    // 使用 compact request 专用的类型（如 CompactRequest，若存在）。
    agent_client_protocol::on_receive_request!(),
)
```

- [ ] **Step 8.3**: 在 `acp_stdio.rs` 文件末尾添加 compact 辅助函数（复制自 `compact.rs`）

在 `run_acp_stdio` 函数之前添加两个 helper 函数：

```rust
fn extract_compact_file_info(messages: &[peri_agent::messages::BaseMessage]) -> Vec<peri_agent::agent::events::CompactFileInfo> {
    let mut files = Vec::new();
    for msg in messages {
        let content = msg.content();
        if let Some(rest) = content.strip_prefix("[最近读取的文件: ") {
            let path = rest.lines().next().unwrap_or("");
            let line_count = rest.lines().count().saturating_sub(1);
            if !path.is_empty() {
                files.push(peri_agent::agent::events::CompactFileInfo {
                    path: path.to_string(),
                    lines: line_count,
                });
            }
        }
    }
    files
}

fn extract_compact_skill_names(messages: &[peri_agent::messages::BaseMessage]) -> Vec<String> {
    let mut skills = Vec::new();
    for msg in messages {
        let content = msg.content();
        if let Some(rest) = content.strip_prefix("[激活的 Skill 指令: ") {
            let name = rest.lines().next().unwrap_or("");
            if !name.is_empty() {
                skills.push(name.to_string());
            }
        }
    }
    skills
}
```

#### 检查步骤

- [ ] `cargo build -p peri-tui` 编译通过
- [ ] `cargo clippy -p peri-tui` 通过
- [ ] 确认 compact helper 函数的 import 路径正确（`CompactFileInfo` 来自 `peri_agent::agent::events`）

#### 风险

- `agent_client_protocol` 可能不支持 `serde_json::Value` 作为 handler 类型。如果编译失败，回退到使用 `agent_client_protocol_schema` 中实际存在的 CompactRequest 类型（如 `CompactRequest` 或类似命名）

---
