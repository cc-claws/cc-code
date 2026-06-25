> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-lsp-transport-no-fast-fail-on-process-exit.md

# LSP transport 层错误处理缺陷（进程退出未更新状态 + 崩溃后无自动重连）

**状态**：Fixed + Verify
**优先级**：高
**创建日期**：2026-05-12
**更新日期**：2026-05-12

## 问题描述

LSP 工具在以下两个场景下完全不可用且无法自恢复：

1. **LSP 服务器进程崩溃后，`ServerState` 永远停留在 `Running`**——`run_dispatch_loop` 退出时只调用 `reject_all_pending()` 拒绝 pending 请求，但从未调用 `on_error` 回调，导致 `ServerState` 不更新为 `Error`。后续请求写入已死子进程的 stdin，返回 `Broken pipe`，永久失败。

2. **`LspTool` 无自动重连逻辑**——即使 `LspClient` 已有完整的 `try_restart()` 实现（含 `max_restarts` 限制），`LspTool.get_initialized_server()` 和 `get_any_ready_server()` 在服务器状态异常时未调用重启，直接返回错误。

此问题与之前的超时问题（`--stdio` 参数移除）共享 transport 层的根因——`run_dispatch_loop` 退出路径不完整。

## 症状详情

### 场景 1：并发请求导致 LSP 服务器崩溃

4 个并发 LSP 请求同时发送 → rust-analyzer 崩溃 → 所有后续操作永久返回 `Broken pipe`：

```
LSP 请求失败: IO 错误: Broken pipe (os error 32)
```

### 场景 2：服务器外部崩溃（OOM / 手动 kill 等）

LSP 服务器被外部因素杀死 → `on_error` 回调未触发 → `ServerState` 仍为 `Running` → 后续请求写入已死管道 → `Broken pipe`。

### 场景 3：初始启动参数错误（已有修复）

详见首次 issue 描述——`--stdio` 参数被移除，子进程立即退出，transport 层超时 30s 而非快速失败。

## 根因分析

### Bug 1：`on_error` 回调从未被调用

**位置**：`peri-lsp/src/jsonrpc/transport.rs:296-311`

```rust
// 修复前
pub async fn run_dispatch_loop(state, rx) {
    while let Some(msg) = rx.recv().await {
        state.dispatch(msg);
    }
    state.reject_all_pending("LSP 服务器已断开连接");
    // ❌ 缺少：on_error 回调从未调用
}
```

`DispatchState.on_error` 存储了回调函数（在 `LspClient::do_start()` 中通过 `set_on_error()` 注册），但 `run_dispatch_loop` 从未调用它。结果是：

- `ServerState` 永远停在 `Running`
- `LspClient.is_ready()` 返回 `true`
- `request()` 通过状态检查，写入已死管道 → `Broken pipe`

### Bug 2：`LspTool` 无崩溃重连

**位置**：`peri-middlewares/src/lsp/tool.rs:104-134`

```rust
// 修复前
async fn get_initialized_server(&self, file_path) {
    match self.pool.server_for_file(file_path) {
        Some(s) if s.is_ready() => Ok(s),
        Some(_) => {
            // 直接调 ensure_server_for_file → 因 initialized 集合已有该名称 → 返回 Ok(())
            // 但服务器实际已崩溃 → is_ready() 仍返回 true（Bug 1 导致）→ Broken pipe
        }
        ...
    }
}
```

即使 Bug 1 修复后（状态正确更新为 `Error`），`ensure_server_for_file()` 仍因 `initialized` 集合包含该服务器名而短路返回 `Ok(())`，不会尝试重启。

### Bug 3：`try_restart` 的 `!Send` 问题

**位置**：`peri-lsp/src/client.rs:405-438`

`try_restart()` 内部持有 `parking_lot::MutexGuard`（`!Send`）在 async 函数中，导致整个调用链的 future 不满足 `Send` 约束，编译失败。

