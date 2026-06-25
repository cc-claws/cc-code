> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-cache-warning-uses-wrong-hit-rate.md

# 缓存率警告使用错误的命中率计算方式

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-12

## 问题描述

Prompt cache 命中率警告消息使用了累计命中率（`cache_hit_rate()`），而非当次命中率（`last_cache_hit_rate()`），与状态栏显示逻辑不一致。

## 症状详情

| 位置 | 当前实现 | 期望实现 |
|------|---------|---------|
| `agent_ops.rs:156` | `cache_hit_rate()` 累计值 | `last_cache_hit_rate()` 当次值 |
| `status_bar.rs:104` | `last_cache_hit_rate()` 当次值 | ✓ 正确 |

**当前警告消息**：

```
⚠ Prompt cache 累计命中率 74% < 80%
```

**期望警告消息**：

```
⚠ Prompt cache 命中率 74% < 80%
```

## 相关代码

- `peri-tui/src/app/agent_ops.rs:152-173` — 缓存率检查逻辑（使用累计值）

  ```rust
  if let Some(rate) = self.session_mgr.sessions[self.session_mgr.active]
      .agent
      .session_token_tracker
      .cache_hit_rate()  // ← 应改为 last_cache_hit_rate()
  {
      if rate < 0.8 {
          let msg = format!("⚠ Prompt cache 累计命中率 {}% < 80%", percentage);
  ```

- `peri-agent/src/agent/token.rs:57-65` — `cache_hit_rate()` 实现（累计值）
- `peri-agent/src/agent/token.rs:70-78` — `last_cache_hit_rate()` 实现（当次值）

## 根因分析

状态栏正确使用 `last_cache_hit_rate()`（反映最近一次 LLM 调用的缓存效率），但警告消息使用 `cache_hit_rate()`（会话累计值），导致：

1. 语义不一致：状态栏显示当次命中率，警告显示累计命中率
2. 警告时机不准确：累计命中率可能因早期调用较低而持续触发警告，即使当次命中率高

## 期望改进方向

1. 将 `agent_ops.rs:156` 的 `cache_hit_rate()` 改为 `last_cache_hit_rate()`
2. 将警告消息中的"累计命中率"改为"命中率"

## 影响范围

- 缓存率警告消息的显示时机和内容
- 不影响功能，仅影响提示信息的准确性

## 解决说明

经验证，issue 描述有误，代码已处于正确状态：

1. `TokenTracker` 不存在 `last_cache_hit_rate()` 方法，`cache_hit_rate()` 本身就是基于 `last_usage` 的当次命中率（`token.rs:54`，注释："当次调用的缓存命中率"）
2. `agent_ops.rs:156` 和 `status_bar.rs:104` 都使用 `cache_hit_rate()`，逻辑完全一致
3. 警告消息文本已为"命中率"，无"累计"字样
