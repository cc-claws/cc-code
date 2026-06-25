> 归档于 2026-05-16，原路径 spec/issues/2026-05-13-cache-warning-show-request-id.md
# 缓存命中率警告中显示 API Request ID

**状态**：Fixed
**优先级**：低
**类型**：Feature
**创建日期**：2026-05-13

## 问题描述

当 Prompt cache 命中率低于 80% 时，TUI 显示警告消息 `⚠ Prompt cache 命中率 35% < 80%`，但缺少 API 请求标识，无法快速在 Anthropic Console / OpenAI Dashboard 中定位对应请求进行排查。

## 症状详情

当前提示：
```
⚠ Prompt cache 命中率 35% < 80%
```

期望提示：
```
⚠ Prompt cache 命中率 35% < 80% (req: req_01XFDUDYJgAACzvnptvVoYEL)
```

若 request ID 不可用（如本地 mock、provider 未返回），则省略括号部分或显示为空。

## 涉及文件

- `peri-agent/src/llm/anthropic.rs` —— 需从响应头提取 `x-request-id`
- `peri-agent/src/llm/openai.rs` —— 需从响应体提取 `id` 字段
- `peri-agent/src/agent/token.rs` —— `SessionTokenTracker` 需记录最近一次 request ID
- `peri-tui/src/app/agent_ops.rs:152-171` —— 缓存率检查逻辑，拼接警告消息时附带 request ID
- `peri-tui/src/ui/main_ui/status_bar.rs:104-105` —— status bar 的缓存率显示（可选）
