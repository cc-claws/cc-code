# Agent 工具调用 3.35% 错误率——93% 源于 subagent_type 参数缺失

**状态**：Fixed
**优先级**：中
**创建日期**：2026-06-05
**类型**：Bug

## 问题描述

通过 `agent-defect-analyzer` 对 `~/.peri/threads/threads.db` 的历史数据分析发现，Agent（SubAgent 调度）工具在 1,344 次调用中有 45 次返回错误（以 `Error:` 开头），错误率 **3.35%**。其中 93.3% 的错误源于同一根因：LLM 调用 Agent 工具时遗漏 `subagent_type` 必填参数或超出并发限制，表明工具描述中对参数要求的传达不够清晰。

## 症状详情

### 总体数据

| 指标 | 值 |
|------|-----|
| Agent 总调用 | 1,344 |
| 成功（内容正常返回） | 1,299 |
| 失败（内容以 `Error:` 开头） | 45 |
| **错误率** | **3.35%** |

### 错误分类

| 错误类型 | 次数 | 占比 | 示例消息 |
|---------|------|------|----------|
| 缺少 `subagent_type`（非后台） | 21 | 46.7% | `Error: please provide subagent_type parameter to specify the agent type, or use fork: true for fork mode` |
| 缺少 `subagent_type`（后台模式） | 15 | 33.3% | `Error: background mode requires subagent_type parameter (or use fork: true)` |
| 超过并发上限 | 5 | 11.1% | `Error: maximum 3 concurrent background tasks reached. Wait for a running task to complete before starting a new one.` |
| agent 定义不存在 | 3 | 6.7% | `Error: cannot find agent definition 'code-reviewer'. Check .claude/agents/ directory or use a built-in agent (explore, plan, general-purpose, verification)` |
| 缺少 `prompt` 参数 | 1 | 2.2% | `Error: missing required parameter prompt` |

### 错误集中度

- **`subagent_type` 缺失**（21 + 15 = 36 次）占所有错误的 **80%**
- 加上并发限制（本质也是参数使用不当，5 次）共 **91.1%**
- 真正的"异常"错误（agent 定义不存在 + prompt 缺失）仅 4 次，占 8.9%

### 与 Edit 工具的相似问题

与 `spec/issues/2026-06-03-edit-tool-errors-invisible-and-retry-inefficient.md` 类似，Agent 工具的错误也以 `Ok("Error: ...")` 返回而非 `Err()`，导致 `is_error` 字段为 false，`tool_errors` 分析器无法捕获。现有的 `tool_errors` 分析器仅依赖 `is_error=true` 筛选，完全遗漏了这 45 次 Agent 错误。

## 复现条件

- **复现频率**：偶发（取决于 LLM 是否正确构造参数）
- **触发步骤**：
  1. 让 LLM 在非 fork 模式下调用 Agent 工具但不提供 `subagent_type`
  2. 或让 LLM 在 `run_in_background: true` 时不提供 `subagent_type`
  3. 工具返回 `Error: please provide subagent_type...`，但 `is_error=false`
- **环境**：所有模型

## 涉及文件

- `peri-middlewares/src/subagent/tool/define.rs` —— Agent 工具定义与参数校验，错误返回方式
- `peri-middlewares/src/subagent/tool/execute_bg.rs` —— 后台模式 `subagent_type` 校验（:56 行）

## 关联 Issue

- `spec/issues/2026-06-03-edit-tool-errors-invisible-and-retry-inefficient.md` —— Edit 工具同样的 `Ok("Error: ...")` 返回问题，同根因

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-05 | — | Open | agent | 基于 agent-defect-analyzer 数据分析创建 |
| 2026-06-05 | Open | Fixed | agent | 修复 #1: 4 处 Ok("Error:") → Err()，参数描述强化 |

## 修复记录

### 修复 #1（2026-06-05）

- **操作人**：agent
- **用户原意**：Agent 工具错误应对监控系统可见，且 LLM 应更少遗漏 subagent_type 参数
- **修复内容**：
  1. `define.rs`: prompt 缺失 + subagent_type 缺失，2 处 `Ok("Error:")` → `Err()`
  2. `define.rs`: load_agent_def 错误 `Ok(e)` → `Err(e.into())`
  3. `execute_bg.rs`: 并发上限 + 后台 subagent_type 缺失，2 处 `Ok("Error:")` → `Err()`
  4. `execute_bg.rs`: load_agent_def 错误 `Ok(e)` → `Err(e.into())`
  5. `execute_fork.rs`: parent_messages 缺失 `Ok("Error:")` → `Err()`
  6. `define.rs`: subagent_type 参数描述强调 REQUIRED unless fork=true
  7. `tool_test.rs`: 4 个测试断言从 `.unwrap()` 改为 `.unwrap_err()`
- **验证状态**：817 测试通过，clippy 无 error
