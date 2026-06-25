### Task 7: stdio 新增 session/load + session/fork handler

**背景：** 使用 Task 2 和 Task 3 创建的 `dispatch::load_session_messages()` 和 `dispatch::fork_session()` 函数，在 stdio 路径实现 session/load 和 session/fork handler。

**参考实现：** `requests.rs:233-293`（load）、`requests.rs:389-441`（fork）。

#### 执行步骤

- [ ] **Step 7.1**: 确认 import

在 `acp_stdio.rs` 的 import 区域确认已有：
- `LoadSessionRequest`, `LoadSessionResponse`
- `ForkSessionRequest`, `ForkSessionResponse`

这些类型来自 `agent_client_protocol::schema`，应在已有的 `use agent_client_protocol::schema::{...}` 块中添加。

- [ ] **Step 7.2**: 添加 `session/load` handler

在 builder 链中（session/set_config_option 或 session/resume handler 之后）添加：

```rust
// ── session/load ──
.on_receive_request(
    {
        let ctx = ctx_clone.clone();
        async move |req: LoadSessionRequest, responder, cx: ConnectionTo<Client>| {
            let sid = req.session_id.0.to_string();
            let cwd = req.cwd.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| ".".to_string());

            // Load history from ThreadStore via dispatch function
            let history = dispatch::load_session_messages(
                ctx.thread_store.as_ref(),
                &sid,
            ).await;

            // Insert into sessions if not already present
            {
                let mut sessions = ctx.sessions.write();
                if let Some(s) = sessions.get_mut(&sid) {
                    if s.history.is_empty() {
                        s.history = history;
                    }
                } else {
                    sessions.insert(
                        sid.clone(),
                        SessionInfo {
                            session_id: sid.clone(),
                            thread_id: sid.clone(),
                            cwd,
                            history,
                            cancel_token: None,
                            frozen_system_prompt: None,
                            frozen_claude_md: None,
                            frozen_claude_local_md: None,
                            frozen_skill_summary: None,
                            frozen_date: None,
                        },
                    );
                }
            }

            let modes = build_mode_state(&ctx.permission_mode);
            let models = {
                let p = ctx.provider.read();
                let c = ctx.peri_config.read();
                build_model_state(&p, &c)
            };
            let config_options = {
                let c = ctx.peri_config.read();
                let p = ctx.provider.read();
                build_config_options(&c, &p, ctx.permission_mode.load())
            };
            let resp = LoadSessionResponse::new()
                .modes(modes)
                .models(models)
                .config_options(config_options);
            let _ = responder.respond(resp);

            // Send AvailableCommandsUpdate notification
            let skill_dirs = peri_middlewares::SkillsMiddleware::resolve_dirs_static(
                &cwd,
                &ctx.plugin_skill_dirs,
            );
            let skills = peri_middlewares::skills::list_skills(&skill_dirs);
            let cmds = dispatch::build_available_commands(&skills);
            let ac_notif = SessionNotification::new(
                SessionId::new(&*sid),
                SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(cmds)),
            );
            let _ = cx.send_notification(ac_notif);
            Ok(())
        }
    },
    agent_client_protocol::on_receive_request!(),
)
```

- [ ] **Step 7.3**: 添加 `session/fork` handler

在 load handler 之后添加：

```rust
// ── session/fork ──
.on_receive_request(
    {
        let ctx = ctx_clone.clone();
        async move |req: ForkSessionRequest, responder, cx: ConnectionTo<Client>| {
            let source_id = req.session_id.0.to_string();
            let cwd = req.cwd.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| ".".to_string());

            // Get source history
            let source_history = {
                let sessions = ctx.sessions.read();
                sessions.get(&source_id)
                    .map(|s| s.history.clone())
                    .ok_or_else(|| {
                        tracing::warn!(session_id = %source_id, "session/fork: source session not found");
                        Vec::new()
                    })
                    .unwrap_or_default()
            };

            if source_history.is_empty() {
                let _ = responder.respond(ForkSessionResponse::new(SessionId::new("error")));
                return Ok(());
            }

            // Fork via dispatch function
            let (new_thread_id, copied_history) = match dispatch::fork_session(
                ctx.thread_store.as_ref(),
                &source_id,
                &source_history,
                &cwd,
            ).await {
                Ok((id, msgs)) => (id, msgs),
                Err(e) => {
                    tracing::error!(error = %e, "session/fork: fork failed");
                    let _ = responder.respond(ForkSessionResponse::new(SessionId::new("error")));
                    return Ok(());
                }
            };

            // Insert new session
            let new_session_id = new_thread_id.clone();
            {
                let mut sessions = ctx.sessions.write();
                sessions.insert(
                    new_session_id.clone(),
                    SessionInfo {
                        session_id: new_session_id.clone(),
                        thread_id: new_thread_id.clone(),
                        cwd,
                        history: copied_history,
                        cancel_token: None,
                        frozen_system_prompt: None,
                        frozen_claude_md: None,
                        frozen_claude_local_md: None,
                        frozen_skill_summary: None,
                        frozen_date: None,
                    },
                );
            }

            let resp = ForkSessionResponse::new(SessionId::new(new_session_id));
            let _ = responder.respond(resp);
            Ok(())
        }
    },
    agent_client_protocol::on_receive_request!(),
)
```

#### 检查步骤

- [ ] `cargo build -p peri-tui` 编译通过
- [ ] `cargo clippy -p peri-tui` 通过
- [ ] 确认 `dispatch::load_session_messages` 和 `dispatch::fork_session` 正确 import

---
