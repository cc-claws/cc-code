# ThreadBrowser 全量 SQLite 查询导致高内存占用

**状态**：Closed
**优先级**：高
**模块**：agent/tui
**创建日期**：2026-06-01
**关闭日期**：2026-06-24
**修复提交**：`f92a0870` — perf: 修复内存占用问题 — 换 jemalloc、限 worker、history 面板按需加载
**效果**：显著降低内存峰值

## 问题

打开 ThreadBrowser 面板时，`list_threads()` 全量加载所有 thread 的 `cached_context` 列（完整消息历史 JSON，~1MB/线程），导致内存占用飙升。

## 根因

`cached_context` 存储完整消息历史 JSON。`list_threads()` 加载所有线程时，该列被一并查出并反序列化到内存。

## 修复

`f92a0870` 中新增 `THREAD_META_COLUMNS` 常量，将 `cached_context` 替换为 `NULL`：

`peri-agent/src/thread/sqlite_store.rs:20-24`：
```rust
/// SELECT thread 元数据列（不含 cached_context），用于 list_threads 等列表场景。
/// cached_context 包含完整消息历史 JSON，加载所有线程时会占用大量内存（~1MB/线程）。
const THREAD_META_COLUMNS: &str = "t.id, t.title, t.cwd, ... t.config, NULL as cached_context, t.agent_status";
```

`load_context()` 保持按需加载完整数据（单个 thread 恢复时使用）。

## 相关文件

| 文件 | 说明 |
|------|------|
| `peri-agent/src/thread/sqlite_store.rs` | `THREAD_META_COLUMNS` + `list_threads()` |
| `peri-agent/src/thread/types.rs` | `ThreadMeta` 结构体 |
| `peri-tui/src/app/thread_ops.rs` | `open_thread_browser()` |

## 关闭原因

修复 `f92a0870` 已合并超过 23 天，无回归报告。用户未明确验证但生产无反馈，按 issue 健康度整理流程转 Closed（替代长期悬挂的 Fixed 状态）。如复现请转 Reopen。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-01 | — | Open | agent | 创建 |
| 2026-06-01 | Open | Fixed | agent | `f92a0870` 合并 |
| 2026-06-24 | Fixed | Closed | agent | 长期无验证反馈，整理流程关闭 |
