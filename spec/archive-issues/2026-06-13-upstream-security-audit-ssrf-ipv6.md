> 归档于 2026-06-24，原路径 spec/issues/2026-06-13-upstream-security-audit-ssrf-ipv6.md

# 上游安全修复审计 — SSRF IPv6 漏洞待 pick

**状态**：Fixed
**优先级**：高（安全）
**创建日期**：2026-06-13

## 问题描述

对上游 `KonghaYao/peri` 最近 20 个核心 crate commit 进行安全审计，发现 1 个未修复的安全漏洞，3 个已同步修复。

## 审计结果

### 需要 pick（1 个）

#### SSRF IPv6 绕过（`80574e51`）— 高优先级

`ssrf_guard.rs` 中 `"::/0"` CIDR 匹配整个 IPv6 地址空间，导致所有公网 IPv6 请求被误拦截。

**上游修复**：移除 `"::/0"`，保留 `fc00::/7`（ULA）+ `fe80::/10`（link-local），`::` 由 `is_unspecified()` 单独处理。

**涉及文件**：
- `peri-middlewares/src/hooks/ssrf_guard.rs` — 移除 `"::/0"` 行
- `peri-middlewares/src/hooks/ssrf_guard_test.rs` — 新增 `test_is_blocked_ipv6_public_allowed`

### 已同步（3 个）

| Commit | 内容 | 我们的修复 |
|--------|------|-----------|
| `1c8ac5f0` | 工具错误 `Ok("Error:")` → `Err()` | 已修复 — `grep.rs`/`edit.rs`/`define.rs`/`execute_bg.rs` 均已改，CLAUDE.md 已文档化 |
| `6aa79e74` | Langfuse flush 阻塞 event pump | 已修复 — `executor.rs:386-395` pump_done 前移 + fire-and-forget + 10s timeout |
| `21b1bdb1` | MCP 插件注入 `CLAUDE_PLUGIN_ROOT/DATA` | 已修复 — `mcp/config.rs:177-200` 已有完整实现 |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启用 hooks SSRF guard
  2. 请求包含公网 IPv6 地址的 URL（如 `http://[2001:4860:4860::8888]`）
  3. 请求被错误拦截
- **环境**：所有 OS

## 涉及文件

- `peri-middlewares/src/hooks/ssrf_guard.rs`（第 98 行）—— `"::/0"` CIDR 误匹配
- `peri-middlewares/src/hooks/ssrf_guard_test.rs`—— 需新增公网 IPv6 放行测试

## 建议修复

cherry-pick 上游 `80574e51` 的 ssrf_guard 部分（10 行改动），或手动应用：

```rust
// 移除这行：
"::/0".parse().unwrap(), // unspecified (we only need to check specific blocked ranges)

// 保留 is_unspecified() 单独检查（已有）
```

新增测试验证 Google/Cloudflare DNS IPv6 不被阻止。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-13 | — | Open | agent | 上游审计发现，待 pick |
| 2026-06-13 | Open | Fixed | agent | pick 上游 80574e51，commit dacfa80f (hotfix/ssrf-ipv6) |
