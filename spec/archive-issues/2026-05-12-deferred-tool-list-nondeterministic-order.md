> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-deferred-tool-list-nondeterministic-order.md

# 多处 HashMap 非确定性顺序导致 Anthropic Prompt Cache 前缀不稳定

**状态**：Fixed + Verify
**优先级**：中
**创建日期**：2026-05-12

## 问题描述

Anthropic prompt cache 基于 prefix 精确匹配。项目中多处使用 `HashMap` 收集工具/条目后直接迭代生成 API 请求内容，HashMap 迭代顺序不确定（Rust 默认 `RandomState` hasher 每次进程启动随机种子不同），导致跨进程重启时 API 请求前缀变化，缓存失效。

## 症状详情

### 核心问题：工具列表顺序不固定

`ToolSearchIndex::format_deferred_list()` 从内部 `HashMap` 迭代生成 deferred tools 列表，注入 system prompt。当前项目有 16 个 deferred tools（CronCreate/CronDelete/CronList/MCP 工具等），列表足够长，HashMap 迭代顺序变化概率高。

```
进程 A 的 system prompt 中 Deferred Tools 段：
  - CronCreate: ...
  - mcp__ide__executeCode: ...
  - mcp__plugin_weixin_weixin__reply: ...
  - CronList: ...
  (HashMap 随机顺序 A)

进程 B 的 system prompt 中 Deferred Tools 段：
  - CronList: ...
  - mcp__plugin_weixin_weixin__reply: ...
  - CronCreate: ...
  - mcp__ide__executeCode: ...
  (HashMap 随机顺序 B)
```

Anthropic prompt cache 基于 prefix 匹配。system prompt 和 tools 数组是请求的前缀部分，顺序变化导致缓存段无法跨进程复用。

### 同进程内：System 消息被过滤导致每轮需重新注入

agent 完成后所有 System 消息被过滤不写入 `agent_state_messages`（`agent.rs:434`）。下一轮 `before_agent` 重新注入 system 消息，如果 `ToolSearchIndex` 被重建（之前每次 submit 都 `Arc::new`），则缓存丢失，重新生成的工具列表顺序与首轮不同，缓存前缀变化。

### 问题点

| 位置 | 数据结构 | 影响 | 严重度 |
|------|---------|------|--------|
| `ToolSearchIndex::format_deferred_list()` | `HashMap` 迭代 | deferred tools 列表注入 system prompt 的顺序不确定 | 🔴 核心问题 |
| `executor::mod.rs` `tool_refs` | `HashMap.values()` | 发送给 LLM 的 `tools` JSON 数组顺序不确定 | 🔴 核心问题 |
| `ToolSearchIndex` 每次 submit 重建 | `run_universal_agent` 局部变量 | `cached_prompt` 跨 submit 丢失 | 🔴 核心问题 |

## 修复方案

1. **`format_deferred_list()` 按名称排序**：收集 `(name, tool)` 对后 `sort_by_key`，保证跨进程输出一致
2. **`tool_refs` 按名称排序**：`executor/mod.rs` 中 `collect()` 后 `sort_by_key(|t| t.name())`，保证 `tools` JSON 数组顺序稳定
3. **`ToolSearchIndex` 会话级缓存**：将 `ToolSearchIndex` 和 `shared_tools` 从 `run_universal_agent` 局部变量提升到 `AgentComm`（session 级），跨 submit 复用同一 `Arc`
4. **每轮注入缓存提示词**：System 消息在 agent 完成后被过滤不写入 `agent_state_messages`，所以每轮 `before_agent` 需重新注入同一缓存内容（跳过重新构建/格式化）

## 修改文件

- `peri-middlewares/src/tool_search/tool_index.rs` — `cached_prompt` 字段 + `format_deferred_list` 排序
- `peri-middlewares/src/tool_search/middleware.rs` — 首次构建+缓存，后续注入缓存内容
- `peri-agent/src/agent/executor/mod.rs` — `tool_refs` 按名称排序
- `peri-tui/src/app/agent_comm.rs` — 新增 `tool_search_index`/`shared_tools` session 级字段
- `peri-tui/src/app/agent.rs` — `AgentRunConfig` 新增字段，`run_universal_agent` 使用共享实例
- `peri-tui/src/app/agent_submit.rs` — 首次 submit 初始化，后续复用
