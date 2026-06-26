# input-history.json 全局可读（0o644）

**状态**：Open
**优先级**：中
**创建日期**：2026-06-26
**来源**：cc-code 全项目安全审计 2026-06-26（Finding M4，置信度 8/10，已磁盘验证）

## 问题描述

`save_input_history()` 通过 `std::fs::write(&tmp_path, json)` 写入用户 TUI 提交的每一条原始输入到 `~/.peri/input-history.json`，但未设置任何文件权限。文件落到默认 umask 0o644，**全局可读**。该文件包含用户在 TUI 中提交的原始提示词文本，可能包含内联 API key、调试命令、粘贴的密钥。

## 当前行为

已磁盘验证（2026-06-26）：

```
$ stat -c "%a %n" ~/.peri/input-history.json
644 /home/jackbot/.peri/input-history.json   ← 全局可读
```

```rust
// peri-tui/src/app/history_persistence.rs:32-58
// save_input_history() 用 std::fs::write(&tmp_path, json) 写入，
// 没有 set_permissions 调用
```

对比 `~/.peri/oauth_tokens.json` 已正确实现 0o600（`-rw-------`）。

## 预期行为

| 文件 | 当前权限 | 目标权限 |
|------|---------|---------|
| `~/.peri/input-history.json` | 644 | 600 |
| `~/.peri/`（目录） | 755 | 700 |

## 利用场景

1. 共享开发机 / CI runner 上，受害者用 peri TUI。
2. 用户在 prompt 中直接输入或粘贴 API key、生产数据库连接串、密钥等（常见用法，例如"帮我看下这个 token 为什么不对：sk-..."）。
3. 历史输入被自动持久化到 `~/.peri/input-history.json`（0o644）。
4. 同机另一账户 `cat ~victim/.peri/input-history.json | grep -E 'sk-|password|token'` 即可批量捞走敏感串。

## 修复方案

在 `save_input_history()` 写入后立即应用权限：

```rust
use std::os::unix::fs::PermissionsExt;

// fs::write(&tmp_path, json) 之后
#[cfg(unix)]
{
    let _ = std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600));
}
// rename 之前注意：set_permissions 在 rename 之前应用到 tmp_path，
// rename 会保留 tmp_path 的权限到最终路径
```

参考 `peri-middlewares/src/mcp/auth_store.rs:88-97` 的 `ensure_file` 实现。

Windows 不受影响（ACL 默认按用户隔离）。

## 涉及文件

- `peri-tui/src/app/history_persistence.rs:32-58` — `save_input_history()` 写入逻辑
- `peri-tui/src/app/history_persistence.rs:54` — `fs::write` 调用位置（需要补 `set_permissions`）

## 关联

- 同源问题见 [[2026-06-26-security-sqlite-threads-db-world-readable]]（H3）
- `~/.peri/oauth_tokens.json` 已实现 0o600，可作为参考模式

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-26 | — | Open | agent | 创建（安全审计 M4，已磁盘验证 0o644） |
