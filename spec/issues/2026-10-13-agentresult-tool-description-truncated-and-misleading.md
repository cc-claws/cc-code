# 系统提示中 AgentResult 工具描述被截断、且与「可调用工具」清单定位冲突，导致 agent 反复发起无效调用

**状态**：Open
**优先级**：中
**创建日期**：2026-10-13
**后续**：按仓库约定，本文件是详细分析补充，工单号唯一来源仍需在 `cc-claws/cc-code` 建正式 GitHub Issue（已建：#470）。

## 问题描述

在并行运行后台子 Agent 的会话中，主 agent 在需要获取后台结果时，主动调用了 `AgentResult` 工具，收到固定报错 "AgentResult does not support queries or polling..."，并连续多次无效调用。

诱发因素（可观察）：`AgentResult` 被列在系统提示词的 Deferred Tools 区块（语义是「可发现、可按需调用」）中，而它实际是「不可直接调用」的兼容标记工具；同时其描述中最关键的禁令（"Do not call it directly..."）在系统提示词里被截断，不可见。

## 症状详情

### 现象 1：系统提示词中该工具描述在句中被截断

系统提示词 Deferred Tools 区块可见文本（末尾以省略号断开）：

```
- AgentResult: Compatibility marker for automatically delivered background agent results. This tool cannot query or poll task status or output, including Bash tasks. Do not ca…
```

该形态在 2026-10-13 的会话中现场可见（系统提示词原样呈现），非纸面推演。

被截掉的后半段恰是最关键约束：

> "…Do not call it directly or through ExecuteExtraTool; wait for the background completion notification instead."

### 现象 2：清单定位与实际定位冲突

Deferred Tools 区块引导语鼓励「发现并按需调用工具」：

> "Additional capabilities are listed below. Use `SearchExtraTools` for full descriptions and input schemas; invoke a needed, authorized tool through `ExecuteExtraTool`."

而 `AgentResult` 的完整定义是「不可直接调用的兼容占位」（见「涉及文件」），二者语义冲突——它被放在了「可调用」语义的清单里。

### 现象 3：调用后的表现

直接调用 `AgentResult` 返回固定提示（原文）：

> AgentResult does not support queries or polling. No task status or output was checked. Background results are delivered automatically on completion; continue other work or wait for that notification. Do not retry AgentResult.

实现上 `AgentResultTool::invoke()` 走的是**错误路径**（`Err(ErrorKind::Unsupported)`，`cc-middlewares/src/subagent/agent_result.rs:54`），即模型拿到的是工具错误而非成功结果；该工具从不返回结果，真实结果只通过后台完成通知自动投递。

### 现象 4：上下文伪 tool_use 历史注入诱发模仿（2026-10-14 实测证实）

2026-10-14 会话实测证实了模型主动发起调用的深层认知诱因：
在 `cc-acp/src/session/executor.rs:711-749` 的 `inject_bg_result_messages()` 中，当后台任务完成时，后端程序向对话历史中合成注入了一对消息：
1. 一条 AI 发出的 `ContentBlock::ToolUse { name: "AgentResult", input: { "task_id": ... } }`；
2. 紧随其后的一条对应 `tool_result`，包含后台任务的完成输出。

**认知诱导机制**：
即使系统提示词未将 `AgentResult` 列入可用清单，但由于在多轮对话中，前序后台任务完成后上下文中真实留下了“AI 曾主动调用 `AgentResult` 并获得了成功结果”的历史记录（Few-shot 示范），模型在后续等待新的后台任务时，会被上下文中的历史行为模式强烈诱导，依样画葫芦主动调用 `AgentResult` 去“拉取”结果，进而触发固定错误并陷入无谓重试。

## 复现条件

