> 归档于 2026-06-06，原路径 spec/issues/2026-06-06-agent-thinking-repeat-loop.md

# Agent 思考重复卡死，同一动作无限循环

**状态**：Closed
**优先级**：高
**创建日期**：2026-06-06
**关闭日期**：2026-06-06
**类型**：Bug

## 问题描述

Agent 在执行任务时，思考过程出现重复，卡在同一个动作上无限循环。用户发送"继续"后，agent 仍然重复同一动作，无法跳出循环。

## 症状详情

### 现象 1：思考内容重复

Agent 的思考输出完全相同，重复数十次：

```
Actually, let me try to use `cargo vendor` to download the source code.
Actually, let me try to use `cargo doc` to generate the docs for tui-textarea-2, which will show me the source code.
Actually, let me try to use `cargo doc` to generate the docs for tui-textarea-2, which will show me the source code.
Actually, let me try to use `cargo doc` to generate the docs for tui-textarea-2, which will show me the source code.
... (重复 20+ 次)
```

### 现象 2：用户发送"继续"后仍然重复

用户发送"继续"消息后，agent 继续重复同一动作，无法跳出循环。

### 现象 3：无工具调用

Agent 在重复思考时，没有实际调用任何工具，只是在思考中重复"让我尝试..."。

## 复现条件

- **复现频率**：偶发
- **触发步骤**：
  1. Agent 执行某个任务
  2. Agent 卡在某个动作上
  3. 思考内容开始重复
  4. 用户发送"继续"
  5. Agent 继续重复，无法跳出
- **环境**：所有模型、所有操作系统

## 涉及文件

- `peri-agent/src/agent/` —— Agent 执行逻辑
- `peri-middlewares/` —— 中间件链

## 关联 Issue

- 无

## 修复方案

在两个层面增加防线：

### 1. 流式层 — RepetitionDetector（`peri-agent/src/llm/repetition.rs`）

在 LLM 流式输出过程中检测退化重复：
- 按句子边界（句号/换行/感叹号/问号）分割文本
- 连续 10 个相同片段判定为退化重复
- 检测间隔 500 字符，最小检测长度 200 字符，避免误伤
- 检测到后设置 `finish_reason="stop"` 提前终止流

集成位置：
- `peri-agent/src/llm/anthropic/stream.rs`
- `peri-agent/src/llm/openai/stream.rs`

### 2. Agent 层 — check_stuck 卡住检测（`peri-agent/src/agent/executor/mod.rs`）

追踪连续多轮 thinking 指纹：
- 提取 thinking 内容前 200 字符作为指纹
- 连续 3 轮指纹相同则注入 Human 消息鼓励换策略
- 注入后重置计数器，避免重复注入

### 测试覆盖

`peri-agent/src/llm/repetition_test.rs` 新增 8 个单元测试：
- `test_normal_text_not_detected` — 正常文本不误报
- `test_alternating_sentences_not_detected` — 交替句子不误报
- `test_repetition_below_threshold` — 未达阈值不触发
- `test_repetition_detected_at_threshold` — 达到阈值正确触发
- `test_newline_separated_repetition` — 换行分隔的重复检测
- `test_too_short_not_checked` — 短文本跳过检测
- `test_no_recheck_within_interval` — 间隔内不重复检测
- `test_realistic_degenerate_output` — 真实退化场景检测

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-06 | — | Open | agent | 基于用户报告的 agent 思考重复现象创建 |
| 2026-06-06 | Open | Closed | wismyzhizi2018 | PR #6 合并修复，流式层+Agent 层双重防线 |
