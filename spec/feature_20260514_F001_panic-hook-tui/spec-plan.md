# Implementation Plan: 20260514_F001 - panic-hook-tui

## 依赖关系

```
Task 1 (PANIC_NOTIFY 全局通道 + install_panic_hook)
  |
  +---> Task 2 (ServiceRegistry 接入)
  |       |
  |       +---> Task 3 (TUI 事件循环消费)
  |
Task 4 (Disconnected 友好提示) -- 独立，可与 Task 1-3 并行
  |
Task 5 (测试) -- 依赖所有前置 Task
```

---

## Task 1: 全局 Panic 通知通道 + 自定义 Panic Hook

**文件**: `peri-tui/src/main.rs`

**变更**:

1. 顶部添加 imports：
   ```rust
   use std::sync::OnceLock;
   use tokio::sync::mpsc;
   ```

2. 在 `Cli` struct 之前添加模块级静态和辅助函数：
   ```rust
   /// 全局 panic 通知通道 sender（OnceLock 保证只初始化一次）
   static PANIC_NOTIFY: OnceLock<mpsc::UnboundedSender<String>> = OnceLock::new();

   /// 格式化 panic 信息为可读字符串（消息 + 位置）
   fn format_panic_message(panic_info: &std::panic::PanicInfo<'_>) -> String {
       let payload = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
           s.to_string()
       } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
           s.clone()
       } else {
           "unknown panic payload".to_string()
       };

       let location = panic_info
           .location()
           .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
           .unwrap_or_else(|| "unknown location".to_string());

       format!("'{}'\n  at {}", payload, location)
   }

   /// 安装自定义 panic hook：
   /// - 通过 tracing::error! 记录到日志文件（不写 stderr）
   /// - 通过 PANIC_NOTIFY 通道通知 TUI
   fn install_panic_hook() {
       std::panic::set_hook(Box::new(|panic_info| {
           let msg = format_panic_message(panic_info);
           tracing::error!("thread panicked at {}", msg);
           if let Some(tx) = PANIC_NOTIFY.get() {
               let _ = tx.send(format!("❌ 后台任务 panic: {}", msg));
           }
       }));
   }
   ```

3. 添加公开初始化函数：
   ```rust
   /// 创建 panic 通知通道并安装自定义 panic hook。
   /// 必须在 enable_raw_mode() 之前调用。
   /// 返回 UnboundedReceiver 供 TUI 消费。
   pub fn init_panic_notify() -> mpsc::UnboundedReceiver<String> {
       let (tx, rx) = mpsc::unbounded_channel();
       let _ = PANIC_NOTIFY.set(tx);
       install_panic_hook();
       rx
   }
   ```

**验证**: `cargo build -p peri-tui` 编译通过。

---

## Task 2: ServiceRegistry 接入 + run_tui 集成

**文件 A**: `peri-tui/src/app/service_registry.rs`

1. 在 `ServiceRegistry` 中添加字段：
   ```rust
   /// panic hook 通知 receiver
   pub panic_notify_rx: Option<tokio::sync::mpsc::UnboundedReceiver<String>>,
   ```

**文件 B**: `peri-tui/src/app/mod.rs`

1. 在 `App::new()` 的 ServiceRegistry 构造中添加：
   ```rust
   panic_notify_rx: None,
   ```

**文件 C**: `peri-tui/src/main.rs`

1. `run_tui()` 中在 `enable_raw_mode()` 之前调用：
   ```rust
   let panic_notify_rx = init_panic_notify();
   ```

2. 传递给 `run_app`：
   ```rust
   let result = run_app(&mut terminal, panic_notify_rx).await;
   ```

3. 更新 `run_app` 签名：
   ```rust
   async fn run_app(
       terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
       panic_notify_rx: mpsc::UnboundedReceiver<String>,
   ) -> Result<()> {
   ```

4. 在 `App::new()` 之后接入：
   ```rust
   app.services.panic_notify_rx = Some(panic_notify_rx);
   ```

**验证**: `cargo build -p peri-tui` 编译通过。

---

## Task 3: TUI 事件循环 Panic 通知消费

**文件 A**: `peri-tui/src/app/agent_ops.rs`