- **复现频率**：必现（描述截断为确定性行为）
- **前置条件**：会话启用了后台子 Agent（`SubAgentMiddleware` 持有 `background_registry`）。`AgentResultTool` 只在 `background_registry.is_some()` 时才注册进工具集（`cc-middlewares/src/subagent/mod.rs:528`），未启用时该条目根本不出现在 Deferred Tools 区块。
- **触发步骤**：
  1. 在会话中并行启动后台子 Agent（`run_in_background=true`）
  2. 查看系统提示词 Deferred Tools 区块的 `AgentResult` 条目 → 描述以 "Do not ca…" 截断（现象 1）
  3. 第一个后台任务完成后，上下文中被注入 `tool_use: AgentResult` 与结果
  4. 启动第二个后台任务，观察主 agent 在等待后台结果期间是否被诱发主动模仿调用 `AgentResult`
- **环境**：peri 本仓库；Windows；会话模型为当前 agent 模型

## 涉及文件

- `cc-middlewares/src/tool_search/tool_index.rs:300-301` —— `format_deferred_list()`（函数入口 `:274`）取描述首个非空行后按 **160 字符** 硬截断（`take(160)` 在 `:300`，超出补 `…` 在 `:301`）。`AgentResult` 描述全长 **260** 字符，前 160 字符正好止于 `...including Bash tasks. Do not ca`，第 161 字符为 `l`，故输出 `Do not ca…`，后续禁令全部丢失。
- `cc-acp/src/session/executor.rs:711-749` —— `inject_bg_result_messages()`：后台任务完成后以 `tool_use: AgentResult` 合成消息注入历史，给 LLM 形成“可通过该工具取结果”的假象范式。
- `cc-tui/prompts/sections/11_subagent.md` —— 系统提示词 SubAgent 章节：缺乏对“严禁调用 AgentResult，历史记录中的 AgentResult 仅为内部标记”的显式负向约束。
- `cc-middlewares/src/subagent/tool/define.rs:39-51` —— `AGENT_DESCRIPTION`：`Usage` 中未明确明令禁止主动调用 `AgentResult`。
- `cc-middlewares/src/subagent/agent_result.rs:5-7` —— `AgentResult` 工具定义：`AGENT_RESULT_DESCRIPTION`（含「不可直接调用 + 等通知」约束）与 `AGENT_RESULT_UNSUPPORTED`（调用后返回的固定提示）；`:54` 为 `invoke()` 返回 `Err(Unsupported)` 的位置。
- `cc-middlewares/src/subagent/mod.rs:528` —— 注册点：仅在 `background_registry.is_some()` 时 `tools.push(Box::new(AgentResultTool::new()))`。

## 解决与压制建议

1. **系统提示词补充强负向约束（`cc-tui/prompts/sections/11_subagent.md`）**：
   在 `## Usage notes` 中增加明令禁止：
   > `- NEVER call AgentResult: Do NOT attempt to poll, query, or wait for background tasks using AgentResult or any tool. Background results are delivered automatically as completion notifications. The AgentResult blocks appearing in conversation history are internal system-injected markers, NOT callable tools. Calling AgentResult will always fail. After spawning a background agent, continue your independent work or finish your turn immediately.`
2. **`Agent` 工具描述强化（`cc-middlewares/src/subagent/tool/define.rs`）**：
   在 `Usage` 中明确声明后台结果只由系统事件通知交付，禁止模型尝试使用任何工具轮询。
3. **优化报错返回引导（`cc-middlewares/src/subagent/agent_result.rs`）**：
   将 `AGENT_RESULT_UNSUPPORTED` 提示词改得更具终结性，明确指示模型“此工具非可调用工具，立即停止调用并结束本轮等待系统通知”，杜绝“稍后可能查到”的心理预期。
4. **消息格式架构优化（根本解）**：
   评估重构 `inject_bg_result_messages`，避免使用模型可见的 `ToolUse` 结构注入后台通知，改用非工具态通知格式，消除上下文中的模仿诱因。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-10-13 | — | Open | agent | 创建（来源：本地会话现场记录） |
| 2026-10-13 | Open | Open | agent | 论据复核：截断逻辑/字符边界/清单定位/注册前提/错误路径逐条对代码核对通过；原「误判为调用成功」降级为待观察推断 |
| 2026-10-14 | Open | Open | agent | 实测证实现象4：定位到 `inject_bg_result_messages` 注入伪 tool_use 导致上下文 Few-shot 模仿驱动；补齐提示词压制建议与涉及文件清单 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
