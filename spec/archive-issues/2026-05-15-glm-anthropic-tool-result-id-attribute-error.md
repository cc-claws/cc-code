> 归档于 2026-05-16，原路径 spec/issues/2026-05-15-glm-anthropic-tool-result-id-attribute-error.md

# GLM Anthropic 兼容端口 500: tool_result block 缺少 id 属性导致多轮工具调用失败

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-15
**修复日期**：2026-05-15

## 问题描述

使用 GLM 5.1 模型的 Anthropic 兼容端口进行多轮工具调用时，API 返回 500 Internal Server Error：`'ClaudeContentBlockToolResult' object has no attribute 'id'`。首轮工具调用正常，经过若干轮后触发。重试 5 次均失败，Agent 停止执行。

根因是 GLM 的 Python 网关内部对所有 content block 统一访问 `.id` 属性，但 Anthropic API 规范中 `tool_result` block 只有 `tool_use_id`，没有自己的 `id` 字段。我们的序列化代码符合规范，这是 GLM 网关的 bug。

## 症状详情

### 错误日志

```
2026-05-15T13:40:13.202284Z  WARN thread.run{...}:agent.execute{max_iterations=500}:
  peri_agent::llm::retry: LLM 调用失败，准备重试
  attempt=5 max_retries=5 delay_ms=18118
  error=LLM HTTP 错误 (500): API 错误 500 Internal Server Error:
  'ClaudeContentBlockToolResult' object has no attribute 'id'
```

### 触发模式

- **首轮工具调用**：正常完成
- **多轮工具调用后**（具体轮次不确定）：500 错误，所有重试均失败
- **错误类型**：Python `AttributeError`——GLM 网关内部 `ClaudeContentBlockToolResult` 对象缺少 `id` 属性

### 消息序列化（我们的代码）

`peri-agent/src/llm/anthropic/invoke.rs:45-57` 生成的 `tool_result` block：

```json
{
  "type": "tool_result",
  "tool_use_id": "call_xxx",
  "content": [...],
  "is_error": false
}
```

符合 Anthropic Messages API 规范（`tool_result` block 不含 `id` 字段），但 GLM 网关要求此字段。

### 现象 2（2026-05-15）

Write 工具调用失败（缺少 `file_path` 参数）后，错误 tool_result 发送回 LLM 时触发同样的 500 错误。日志显示完整的请求体被记录到 `data/2026-05-15_15-07-18-924_0014/` 目录。

## 复现条件

- **复现频率**：多轮工具调用后触发（首轮正常）
- **触发步骤**：
  1. 配置 GLM 5.1 的 Anthropic 兼容端口（`OPENAI_BASE_URL` 指向 GLM 端点，使用 Anthropic adapter）
  2. 进行首轮工具调用 → 正常
  3. 继续多轮工具调用 → 某轮触发 500
- **环境**：GLM 5.1，Anthropic 兼容端口，多轮工具调用

## 相关代码

- `peri-agent/src/llm/anthropic/invoke.rs:45-57` — `block_to_anthropic` 中 `ToolResult` 的序列化（无 `id`，符合规范）
- `peri-agent/src/llm/anthropic/invoke.rs:152-187` — `messages_to_anthropic` 中 `Tool` 消息合并为 user content blocks
- `peri-agent/src/llm/anthropic/cache.rs:161-193` — `ensure_thinking_blocks` 为 assistant 消息注入 thinking 占位（多轮时消息历史增长可能影响 GLM 网关验证路径）
- `peri-agent/src/llm/retry.rs` — 重试逻辑，5 次重试后放弃

## 交叉验证结论（3 个并行 agent）

### Agent 1：Anthropic API 规范验证

`tool_result` content block 在官方 Anthropic Messages API 规范中**没有 `id` 字段**。代码中的序列化（`invoke.rs:45-57`）、反序列化（`content.rs:188-209`）、全部测试用例均一致——只有 `tool_use_id`，没有 `id`。

| Block 类型 | ID 字段 | 用途 |
|-----------|---------|------|
| `tool_use` | `id` | 工具调用的唯一标识 |
| `tool_result` | `tool_use_id`（引用） | 对 tool_use 的响应，无独立 id |