```rust
// 修复前
#[allow(clippy::await_holding_lock)]
pub async fn try_restart(&self, root_uri) {
    let mut count = self.restart_count.lock(); // parking_lot::MutexGuard — !Send
    ...
    drop(count);
    // .await 点
}
```

## 修复内容

### 修复 1：transport 层调用 `on_error` 回调

**文件**：`peri-lsp/src/jsonrpc/transport.rs`

| 改动 | 说明 |
|------|------|
| 新增 `DispatchState::invoke_on_error()` | 取出并调用 `on_error` 回调，通知上层更新 `ServerState` |
| `run_dispatch_loop()` 退出时调用 `invoke_on_error(LspError::TransportClosed)` | dispatch loop 退出（stdout EOF / 读取错误）时通知上层 |
| 新增 `LspError::TransportClosed` 错误变体 | 用于 transport 断开时的错误描述 |

### 修复 2：`LspTool` 自动重连

**文件**：`peri-middlewares/src/lsp/tool.rs`

| 改动 | 说明 |
|------|------|
| `get_initialized_server()` 检测 `Error`/`Stopped` 状态 | 服务器异常时调用 `try_restart()` 重连 |
| `get_any_ready_server()` 遍历所有服务器尝试重启 | workspaceSymbol 等全局操作的重连路径 |
| 新增 `LspServerPool::root_uri()` | 暴露工作目录 URI 供重连使用 |
| 新增 `LspServerPool::all_servers()` | 暴露所有服务器实例供重连遍历 |

### 修复 3：`try_restart` 消除 `!Send`

**文件**：`peri-lsp/src/client.rs`

| 改动 | 说明 |
|------|------|
| 提取 `check_and_increment_restart()` 同步方法 | `parking_lot::MutexGuard` 仅存在于同步函数中，不进入 async 状态机 |

### 修复 4：补充错误日志

| 位置 | 日志级别 | 内容 |
|------|---------|------|
| `run_dispatch_loop` 退出时 | `error` | "LSP transport 断开：stdout EOF，拒绝所有 pending 请求" |
| `LspClient::request()` 发送失败时 | `error` | 含 server name、method、error 详情 |
| `LspTool` 服务器状态异常时 | `warn` | 含 server name、state、file_path |
| `LspTool` 重启失败时 | `error` | 含 server name、error 详情 |
| `LspTool` 无匹配服务器时 | `warn` | 含 file_path、extension |

## 修改文件清单

| 文件 | 修改类型 |
|------|---------|
| `peri-lsp/src/error.rs` | 新增 `TransportClosed` 变体 |
| `peri-lsp/src/jsonrpc/transport.rs` | 新增 `invoke_on_error()`、dispatch loop 退出调用、日志 |
| `peri-lsp/src/client.rs` | `check_and_increment_restart()` 提取、`request()` 发送失败日志 |
| `peri-lsp/src/pool.rs` | 新增 `root_uri()`、`all_servers()` |
| `peri-middlewares/src/lsp/tool.rs` | 自动重连逻辑、详细日志 |

## 相关代码

- `peri-lsp/src/jsonrpc/transport.rs` —— 传输层，进程 spawn + 消息分发 + on_error 回调
- `peri-lsp/src/client.rs` —— LspClient，状态管理 + try_restart + request 发送
- `peri-lsp/src/pool.rs` —— LspServerPool，扩展名路由 + 服务器生命周期
- `peri-middlewares/src/lsp/tool.rs` —— LspTool，工具入口 + 自动重连逻辑

## 后续建议

- `startup_timeout` 配置字段已定义（`config.rs:52`）但未传入 `client.start()`，始终硬编码 30s。应将配置透传到 `do_start()` 以支持慢 LSP 服务器的自定义超时。
- 并发请求导致 rust-analyzer 崩溃的问题可能需要请求排队/序列化机制，避免同时发送多个 LSP 请求。
