# TUI 状态栏模型已切换但实际网关请求仍使用旧模型的问题排查与修复方案

**状态**：Open  
**创建日期**：2026-09-20  
**优先级**：High (P0 - 影响核心功能体验与模型计费/回答效果)  
**标签**：`tui`, `acp`, `model-switch`, `session-lifecycle`

---

## 一、现象描述

在 TUI 界面中通过快捷键（如 `Ctrl+T`）、命令面板（`Ctrl+P`）或 `/model` 面板切换模型后：
1. **TUI 状态栏显示已切换**：左下角状态栏立即高亮显示新模型名称（例如截图中的 `[gemini-3.8-flash-high]`）。
2. **实际请求未切换**：在下游网关（如 9router、OneAPI、反代网关等）的访问日志中，该会话实际向 LLM 发送的请求体中的 `model` 字段依然是切换前的旧模型（例如 `mimo-x` / `mimo-x-pro-preview`）。

---

## 二、复现条件与典型步骤

### 典型复现场景：发送首条消息前切换模型（100% 复现）

1. 启动 TUI（`cargo run -p peri-tui` 或直接启动 `peri`），此时默认加载配置中的激活模型（如 `haiku` 别名对应 `mimo-x-pro-preview`）。
2. 在尚未输入任何消息之前，按下 `Ctrl+T`、使用 `Ctrl+P` 或打开 `/model` 面板，选择另一个模型别名（如 `sonnet` 对应 `gemini-3.8-flash-high`）。
3. 观察 TUI 界面：左下角状态栏正常刷新为 `[gemini-3.8-flash-high]`。
4. 在输入框输入任意测试问题并按回车发送。
5. 检查网关日志：实际接收到的请求模型依然是 `mimo-x-pro-preview`。

---

## 三、涉及核心文件与调用链路

| 文件路径 | 关键函数/位置 | 涉及逻辑 |
|---------|-------------|---------|
| `peri-tui/src/acp_client/client.rs` | `set_config_option` (L362), `update_config` (L379) | 客户端同步配置到 ACP Server 的入口 |
| `peri-tui/src/acp_client/client.rs` | `new_session` (L231), `load_session` (L250) | 客户端向 ACP Server 申请创建/恢复 Session |
| `peri-tui/src/acp_server/requests.rs` | `session/new` (L86), `session/load` (L242) | ACP Server 接收 Session 创建/加载请求 |
| `peri-tui/src/acp_server/requests.rs` | `session/set_config_option` (L188), `apply_model_selection` (L35) | ACP Server 处理配置变更 |
| `peri-tui/src/app/agent_submit.rs` | `submit_message` (L212-L246) | 发送消息时按需创建 Session 并提交 prompt |
| `peri-tui/src/event/keyboard/shortcuts.rs` | `handle_shortcuts` - `Ctrl+T` (L75) | 快捷键切换 model alias 并异步通知 |
| `peri-tui/src/app/panel_model.rs` | `model_panel_confirm` (L80) | `/model` 面板确认并异步通知 |
| `peri-tui/src/app/command_palette_panel.rs` | `apply_selection` (L384) | `Ctrl+P` 命令面板确认并同步通知 |

---

## 四、根本原因深度剖析

### 根因 1：无活跃 Session 时，模型切换请求被 ACP 客户端本地静默丢弃（核心根因）

- **现象原理**：
  TUI 启动时并没有立即向 ACP Server 预先申请创建 Session，只有当用户第一次在输入框按回车提交 prompt 时（`agent_submit.rs:214`），才会触发 `client.new_session()`。因此在用户发送第一条消息之前，`acp_client.current_session_id` 恒为 `None`。
- **代码缺陷点**（`peri-tui/src/acp_client/client.rs` 第 362-366 行 & 第 379-383 行）：
  ```rust
  pub async fn set_config_option(&self, config_id: &str, value: &str) -> Result<(), String> {
      let session_id = match self.current_session_id.lock().unwrap().clone() {
          Some(id) => id,
          None => return Ok(()), // ！！！致命缺陷：无 session 时直接静默退出，未向 ACP Server 发送任何请求
      };
      let params = json!({ "sessionId": session_id, "configId": config_id, "value": value });
      self.transport.send_request("session/set_config_option", params).await...
  }
  ```
  `update_config` 中同样存在一模一样的检查逻辑。
