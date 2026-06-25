> 归档于 2026-05-24，原路径 spec/issues/2026-05-23-continue-flag-not-implemented.md

# -c/--continue 未实现：启动后显示空会话

- **状态**: Fixed
- **优先级**: P2
- **分类**: 功能缺陷
- **影响范围**: CLI 参数 → TUI 启动

## 问题描述

使用 `peri -c` 启动时，期望恢复当前目录最近的对话，但实际显示空会话。`-c` / `--continue` 标志仅打印日志 `"会话恢复功能尚未完全实现"`，未执行任何恢复逻辑。`-r/--resume` 同样受影响。

## 复现步骤

1. 在某目录启动 TUI，进行一轮对话，退出
2. 在同一目录执行 `peri -c`
3. TUI 显示空会话，历史消息未加载

## 预期行为

`-c` 应找到当前目录 `cwd` 下 `updated_at` 最新的 thread，调用已有的 `open_thread()` 恢复完整会话状态（消息、pipeline、ACP session）。

## 根因

`main.rs:485-488` 的实现是空壳：

```rust
// 会话恢复骨架
if tui_opts.continue_session || tui_opts.resume_session.is_some() {
    tracing::info!("会话恢复功能尚未完全实现");
}
```

所有必要的基础设施已存在，但未接线：
- `ThreadStore::list_threads()` — 返回按 `updated_at DESC` 排序的 thread 列表
- `ThreadMeta.cwd` — 可按目录过滤
- `App::open_thread(thread_id)` — 完整的恢复流程（消息加载、pipeline 同步、ACP session 同步、UI 渲染）

## 修复方案

替换 `main.rs:485-488` 为实际恢复逻辑：

```rust
if tui_opts.continue_session {
    let store = app.services.thread_store.clone();
    let cwd = app.services.cwd.clone();
    let thread_id = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let threads = store.list_threads().await.ok()?;
            threads.into_iter()
                .filter(|t| t.cwd == cwd)
                .next()
                .map(|t| t.id)
        })
    });
    if let Some(tid) = thread_id {
        tracing::info!(thread_id = %tid, "-c: 恢复最近会话");
        app.open_thread(tid);
    } else {
        tracing::info!("-c: 当前目录无历史会话，创建新会话");
    }
} else if let Some(session_id) = tui_opts.resume_session {
    // -r <session_id>: 按 ID 恢复指定会话
    tracing::info!(session_id = %session_id, "-r: 恢复指定会话");
    app.open_thread(session_id);
}
```

## 涉及文件

| 文件 | 变更 |
|------|------|
| `peri-tui/src/main.rs:485-488` | 替换空壳为实际恢复逻辑 |
| `peri-tui/src/app/thread_ops.rs:149` | `open_thread()` — 已有，无需修改 |

## 验证方式

1. 创建对话并退出 → `peri -c` → 应显示历史消息且可继续对话
2. 不同目录有不同历史 → `peri -c` → 应恢复当前目录的对话
3. 无历史的新目录 → `peri -c` → 正常创建空会话（无报错）
4. `peri -r <id>` → 恢复指定 ID 的会话
