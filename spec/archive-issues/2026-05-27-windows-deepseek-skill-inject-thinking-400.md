> 归档于 2026-05-31，原路径 spec/issues/2026-05-27-windows-deepseek-skill-inject-thinking-400.md

# Windows + DeepSeek Anthropic 兼容模式 /skill 注入假 Read 调用触发 thinking 400 错误

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-27
**更新日期**：2026-05-30

## 问题描述

在 Windows 环境下，使用 DeepSeek provider 的 Anthropic 兼容 API 接口时，通过 `/skill-name` 语法触发 SkillPreloadMiddleware 注入假的 Read 工具调用消息后，API 返回 400 错误：*"The `content[].thinking` in the thinking mode must be passed back to the API"*。该问题在 macOS/Linux 下未观察到，Windows 下 100% 必现，且在第一轮对话即触发。

## 症状详情

| 维度 | 现象 |
|------|------|
| 错误信息 | LLM HTTP 错误 (400): API 错误 400 Bad Request: The `content[].thinking` in the thinking mode must be passed back to the API |
| 触发条件 | 用户消息中包含 `/skill-name` token，SkillPreloadMiddleware 注入 Ai[ToolUse{Read}] + Tool[ToolResult] 消息对 |
| 平台 | 仅 Windows（macOS/Linux 正常） |
| 模型/Provider | DeepSeek 模型的 Anthropic 兼容 API 接口 |
| 出现轮次 | 第一轮对话即报错 |
| 复现频率 | 100% 必现 |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. Windows 环境下启动 peri，配置 DeepSeek provider 使用 Anthropic 兼容 API
  2. 在对话中输入包含 `/skill-name` 的消息（触发 SkillPreloadMiddleware 假 Read 调用注入）
  3. Agent 构建请求发送至 DeepSeek Anthropic 兼容接口
  4. API 返回 400 错误
- **环境**：Windows + DeepSeek provider + Anthropic 协议兼容接口

## 涉及文件

- `peri-middlewares/src/subagent/skill_preload.rs` —— SkillPreloadMiddleware，在 `before_agent` 中检测 `/skill-name` token 后，将 SKILL.md 内容以 Ai[ToolUse{Read, path}] + Tool[ToolResult] 消息对注入 agent state