- **后果**：
  用户在刚进入 TUI、发消息前进行模型切换，TUI 前端本地变量 `app.services.model_name` 确实更新了，状态栏也变了；但由于 `current_session_id` 为 `None`，同步给 ACP Server 的网络请求**被本地静默吃掉**，后台 ACP Server 的 `cfg.provider` 依然保留启动时的初始旧模型。

---

### 根因 2：`session/new` 和 `session/load` 完全忽略了客户端传入的 `model` 参数

- **现象原理**：
  当用户输入第一句话按回车时，`submit_message` 执行：
  ```rust
  let model_clone = self.services.model_name.clone();
  client.new_session(&cwd_clone, Some(&model_clone)).await;
  ```
  客户端传了 `model: "gemini-3.8-flash-high"` 参数给 ACP Server。
- **代码缺陷点**（`peri-tui/src/acp_server/requests.rs` 第 86 行 `"session/new"` 分支）：
  ```rust
  "session/new" => {
      let cwd = params.get("cwd")...;
      let meta = ThreadMeta::new(&cwd);
      let thread_id = cfg.thread_store.create_thread(meta).await...;
      // ...
      // ！！！完全没有读取 params.get("model")，完全没有调用 apply_model_selection
      // ！！！直接使用启动时固定在内存的 cfg.provider（旧模型 mimo-x）
      let models = {
          let p = cfg.provider.read();
          let c = cfg.peri_config.read();
          build_model_state(&p, &c)
      };
      // ...
  }
  ```
- **后果**：
  新 Session 创建时，ACP Server 从未应用客户端传入的 `model` 参数，直接沿用最初的 `cfg.provider`。随后 `execute_prompt` 执行时，读取的依然是旧模型。

---

### 根因 3：`Ctrl+T` 和 `/model` 采用 Fire-and-Forget 异步非阻塞通信导致的并发竞态

- **代码缺陷点**（`shortcuts.rs:113`、`panel_model.rs:85`）：
  ```rust
  tokio::spawn(async move {
      let _ = acp.set_config_option("model", &alias).await;
  });
  ```
  使用脱离当前主线程生命周期的 `tokio::spawn` 异步发送，且不检查返回值。若用户切换模型后立即回车发送消息，`submit_message` 发出的 `prompt` 请求可能在 `set_config_option` 之前被 ACP Server 消费，导致当前轮次依然使用旧模型。

---

### 根因 4：裸别名传参无法支持跨 Provider 切换

- 在 `shortcuts.rs` 和 `panel_model.rs` 中，传给 ACP Server 的 `value` 只是裸 alias（如 `"sonnet"`），而不是标准的 `provider_id::alias`。
- 如果用户通过配置切换了不同的 Provider，单纯传递裸 alias 无法定位目标 Provider，导致 `apply_model_selection` 无法正确激活指定 Provider 下的模型。

---

## 五、推荐修复步骤（供开发人员直接参考实施）

### 修复步骤 1：TUI 启动时主动预创建默认 Session（启动即就绪，消除无会话悬空期）

这是解决该问题最自然、最彻底的方案。在 TUI 启动完成且尚未进入事件循环前，由客户端主动向 ACP Server 申请创建初始 Session：

- **实现位置**：`peri-tui/src/main.rs` 中初始化 `AcpTuiClient` 之后（若非 `-c` / `-r` 恢复历史会话）。
- **代码参考**（复用已有的 `new_thread` 通信逻辑）：
  ```rust
  // peri-tui/src/main.rs
  // 在初始化 acp_client 之后：
  if tui_opts.resume_session.is_none() && !tui_opts.continue_session {
      let client = acp_client.clone();
      let cwd = app.services.cwd.clone();
      let model = app.services.model_name.clone();
      tokio::task::block_in_place(|| {
          tokio::runtime::Handle::current().block_on(async {
              match client.new_session(&cwd, Some(&model)).await {
                  Ok(sid) => tracing::info!(session_id = %sid, "启动即就绪: 初始 ACP session 创建成功"),
                  Err(e) => tracing::warn!(error = %e, "启动即就绪: 初始 ACP session 创建失败"),
              }
          })
      });
  }
  ```
