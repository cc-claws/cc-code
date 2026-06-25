> 归档于 2026-05-24，原路径 spec/issues/2026-05-23-mcp-tools-instability-breaks-anthropic-cache.md

# Deferred Tools 段（MCP 描述）注入 system prompt 动态区域导致跨会话 Cache 部分失效

**状态**：Fixed（bc98baa）
**优先级**：中
**创建日期**：2026-05-23

## 问题描述

`ToolSearchMiddleware` 在 `before_agent` 阶段将 MCP 工具描述注入 system prompt 的 `Deferred Tools` 段。该段位于 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 之后的动态区域，但由于动态区域作为最后一个 system block 也会获得 `cache_control` 断点（`invoke.rs:331-334` 的 `i == last_idx` 逻辑），MCP 连接状态不同 → Deferred Tools 内容不同 → 动态 block 变化 → 第二个 cache breakpoint 失效。

### 数据流

```
ToolSearchMiddleware::before_agent()
  → format_deferred_list()                    # tool_index.rs:284，根据 MCP 连接状态生成描述
  → state.prepend_message(BaseMessage::system(cached))  # middleware.rs:79

build_request_body()                          # invoke.rs:202-222
  → middleware 文本插入到 __SYSTEM_PROMPT_DYNAMIC_BOUNDARY__ 之后
  → split_system_blocks()                     # cache.rs
    → Block 0（静态段）: cache_control = true
    → Block 1（动态段）: cache_control = true（因为 i == last_idx）
```

**预期行为**：静态段（~8K tokens）跨会话稳定，应能命中缓存。动态段因 MCP 描述变化会 miss。
**实际行为**：3 分钟间隔内确实命中静态段（33% = 8K/24K），≥5 分钟全部 0%（TTL 到期）。

## 症状详情

### Diff 证据：30,878 vs 24,640 tokens 的唯一差异

通过 Langfuse API 下载两个会话首次 LLM 调用的完整 input 并 diff：

```bash
curl -s -u "$PK:$SK" "$HOST/api/public/observations/<obs_id>" | jq '.input' > /tmp/input_a.json
curl -s -u "$PK:$SK" "$HOST/api/public/observations/<obs_id>" | jq '.input' > /tmp/input_b.json
diff /tmp/input_a.json /tmp/input_b.json
```

结果：
- **system prompt 静态段**：完全相同
- **tools 数组**：完全相同（12 个核心工具）
- **唯一差异**：`Deferred Tools` 段

30,878 会话的 `Deferred Tools` 段额外包含 **13 个 Sentry MCP 工具的完整描述**（~6,238 tokens）：
```
- mcp__sentry__analyze_issue_with_seer: [MCP:sentry] Use Seer to analyze...
- mcp__sentry__find_organizations: [MCP:sentry] Find organizations...
- mcp__sentry__find_projects: [MCP:sentry] Find projects...
... (共 13 个 mcp__sentry__* 工具)
```

24,640 会话的 `Deferred Tools` 段仅包含 cron 工具（无 Sentry MCP）。

### 跨会话缓存失效模式

| 时间 | 会话 | 输入 tokens | Deferred Tools | cache_read | 缓存率 | 间隔 |
|------|------|------------|----------------|------------|--------|------|
| 06:37 | hello | **30,878** | cron + Sentry | 0 | 0% | — |
| 06:40 | hello | **24,640** | cron only | 8,192 | **33.2%** | 3 min |
| 06:46 | hello | **30,878** | cron + Sentry | 0 | 0% | 6 min |
| 06:52 | hello | **24,640** | cron only | 0 | 0% | 6 min |
| 06:57 | 测试工具 | 24,643 | cron only | 0 | 0% | 5 min |

关键观察：
- **3 分钟间隔命中静态段**（8K/24K = 33%）：Block 0 缓存有效，Block 1 因内容变化 miss
- **≥5 分钟全部 0%**：Anthropic cache TTL（5 分钟）到期，两个 block 都 miss
- **30,878 vs 24,640 交替出现**：精确对应 Sentry MCP 连接状态

### 会话内缓存正常

Session `019e539f` 的 3 次 LLM 调用（system prompt 冻结后不变）：

| 调用 | input_tokens | cache_read | 缓存率 |
|------|-------------|------------|--------|
| 1 | 24,643 | 0 | 0%（冷启动，创建缓存） |
| 2 | 24,843 | 24,640 | 99.2% |
| 3 | 24,972 | 24,832 | 99.5% |

### cache_creation_input_tokens 始终为 0

所有 LLM 调用（包括首次）的 `cache_creation_input_tokens` 均为 0。后续请求能 cache_read 说明缓存确实被创建了，但 Anthropic API 未在响应中报告 creation tokens。可能原因：代理不转发此字段，或 Anthropic 行为变更。

## 涉及文件

| 文件 | 位置 | 职责 |
|------|------|------|
| `tool_search/tool_index.rs` | :284 | `format_deferred_list()` — 根据已注册的 extra tools 生成描述文本 |
| `tool_search/middleware.rs` | :79 | `before_agent()` — `state.prepend_message(BaseMessage::system(cached))` 注入 |
| `anthropic/invoke.rs` | :202-222 | middleware 文本插入到 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 之后 |
| `anthropic/invoke.rs` | :331-334 | `i == last_idx` 导致动态 block 也获得 `cache_control` |
| `anthropic/cache.rs` | — | `split_system_blocks()` 按边界标记拆分为静态/动态两个 block |

## 修复方向

1. **动态 block 不加 `cache_control`**：修改 `invoke.rs:331-334`，仅对 `b.cache_control == true` 的 block 加断点，去掉 `i == last_idx` 的 fallback。这样动态段内容变化不影响缓存前缀，静态段可跨会话复用
2. **Deferred Tools 内容固定化**：在 frozen 阶段锁定 MCP 描述列表，即使 MCP 连接状态变化也不更新（与系统提示词稳定性原则对齐）
3. **移出 system prompt**：将 Deferred Tools 作为独立的消息类型或放在 tools 数组描述中，不污染 system prompt

方向 1 是最小改动且最安全——仅影响 `cache_control` 断点位置，不改变 system prompt 内容。