### Agent 2：多轮失败根因分析

首轮正常、多轮后失败的最可能原因：

1. 消息历史增长后，GLM 网关进入**更严格的验证路径**——对所有 content block 统一访问 `.id` 属性
2. 首轮消息结构简单（只有 1 个 tool_result），GLM 可能有容错/快捷路径
3. 多轮后消息中混杂了 text、tool_use、tool_result、thinking 等多种 block，触发统一验证逻辑
4. 如果 `extended_thinking` 开启，`ensure_thinking_blocks` 注入的 thinking 占位进一步增加消息复杂度

### Agent 3：OpenAI 适配器对比

**GLM 5.1 切换到 OpenAI 兼容端口可以完全规避此问题**：

| 特性 | Anthropic Adapter | OpenAI Adapter | GLM 兼容性 |
|------|-------------------|----------------|------------|
| 工具结果格式 | content block (`tool_result`) | 独立消息 (`role: "tool"`) | OpenAI 兼容 |
| ID 字段名 | `tool_use_id` | `tool_call_id` | OpenAI 兼容 |
| Reasoning 支持 | thinking block | 顶层 `reasoning`/`reasoning_content` | OpenAI 兼容（代码注释明确提到"GLM 系列模型使用 reasoning 字段名"） |

OpenAI 格式不存在 `tool_result` content block 的概念（工具结果是独立 `role: "tool"` 消息），从根本上避开了 GLM 的 `.id` 访问。

## Workaround 方向

**方案 A（推荐：切换到 OpenAI 兼容端口）**：GLM 5.1 同时提供 OpenAI 和 Anthropic 两种兼容端口。使用 OpenAI 兼容端口可以完全规避此问题，且代码中已有 GLM 系列模型的 reasoning 兼容处理（`openai/invoke.rs` 注释提到"GLM 系列模型使用 reasoning 字段名"）。

**方案 B（客户端兼容）**：在 `block_to_anthropic` 的 `ToolResult` 分支中添加 `"id"` 字段（值可用 `tool_use_id`），兼容 GLM 网关的期望。风险：其他 provider 可能不接受未知字段。需要按 provider 条件注入（`ChatAnthropic` 增加 `compat_tool_result_id` 标志）。

**方案 C（等待 GLM 修复）**：向智谱反馈此 bug，等待其修复网关。

## 关联 Issue

- `spec/issues/2026-05-14-deepseek-multi-turn-tool-result-duplication.md`（Fixed）— 同为第三方 Anthropic 兼容端口的兼容性问题
- CLAUDE.md 中的 DeepSeek `unknown variant 'thinking'` TRAP — 同类问题：第三方 provider 对 Anthropic API 规范实现不完整

## 修复记录（2026-05-15）

采用方案 B（客户端兼容），为所有 `tool_result` block 添加 `id` 字段。

### 修改内容

| 文件 | 修改 |
|------|------|
| `peri-agent/src/messages/content.rs` | `ContentBlock::ToolResult` 结构体新增 `id: Option<String>` 字段；序列化时条件写入 `id`；反序列化时解析 `id` |
| `peri-agent/src/llm/anthropic/invoke.rs` | `block_to_anthropic` 的 `ToolResult` 分支：有 `id` 则用，无则生成 `toolu_{uuid_v7}`；`messages_to_anthropic` 的 `BaseMessage::Tool` 转换：使用 Tool 消息自身的 `MessageId`（UUID v7）作为 `id` |
| `peri-agent/src/messages/adapters/anthropic.rs` | 同上两处修改：`block_to_anthropic` + `to_anthropic_with_system` |

### ID 来源

- `BaseMessage::Tool` 路径：使用 Tool 消息自带的 `id: MessageId`（UUID v7），全局唯一
- `ContentBlock::ToolResult` 路径：优先使用结构体内的 `id`，无则运行时生成 `toolu_{uuid_v7}`

### 验证

全部 389 个测试通过（`cargo test -p peri-agent`），包含 tool_result 序列化/反序列化、Anthropic 适配器、tool_dispatch 不变量等测试。
