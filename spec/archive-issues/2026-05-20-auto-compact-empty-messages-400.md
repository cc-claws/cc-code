> 归档于 2026-05-20，原路径 spec/issues/2026-05-20-auto-compact-empty-messages-400.md

# Auto compact 后 LLM 请求 messages 为空导致 400 错误

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-20
**修复日期**：2026-05-20

## 问题描述

上下文 token 超过阈值触发 auto compact 后，compact 完成紧接的 LLM 请求发送了空的 messages 数组，API 返回 400 错误 `messages: at least one message is required`。之后整个 session 卡死，无法继续对话。该问题在 DeepSeek 模型上必现。

## 根因

Compact 后将摘要作为 `BaseMessage::system()` 放入消息列表。LLM 适配器（`messages_to_json`/`messages_to_anthropic`）将 System 消息提取到 system 字段，不进入 messages 数组。结果发给 API 的 messages 数组中只有 system 角色消息，DeepSeek/OpenAI 兼容 API 要求至少一条 user/assistant 消息。

## 修复方案

架构从「外层 loop + resubmit（`compact_runner.rs`）」改为「`CompactMiddleware` 作为 `before_model` 钩子在 ReAct 循环内原地处理」，摘要始终作为 `BaseMessage::human()` 存入消息列表。

### 修改文件

| 文件 | 变更 |
|------|------|
| `peri-middlewares/src/compact_middleware.rs:230` | 新增 `CompactMiddleware`，摘要使用 `BaseMessage::human()` |
| `peri-acp/src/session/compact_runner.rs` | 已删除（旧架构代码） |
| `peri-acp/src/session/executor.rs` | 移除外层 compact loop + resubmit，委托给 `CompactMiddleware` |
| `peri-tui/src/acp_server/compact.rs:112` | 手动 `/compact` 路径同步修改，摘要使用 `BaseMessage::human()` |

### 修复后消息结构

compact 后 state messages 始终为 `[Human(summary), System(files...), System(skills...)]`。LLM 适配器提取 System 消息后，messages 数组至少有一条 Human 消息，不会再触发 400。

## 症状详情

| 维度 | 表现 |
|------|------|
| 触发时机 | Auto compact（上下文 token 超过阈值自动触发） |
| Compact 表现 | TUI 显示「上下文已压缩」通知 |
| 错误信息 | `LLM HTTP 错误 (400): API 错误 400 Bad Request: messages: at least one message is required` |
| 错误后状态 | Session 完全卡死，无法继续输入或发送消息 |
| 复现频率 | 必现 |

## 相关 Issue

- `spec/issues/2026-05-20-llm-error-message-area-clear-flicker.md` — compact 后 LLM 400 错误导致 UI 清空（不同层面：UI 表现 vs API 请求层面 messages 为空）
