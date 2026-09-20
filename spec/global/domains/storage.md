# Storage 领域

## 领域综述

Storage 领域负责 Agent 框架中所有数据持久化相关的基础设施选型与实现。当前聚焦于线程（会话）持久化层的异步数据库访问。

核心职责：
- SQLite 数据库连接池管理（sqlx SqlitePool）
- 线程持久化（threads + messages 两表 Schema）
- 异步数据库操作，消除 sync 桥接模式

边界：不涉及消息内容格式（由 agent 领域管理）、不涉及 TUI 层的会话浏览 UI。

## 核心流程

### 线程持久化流程

```
StateSnapshot 事件触发
  → 过滤 System 消息（不持久化）
  → append_messages 事务写入 SQLite（sqlx::query）
  → WAL 模式保证 crash-safe
  → SqlitePool(max=5) 连接池管理并发
  → 下次 Agent 执行时 load_messages 恢复
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 数据库驱动 | sqlx 0.8（runtime-tokio + sqlite），原生 async，不含 macros/migrate |
| 连接管理 | SqlitePool(max=5)，替代 Arc\<Mutex\<Connection\>\> + spawn_blocking |
| PRAGMA | journal_mode=WAL, synchronous=NORMAL, foreign_keys=ON |
| Schema | threads(id,title,cwd,created_at,updated_at,message_count) + messages(message_id,thread_id,role,content) |
| 事务 | append_messages 使用 pool.begin() + tx.commit() |
| 初始化 | init_schema 拆分为 3 次 sqlx::query().execute()（sqlx 不支持多语句） |
| 调用方影响 | App::new() / new_headless() 变 async，移除 Default impl |

## Feature 附录

### feature_20260504_F001_sqlx-migration
**摘要:** 线程持久化层从 rusqlite 同步迁移到 sqlx 原生异步
**关键决策:**
- sqlx 0.8（runtime-tokio + sqlite）替代 rusqlite + parking_lot
- SqlitePool(max=5) 替代 Arc\<Mutex\<Connection\>\> + spawn_blocking
- Schema 不变（threads + messages 两表）
- 最小 feature：仅 runtime-tokio + sqlite，不含 macros/migrate
- App::new() / new_headless() 变 async，移除 Default impl
- init_schema 拆分为 3 次独立 execute 调用（sqlx 不支持多语句）
**归档:** [链接](../../archive/feature_20260504_F001_sqlx-migration/)
**归档日期:** 2026-05-04

---

## 相关 Feature
- → [agent.md#20260322_F001_agent-storage-refactor](./agent.md#20260322_F001_agent-storage-refactor) — SQLite WAL 持久化替代 JSONL（初始实现）
- → [agent.md#feature_20260326_F006_message-uuid-v7](./agent.md#feature_20260326_F006_message-uuid-v7) — message_id 为主键

---

## Issue 经验附录

### issue_2026-06-01-thread-browser-full-table-scan-high-memory

**摘要:** ThreadBrowser 全量 SQLite 查询导致高内存占用
**状态:** Fixed
**归档日期:** 2026-09-20
**问题本质:** 打开历史对话面板时一次性 select * 加载所有历史会话的所有消息与上下文
**通用模式:** 本地嵌入式存储的元数据与大体积载荷（会话全量消息）必须实行两级加载策略，防止冷数据常驻内存
**技术决策:** 改造为分页查询元数据（ID、标题、时间、首轮摘要），点击具体会话时再按需加载完整消息
**涉及文件:** spec/archive-issues/2026-06-01-thread-browser-full-table-scan-high-memory.md
**CLAUDE.md 链接:** false

### issue_2026-06-26-security-input-history-json-world-readable

**摘要:** input-history.json 全局可读权限漏洞
**状态:** Fixed
**归档日期:** 2026-09-20
**问题本质:** 文件创建时使用了操作系统默认的 0o644 umask，导致同机其他用户可读取敏感输入历史
**通用模式:** 所有持久化用户凭证、提示词历史与敏感配置的文件，在创建时必须硬编码设为 owner-only 权限
**技术决策:** 创建文件时显式设置 0o600 权限（仅 owner 可读写）
**涉及文件:** spec/archive-issues/2026-06-26-security-input-history-json-world-readable.md
**CLAUDE.md 链接:** false

### issue_2026-06-26-security-sqlite-threads-db-world-readable

**摘要:** threads.db 数据库全局可读漏洞
**状态:** Fixed
**归档日期:** 2026-09-20
**问题本质:** SQLite 创建数据库文件时未显式限制文件掩码，默认同机可读
**通用模式:** 数据持久化不仅要关注 SQL 注入，本地文件系统的 DAC（自主访问控制）也是安全防御不可分割的一环
**技术决策:** 在打开/初始化 SQLite 前后确保目录与 DB 文件的权限为 0o600
**涉及文件:** spec/archive-issues/2026-06-26-security-sqlite-threads-db-world-readable.md
**CLAUDE.md 链接:** false
