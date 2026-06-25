# Feature: 20260514_F001 - panic-hook-tui

## 需求背景

TUI 运行时，非渲染线程（tokio runtime worker）发生 panic 时，Rust 默认 panic hook 将 panic 信息输出到 stderr。由于 TUI 占据了整个终端屏幕并启用了 raw mode，stderr 输出直接覆盖在 TUI 画面上，造成显示混乱。

当前项目没有任何 panic 捕获机制：
- 没有 `std::panic::set_hook`
- 没有 `catch_unwind` 包裹 spawn 任务
- panic 信息不会出现在 tracing 日志中（仅走 stderr）

用户无法从 TUI 画面上获取有用的调试信息，也无法通过日志文件回溯 panic。

## 目标

- 阻止 Rust 默认 panic hook 向 stderr 输出（避免破坏 TUI 画面）
- 通过 `tracing::error!` 记录完整的 panic 信息和 stack trace 到日志文件
- 在 TUI 中显示红色错误通知（复用现有 SystemNote 机制）
- 对高风险 spawn 任务加 `catch_unwind` 防止单个任务 panic 导致通道静默断开

## 方案设计

### 1. 自定义 Panic Hook

在 `run_tui()` 函数中，`enable_raw_mode()` 之前安装自定义 panic hook：

**位置**：`peri-tui/src/main.rs` → `run_tui()` 函数开头

**实现**：
```rust
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
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

        // 使用 tracing 记录（写日志文件），不写 stderr
        tracing::error!(
            "💥 thread panicked at '{}'\n  at {}",
            payload,
            location
        );

        // 尝试获取 backtrace（如果 RUST_BACKTRACE=1）
        // tracing::error! 会自动附带 span 信息
    }));
}
```

**关键决策**：
- 安装时机在 `enable_raw_mode()` 之前，确保 TUI 初始化后不会出现 stderr 输出
- 用 `tracing::error!` 替代默认的 stderr 输出，信息进入日志文件
- 不在 hook 中尝试恢复终端或直接操作 UI（hook 运行在 panic 线程上下文中，可能不安全）

### 2. TUI 页面内通知

Panic 信息需要传播到 TUI 显示。由于 panic hook 运行在 panic 线程中，不能直接操作 App 状态。采用**通道通知**机制：

**实现思路**：
- 在 App 层创建一个全局 `mpsc::UnboundedSender<String>`（`panic_notify_tx`），存储在 `ServiceRegistry` 中
- panic hook 通过全局变量（`std::sync::OnceLock`）获取 sender，发送 panic 消息
- TUI 主循环的 event tick 中 poll 这个 receiver，收到消息后调用 `push_system_note()` 显示

**具体结构**：
```rust
// 全局 panic 通知通道（OnceLock 保证只初始化一次）
static PANIC_NOTIFY: std::sync::OnceLock<mpsc::UnboundedSender<String>> = std::sync::OnceLock::new();

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let msg = format_panic_message(panic_info);
        tracing::error!("💥 {}", msg);

        // 尝试通知 TUI
        if let Some(tx) = PANIC_NOTIFY.get() {
            let _ = tx.send(format!("❌ 后台任务 panic: {}", msg));
        }
    }));
}
```

**TUI 侧消费**：
在 `App::handle_event()` 的 tick 分支中，poll panic receiver：

```rust
if let Some(rx) = &mut self.service_registry.panic_notify_rx {
    while let Ok(msg) = rx.try_recv() {
        self.push_system_note(msg);
        self.request_rebuild();
    }
}
```

**渲染效果**：复用现有 SystemNote 渲染逻辑，消息包含 `❌` 前缀，会被 `message_render.rs:559` 的错误检测逻辑自动渲染为红色（`theme::ERROR`）。

### 3. 高风险 Spawn 任务的 catch_unwind

对以下 spawn 位置加 `std::panic::catch_unwind` 包裹：

| 文件 | 位置 | 风险 |
|------|------|------|
| `agent_submit.rs:346` | agent 执行 spawn | 核心路径，unwrap + LLM 调用 |
| `mcp_panel.rs` | MCP 操作 spawn | 外部服务交互 |
| `plugin_panel.rs` | 插件操作 spawn | 外部代码执行 |

**包装模式**：
```rust
tokio::spawn(async move {
    let result = std::panic::AssertUnwindSafe(async {
        agent::run_universal_agent(config).await
    })
    .await;

    match std::panic::catch_unwind(result) {
        // AssertUnwindSafe 本身不 catch async panic
        // 需要用 spawn_blocking + catch_unwind 或在 spawn 外层包
    }
});
```

**修正**：`catch_unwind` 只能捕获同步代码的 panic。对于 `tokio::spawn` 中的 async 代码，需要使用 `tokio::task::spawn_blocking` + `catch_unwind`，或者使用更简单的方式——在 panic hook 中记录，并确保通道断开时 UI 侧有正确的错误显示。

**最终方案**：不在 spawn 任务中加 catch_unwind（async panic 捕获机制复杂且不可靠），而是依赖 panic hook + 通道通知 + 现有的 Disconnected 检测。在 `agent_ops.rs` 的 `Disconnected` 处理中，补充一条更友好的提示：

```rust
Some(Err(mpsc::error::TryRecvError::Disconnected)) => {
    // 现有逻辑
    tracing::error!("Agent channel disconnected unexpectedly");
    // 新增：提示用户查看日志
    self.push_system_note("❌ Agent 执行异常断开，详情请查看日志文件".to_string());
}
```

## 实现要点

1. **安装顺序**：`install_panic_hook()` 必须在 `enable_raw_mode()` 之前调用，确保 TUI 初始化期间也不会有 stderr 输出
2. **OnceLock 生命周期**：`PANIC_NOTIFY` 的 sender 被 App 持有，receiver 被 App 持有。App drop 时 sender 先 drop，receiver 的 `try_recv()` 会返回 `Disconnected`，不会 panic
3. **不恢复终端**：panic hook 中不做终端恢复操作（不调用 `disable_raw_mode`），因为这可能在非主线程执行，状态不安全。终端恢复由现有 `run_tui()` 的 RAII 模式保证
4. **ACP 模式**：ACP 模式不是 TUI，使用默认 panic hook 即可（stderr 输出是正常的）。因此 `install_panic_hook()` 仅在 `run_tui()` 中调用

## 约束一致性

- 符合 `constraints.md` 的日志规范：使用 `tracing`，禁止 `println!`/`eprintln!`
- 符合 `architecture.md` 的事件系统设计：通过 channel 传递 panic 通知，与现有 agent event 通道解耦
- 无新增外部依赖
- 无架构偏离

## 验收标准

- [ ] TUI 运行时，非渲染线程 panic 不会在终端画面上出现原始 panic 输出
- [ ] panic 信息通过 `tracing::error!` 记录到日志文件，包含 panic message、文件名、行号
- [ ] TUI 页面中显示红色错误通知（`❌ 后台任务 panic: ...`）
- [ ] ACP 模式不受影响（使用默认 panic hook）
- [ ] Agent 通道断开时显示友好提示，引导用户查看日志
