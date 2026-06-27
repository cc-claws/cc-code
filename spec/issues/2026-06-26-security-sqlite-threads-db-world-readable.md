# SQLite 对话历史数据库 threads.db 全局可读（0o644）

**状态**：Open
**优先级**：高
**创建日期**：2026-06-26
**来源**：cc-code 全项目安全审计 2026-06-26（Finding H3，置信度 9/10，已磁盘验证）

## 问题描述

`SqliteThreadStore::new()` 用 `SqliteConnectOptions::create_if_missing(true)` 创建数据库，但从未调用 `set_permissions(0o600)`，文件落到默认 umask 0o644，**全局可读**。`messages.content` JSON 列保存完整对话历史（人类输入的提示词、AI 回复、工具输出），用户在会话中粘贴的 API key/令牌/源码/PII 全在里面。同机任何本地账户都能直接 `sqlite3 ~victim/.cc-code/threads/threads.db "SELECT content FROM messages"` 转储。

## 当前行为

已磁盘验证（2026-06-26）：

```
$ stat -c "%a %n" ~/.cc-code/threads/threads.db
644 /home/jackbot/.cc-code/threads/threads.db   ← 全局可读

$ stat -c "%a %n" ~/.cc-code/threads/ ~/.cc-code/
755 /home/jackbot/.cc-code/threads/
755 /home/jackbot/.cc-code/
```

```rust
// peri-agent/src/thread/sqlite_store.rs:35-55
// SqliteThreadStore::new() 仅设置 create_if_missing(true)，未应用 0o600
```

对比同项目 `mcp/auth_store.rs:88-97` 已为 `oauth_tokens.json` 正确实现 0o600，磁盘验证 `-rw------- /home/jackbot/.peri/oauth_tokens.json`。

## 预期行为

| 文件 | 当前权限 | 目标权限 |
|------|---------|---------|
| `~/.cc-code/threads/threads.db` | 644 | 600 |
| `~/.cc-code/threads/threads.db-wal` | 644 | 600 |
| `~/.cc-code/threads/threads.db-shm` | 644 | 600 |
| `~/.cc-code/threads/`（目录） | 755 | 700 |
| `~/.cc-code/`（目录） | 755 | 700 |

## 利用场景

1. 共享开发机 / CI runner / 多租户服务器上，受害者用 peri 处理代码 / 调试。
2. 用户在对话中粘贴 API key、源码、生产数据等敏感内容（属常见用法）。
3. 同机另一账户执行 `sqlite3 ~victim/.cc-code/threads/threads.db "SELECT id, content FROM messages WHERE content LIKE '%sk-%'"` 即可捞走全部 Anthropic/OpenAI key。
4. 横向渗透、API 滥用、源码泄露。

## 修复方案

在 `SqliteThreadStore::new()` 创建/打开数据库后立即应用权限：

```rust
use std::os::unix::fs::PermissionsExt;

// 创建/打开数据库后
#[cfg(unix)]
{
    let perms = std::fs::Permissions::from_mode(0o600);
    let _ = std::fs::set_permissions(&db_path, perms);
    // WAL/SHM 同步处理（若启用 WAL 模式）
    for suffix in ["-wal", "-shm"] {
        let p = db_path.with_extension(format!("db{}", suffix));
        if p.exists() {
            let _ = std::fs::set_permissions(&p, perms.clone());
        }
    }
    // 目录级
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
    }
}
```

Windows 不受影响（ACL 默认按用户隔离）。

## 涉及文件

- `peri-agent/src/thread/sqlite_store.rs:35-55` — `SqliteThreadStore::new()` 入口
- `peri-agent/src/thread/sqlite_store.rs:57-65` — `default_path()` 默认路径

## 关联

- 同源问题见 [[2026-06-26-security-input-history-json-world-readable]]（M4）
- `~/.peri/oauth_tokens.json` 已实现 0o600，可作为参考

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-26 | — | Open | agent | 创建（安全审计 H3，已磁盘验证 0o644） |
