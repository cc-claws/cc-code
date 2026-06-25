> 归档于 2026-05-24，原路径 spec/issues/2026-05-23-tui-image-sent-as-text.md

# TUI 粘贴图片后 LLM 仅收到文本而非图片内容

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-23
**修复日期**：2026-05-23

## 问题描述

用户在 TUI 中粘贴图片并发送消息后，Anthropic 和 OpenAI 两个 provider 的 LLM 均只收到文本内容而非图片。TUI 界面显示似乎正常，但 LLM 回复明确表明未接收到图片。

## 症状详情

- 发送方向：用户 → LLM
- TUI 界面表现：粘贴图片后界面显示正常
- 实际发出去的内容：文本（非图片格式）
- LLM 表现：回复表明只看到了文字，无法做视觉分析

### 具体 LLM 回复内容

**第一次发送图片后，LLM 回复**：
> 我没有收到你发送的图片。你可以尝试：
> 1. 直接粘贴图片到对话框
> 2. 上传图片文件（拖拽或点击附件按钮）
> 3. 提供图片链接（URL）
> 请重新发送图片，我会帮你分析内容。

**再次发送后，LLM 回复**：
> 抱歉，我这边没有收到你发送的图片。可能是上传失败了。
> 请再次尝试发送图片，或者描述一下图片的内容，我可以尽力帮你解答。

**用户侧 TUI 显示**：`发过去咯 [ 1 张图片 ]`

## 根因分析

TUI → ACP → Executor 整个链路只传递 `String` 纯文本，图片附件从未进入消息内容：

1. `event/keyboard.rs` Ctrl+V 粘贴图片 → 成功存入 `pending_attachments`（base64 PNG）
2. `app/agent_submit.rs` `submit_message()` 消费 `attachments` → 但只把 `input`（纯文本）传给 `client.prompt()`
3. `acp_client/client.rs` `prompt()` 发送 JSON `{ "role": "user", "content": text }` → 只有字符串
4. `acp_server/prompt.rs` 提取 `content` → `.as_str()` 只解析字符串
5. `peri-acp/src/session/executor.rs` `execute_prompt()` 接收 `content: String` → 构建 `AgentInput::text(content)`

图片数据在步骤 2 之后就丢失了，后续所有层都只处理纯文本。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 在 TUI 中粘贴图片
  2. 发送消息
  3. LLM 回复表明只收到了文本
- **环境**：Anthropic 和 OpenAI 两个 provider 均有此问题

## 涉及文件

- `peri-agent/src/messages/content.rs` — ContentBlock::Image 与 ImageSource 定义
- `peri-agent/src/messages/adapters/openai.rs` — OpenAI 格式消息适配（含 Image block 序列化）
- `peri-agent/src/messages/adapters/anthropic.rs` — Anthropic 格式消息适配（含 Image block 序列化）
- `peri-agent/src/llm/openai/invoke.rs` — OpenAI 请求构建（content_to_openai）
- `peri-agent/src/llm/anthropic/invoke.rs` — Anthropic 请求构建（content_to_anthropic）

## 修复方案

将 TUI → ACP → Executor 整条链路的 `String` 升级为 `MessageContent`，支持多模态 blocks：

| 文件 | 变更 |
|------|------|
| `peri-acp/src/session/executor.rs` | `content: String` → `content: MessageContent`，`AgentInput::text()` → `AgentInput::blocks()` |
| `peri-tui/src/acp_server/prompt.rs` | 解析 `content` 为 `MessageContent`（兼容字符串和 blocks 数组） |
| `peri-tui/src/acp_client/client.rs` | `prompt(&str)` → `prompt(&MessageContent)` |
| `peri-tui/src/app/agent_submit.rs` | 构建 `MessageContent::Blocks([Text, Image...])` 包含附件 |
| `peri-tui/src/acp_stdio.rs` | ACP SDK ContentBlocks → `MessageContent` 转换 |
| `peri-tui/src/cli_print.rs` | 适配新签名传递 `MessageContent::text()` |

## 修复验证

- `cargo build -p peri-acp -p peri-tui` 编译通过
- `cargo test -p peri-acp --lib` 29 passed
- `cargo test -p peri-tui --lib` 525 passed
- `cargo test -p peri-agent --lib` 386 passed
