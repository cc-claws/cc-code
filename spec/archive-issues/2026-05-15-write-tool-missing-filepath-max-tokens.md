> 归档于 2026-05-16，原路径 spec/issues/2026-05-15-write-tool-missing-filepath-max-tokens.md

# Write 工具超长内容触发 max_tokens 截断导致 file_path 缺失

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-15

## 问题描述

Write 工具在写入超长内容（如 ~4000+ token 的实现计划文档）时频繁失败（>50%），错误信息为 `Missing file_path parameter`。根因是 LLM 流式生成 JSON 参数时，`content` 字段占用了大量输出 token，触发 `max_tokens=4096` 截断，导致 `file_path` 从未被写入 JSON 对象。

两种失败模式：

1. **GLM-5.1**：JSON 键顺序为 `{"content": "超长文本"}` 优先，`content` 写完且 JSON 闭合后 token 用完，`file_path` 从未产出 → 合法的 JSON 对象但缺少 `file_path` 键
2. **DeepSeek v4 Pro**：JSON 键顺序为 `{"file_path": "/path/to/file", "content": "超长文本...` 优先，`content` 写到一半截断 → JSON 非法无法解析

## 症状详情

### 日志证据

**GLM-5.1（`data/2026-05-15_15-05-05-698_0008/stream.log`）**：

```
content_block_start: {"type": "tool_use", "name": "Write", "input": {}}
content_block_delta: partial_json: {"content":"# Tool Output Truncation..."}
... (content 持续流式输出，约 4000 token)
content_block_stop
message_delta: stop_reason: "max_tokens", usage: {"output_tokens": 4096}
```

LLM 产出了完整的 `{"content": "超长计划文档文本"}` 但 `file_path` 从未出现。

**DeepSeek v4 Pro（`data/2026-05-15_15-44-10-887_0001/stream.log`）**：

```
content_block_start: {"type": "tool_use", "name": "Write", "input": {}}
content_block_delta: partial_json: {file_path":"/Users/konghayao/...", "content":"# ...}
... (content 流式输出到 12100 字符后被截断，JSON 未闭合)
message_delta: stop_reason: "max_tokens"
```

LLM 正确地将 `file_path` 放在前面，但 `content` 写到一半时 token 耗尽，JSON 不合法。

### 涉及模型

| 模型 | max_tokens | 失败模式 | 频率 |
|------|-----------|----------|------|
| GLM-5.1 (Anthropic) | 4096 | content 先于 file_path，JSON 完整但缺键 | 高 |
| DeepSeek v4 Pro (OpenAI) | 4096 | 正确排序但 content 截断致 JSON 非法 | 中 |

### 共同前提

两个模型的 `max_tokens` 都是 4096。当 LLM 计划使用 Write 写入超长内容时（如 4000+ token 的代码/计划），输出 token 不足以同时容纳 `file_path` + `content`。

## 复现条件

- **复现频率**：>50%（取决于是否有超长内容要写入）
- **触发步骤**：
  1. LLM 产出需要写入的超长内容（~4000+ tokens）
  2. LLM 调用 Write 工具流式输出 JSON
  3. `max_tokens=4096` 耗尽，JSON 截断在 `content` 字段中或之后
- **环境**：GLM-5.1 / DeepSeek v4 Pro，`max_tokens=4096`（默认值）

## 根因分析

1. **直接原因**：`max_tokens` 限制迫使 LLM 在写入超长内容时截断 JSON 参数对象
2. **键顺序因素**（GLM-5.1）：如果 LLM 将 `content` 放在 `file_path` 前，即使 JSON 在语法上完整，也会缺少 `file_path` 键
3. **系统瓶颈**：当前的 `max_tokens=4096` 不足以同时容纳工具调用的元信息和超长参数值

## 相关代码

- `peri-middlewares/src/tools/filesystem/write.rs:58-67` — `invoke()` 要求 `file_path` 和 `content` 两个键都存在
- `peri-middlewares/src/tools/filesystem/write.rs:41-56` — `parameters()` 定义，`file_path` 在 properties 中排在 `content` 之前
- `peri-agent/src/llm/anthropic/invoke.rs` — Anthropic 适配器，`max_tokens` 在此设置
- `peri-agent/src/llm/openai/invoke.rs` — OpenAI 适配器，`max_tokens` 在此设置

## 期望改进方向

需要从多个层面降低 Write 工具参数截断的概率：

1. **工具层防御**：当 `file_path` 缺失时，不直接报错，而是检查能否从上下文推断（如工作目录 + 文件内容推断或提示 LLM 重试）
2. **Schema 优化**：强化 `file_path` 的优先级提示，建议 LLM 始终先输出 `file_path`
3. **max_tokens 策略**：超长 Write 场景可能需要更大的 output token 预算
4. **分块写入策略**：如果内容超长，LLM 可考虑先写文件路径占位，再用 Edit/追加写入内容

## 关联 Issue

- `spec/issues/2026-05-15-glm-anthropic-tool-result-id-attribute-error.md`（Fixed）— 同一次会话中出现的关联问题，GLM 5.1 的 tool_result `id` 属性缺失与 Write 工具 `file_path` 缺失同属 LLM 工具调用的参数完整性问题
