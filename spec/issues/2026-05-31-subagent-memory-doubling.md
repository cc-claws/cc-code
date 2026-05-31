# SubAgent 导致内存翻倍（parent_messages 每轮全量克隆 + 系统提示词/LLM 客户端重复构建）

**状态**：Open
**优先级**：高
**创建日期**：2026-05-31

## 问题描述

开启 SubAgent 功能后，进程 RSS 近乎翻倍。根因有三：`before_agent` 每轮无条件全量克隆消息历史（最严重，即使不用 SubAgent 也触发）；Fork 模式消息历史存在 3 个副本；SubAgent 重建系统提示词和 LLM 客户端而非复用。

## 症状详情

### 现象 1：`before_agent` 每轮全量 clone（即使无 SubAgent）

`SubAgentMiddleware::before_agent()` 在每轮 ReAct 循环中无条件执行：

```rust
// peri-middlewares/src/subagent/mod.rs:456-457
*pm.write() = state.messages().to_vec();
```

对话累积 500+ 条消息后，每轮额外分配 ~1-2 MB。此内存在旧 Vec drop 后由 mimalloc 持有（purged 但不归还 OS），成为常驻开销。

### 现象 2：Fork 模式消息历史三重拷贝

fork 执行期间，同一条消息历史存在于三个位置：

| # | 位置 | 代码 |
|---|------|------|
| 1 | `parent_messages` | `mod.rs:457` `state.messages().to_vec()` |
| 2 | `parent_msgs`（局部变量） | `execute_fork.rs:20` `pm.read().clone()` |
| 3 | `fork_state` | `execute_fork.rs:52-53` `fork_state.add_message(msg)` |

fork 执行完成后 #2 和 #3 随函数返回 drop，但 #1 跨轮次常驻。

### 现象 3：SubAgent 重建系统提示词和 LLM 客户端

每次 fork/normal 子 Agent 执行时：

- `execute_fork.rs:64-66`：`builder(None, cwd)` 重建完整系统提示词（与父 Agent 内容相同，~200 KB-1 MiB）
- `execute_fork.rs:55` / `build_agent.rs:97`：`llm_factory(alias)` 创建新 `RetryableLLM<BaseModelReactLLM>`，内含新 `reqwest::Client`（~0.5-1.5 MiB）

主 Agent 构建器（`builder.rs`）已有 `CachedLlmInstances` 复用机制，但 SubAgent 路径完全未使用。

## 涉及文件

- `peri-middlewares/src/subagent/mod.rs:456-457` —— `before_agent` 每轮全量 clone
- `peri-middlewares/src/subagent/tool/execute_fork.rs:20-21,52-54,55,64-66` —— fork 模式三重拷贝 + 重建
- `peri-middlewares/src/subagent/tool/build_agent.rs:97,119-127` —— normal agent 路径的 LLM + system prompt 重建
- `peri-middlewares/src/subagent/tool/execute_bg.rs:255-256` —— background fork 同样的问题
- `peri-acp/src/agent/builder.rs:284-294,321-322` —— `system_builder` closure 和 `llm_factory` 定义
