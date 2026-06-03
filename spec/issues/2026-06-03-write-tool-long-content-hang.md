---
id: 2026-06-03-write-tool-long-content-hang
title: Write 工具参数过长时执行卡住无响应
status: open
priority: high
created: 2026-06-03
---

## 问题

当大模型生成的 Write 工具 `file_content` 参数过长时，Write 工具执行会卡住无响应，最终超时。问题必现，且不限于特定模型。

## 症状详情

- **触发条件**：LLM 输出的 Write 工具调用中 `file_content` 参数非常长（如生成数千行代码文件）
- **现象**：工具执行开始后卡住，无任何输出或进度，最终超时
- **错误信息**：无明确错误提示，只是静默卡住
- **复现频率**：必现
- **影响范围**：所有模型 provider

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 让模型生成一个很大的文件（如数千行代码）
  2. 模型调用 Write 工具，`file_content` 参数极长
  3. Write 执行卡住，无响应直到超时
- **环境**：所有 provider

## 涉及文件

- `peri-middlewares/src/tools/filesystem/` —— Write 工具实现
- `peri-agent/src/agent/tool_dispatch.rs` —— 工具调度与超时处理