1. 添加 `poll_panic_notifications` 方法（放在 `poll_background_events` 附近）：
   ```rust
   /// 轮询 panic 通知通道，返回是否有新消息。
   pub fn poll_panic_notifications(&mut self) -> bool {
       let rx = match self.services.panic_notify_rx.as_mut() {
           Some(rx) => rx,
           None => return false,
       };
       let mut updated = false;
       loop {
           match rx.try_recv() {
               Ok(msg) => {
                   self.push_system_note(msg);
                   self.request_rebuild();
                   updated = true;
               }
               Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
               Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                   self.services.panic_notify_rx = None;
                   break;
               }
           }
       }
       updated
   }
   ```

**文件 B**: `peri-tui/src/main.rs`

1. 在 `'event_loop` 中 `poll_background_events()` 之后添加：
   ```rust
   let panic_updated = app.poll_panic_notifications();
   ```

2. 更新 render 条件，加入 `panic_updated`。

**渲染行为**: `push_system_note` 添加的 SystemNote 包含 `❌` 前缀，`message_render.rs:559` 的 `is_error` 检测逻辑自动渲染为 `theme::ERROR` 红色，无需修改渲染代码。

**验证**: 手动测试 — 在后台任务中触发 panic，确认 TUI 显示红色通知，终端无原始 stderr 输出。

---

## Task 4: Disconnected 通道友好提示增强

**文件**: `peri-tui/src/app/agent_ops.rs`

1. 在非后台任务的 `Disconnected` 分支中，现有 `apply_pipeline_action` 之后添加：
   ```rust
   self.push_system_note("❌ Agent 执行异常断开，详情请查看日志文件".to_string());
   ```

**注意**: 后台任务的 `Disconnected` 分支已有静默处理逻辑，不添加此消息。

**验证**: 模拟 agent 通道断开，确认 tool_block error 和 system note 同时出现。

---

## Task 5: 测试

### A. 单元测试（`peri-tui/src/main.rs`）

| 测试名 | 场景 |
|--------|------|
| `test_format_panic_message_string_payload` | String payload 格式化正确 |
| `test_format_panic_message_str_payload` | &str payload 格式化正确 |
| `test_format_panic_message_unknown_payload` | 非字符串 payload 返回 "unknown panic payload" |

### B. 手动集成测试

| 场景 | 步骤 | 预期 |
|------|------|------|
| 后台 panic 被捕获 | 在 tokio::spawn 中注入 panic | 终端无 stderr 输出；TUI 显示红色通知；日志文件记录完整信息 |
| ACP 模式不受影响 | `peri acp` 中触发 panic | 默认 panic hook 行为（stderr 正常输出） |
| Agent Disconnected 友好提示 | 中途断开 agent 通道 | tool_block error + system note 引导查看日志 |

---

## 风险与缓解

| 风险 | 缓解措施 |
|------|----------|
| `PANIC_NOTIFY` 在 panic 中并发访问 | `OnceLock::get()` 是 panic-safe；`UnboundedSender::send()` 非阻塞 |
| Receiver 在 App drop 前断开 | `try_recv()` 返回 Disconnected，handler 设 `rx = None` |
| panic hook 安装过晚 | `init_panic_notify()` 是 `run_tui()` 第一个调用，在 `enable_raw_mode()` 之前 |
| panic hook 与 ACP 模式冲突 | `init_panic_notify()` 仅在 `run_tui()` 中调用，ACP 分支不调用 |
| panic hook 中 `tracing::error!` 二次 panic | tracing 的 error! 宏设计为 panic-safe |

## 无新增外部依赖

所有变更使用现有类型：`std::sync::OnceLock`、`tokio::sync::mpsc::unbounded_channel`、`std::panic::set_hook`、`tracing::error!`。

---

## 关键文件清单

| 文件 | 变更类型 |
|------|----------|
| `peri-tui/src/main.rs` | 新增 PANIC_NOTIFY、format_panic_message、install_panic_hook、init_panic_notify；修改 run_tui、run_app |
| `peri-tui/src/app/service_registry.rs` | 新增 panic_notify_rx 字段 |
| `peri-tui/src/app/mod.rs` | ServiceRegistry 构造添加 panic_notify_rx: None |
| `peri-tui/src/app/agent_ops.rs` | 新增 poll_panic_notifications 方法；Disconnected 分支添加友好提示 |
| `peri-tui/src/ui/message_render.rs` | 无需修改（现有 `❌` 检测自动渲染红色） |