- **核心注意事项**：
  1. **禁止前端本地伪造 UUID**：ACP 是 C/S 架构，`sessionId` 对应服务端内存中的 `SessionState`（含 `agent_pool`、`frozen` 系统提示词快照、`cancel_token` 等）。必须通过 `client.new_session()` 让 ACP Server 正式构建，不能仅仅在前端本地赋一个随机 UUID，否则后续请求会导致服务端报 `session not found`。
  2. **空白历史会话（Empty Thread）防堆积**：`session/new` 会在 SQLite `thread_store` 中创建 Thread 记录。如果用户打开 TUI 未发言即退出，可能产生消息数为 0 的空会话。建议：
     - 在会话持久化层（或 `ThreadBrowser` 面板）对 `message_count == 0` 的空会话进行过滤或退出时自动回收。
  3. **简化后续消息发送链路**：启动即建好 Session 后，`submit_message`（`agent_submit.rs`）无需在发首条消息时临时“抢跑”创建 session，发消息可直接变为纯粹的 `client.prompt()`。

---

### 修复步骤 2：解除 ACP Client 对必须存在 `current_session_id` 的拦截（防御性兜底）

即便启动时已创建 session，客户端接口也应具备高容错性。在 `peri-tui/src/acp_client/client.rs` 中：
`set_config_option` 和 `update_config` 修改为**允许在无 Session 时向 Server 发送配置变更**（将 `session_id` 降级为空字符串或缺省）：
```rust
// peri-tui/src/acp_client/client.rs

pub async fn set_config_option(&self, config_id: &str, value: &str) -> Result<(), String> {
    let session_id = self.current_session_id.lock().unwrap().clone().unwrap_or_default();
    let params = json!({ "sessionId": session_id, "configId": config_id, "value": value });
    let _ = self
        .transport
        .send_request("session/set_config_option", params)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn update_config(&self, config: &crate::config::PeriConfig) -> Result<(), String> {
    let session_id = self.current_session_id.lock().unwrap().clone().unwrap_or_default();
    let params = json!({
        "sessionId": session_id,
        "config": config,
    });
    let _ = self
        .transport
        .send_request("session/update_config", params)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
```
*注：ACP Server 端的 `handle_request` 在 `session_id` 为空字符串时已具备兼容性（通过 `extract_session_id(params, "")` 解析，无 session 时仅跳过 `agent_pool.invalidate()`，而 `*cfg.provider.write()` 和 `persist_config` 仍正常生效）。*

---

### 修复步骤 3：在 ACP Server `session/new` 和 `session/load` 中消费 `model` 参数

在 `peri-tui/src/acp_server/requests.rs` 的 `"session/new"` 和 `"session/load"` 分支中，提取并应用客户端传入的 `model` 参数：
```rust
// peri-tui/src/acp_server/requests.rs

"session/new" => {
    // 1. 如果客户端请求中带有 model 参数，优先对齐服务端 provider
    if let Some(model_id) = params.get("model").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        if let Some(new_provider) = apply_model_selection(cfg, model_id) {
            info!(model = %new_provider.model_name(), "session/new: model applied from request");
            *cfg.provider.write() = new_provider;
            persist_config(cfg);
        }
    }
    // ... 其余原有 session/new 逻辑保持不变 ...
}
```
同时在 `apply_model_selection` 中增强匹配能力：如果传入的是具体 `model_name`（如 `"gemini-3.8-flash-high"`）而非别名（`"sonnet"`），支持在当前或全部 providers 的 `models` 中按名称反查出对应的 alias/provider 并切换。

---

### 修复步骤 4：统一模型切换通知链路，消除异步时序竞争

1. 将 `shortcuts.rs`（`Ctrl+T`）与 `panel_model.rs`（`/model` 确认）统一改为类似 `command_palette_panel.rs` 的同步等待模式，或者调用统一的 `ctx.sync_acp_config()` / `sync_model_change()` 逻辑。
2. 切换时携带标准格式 `provider_id::alias`（通过 `format_model_selection_value` 构造），彻底解决跨 Provider 切换与别名重名歧义。

---

## 六、验证 Checklist

- [ ] 启动 TUI，不发任何消息，使用 `Ctrl+T` 切换到另一个模型，状态栏变为新模型后发消息，验证网关抓包/日志显示为新模型。
- [ ] 启动 TUI，不发任何消息，输入 `/model` 打开面板并切换模型回车，发消息验证网关为新模型。
- [ ] 启动 TUI，不发任何消息，使用 `Ctrl+P` 打开 Command Palette 切换不同 Provider 下的模型，发消息验证网关为新模型。
- [ ] 对话多轮后，中途切换模型，紧接着发送下一轮消息，验证网关立即变为新模型且不残留旧模型。
- [ ] 使用 `/clear` 创建新会话后切换模型，验证新会话首轮消息正确使用新模型。
