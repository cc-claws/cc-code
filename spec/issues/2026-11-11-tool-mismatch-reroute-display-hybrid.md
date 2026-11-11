# 工具错配自动改道后历史回放显示「声明名 + 改道后输出」杂交体

**状态**：Open
**优先级**：低
**创建日期**：2026-11-11

## 问题描述

模型声明工具 A 但传了工具 B 的参数、经自动改道实际执行工具 B 后，消息区的工具块在历史回放（StateSnapshot 到达 / resume / 会话恢复）时显示成「工具 A 的名字 + 工具 B 的输出」的杂交体。流式期间显示正常，稍后翻转为杂交形态。纯展示问题，不影响执行结果与模型上下文。

## 症状详情

消息区出现这样一个工具块（真实会话实例）：

```
● Bash
  ⎿ Found 3 files
    .\spec\features\agent-command-center-prd.md
    .\cc-tui\src\app\agent_center\render.rs
    … +1 line (ctrl+o to expand)
```

头部写着 `Bash`，正文却是内容搜索（`files_with_matches` 模式）的输出。两个可观察的破绽：

1. **头部没有参数摘要括号**。同一位置的正常工具块形如 `● Search(pattern: "…")`，这里只有 `● Bash`，后面空无一物。
2. **正文格式与工具名对不上**。`Found N files` + 文件路径列表是内容搜索的输出形态，Bash 不会产生这种正文。

同一个块在两个时刻显示成两个名字：流式期间显示 `● Search(pattern: "…")`，StateSnapshot 到达后变为 `● Bash` + 搜索正文、且无参数括号。

## 复现条件

- **复现频率**：偶发（取决于模型是否恰好产出「只带 `pattern` / `output_mode`、不带 `command`」的错配调用）
- **触发步骤**：
  1. 让模型发起一次错配调用：声明 `Bash` 但入参为 `{"pattern": "...", "output_mode": "files_with_matches"}`（无 `command`）
  2. 观察该工具块：流式期间显示 `● Search(…)`，StateSnapshot 到达后变为 `● Bash` + 搜索正文、且无参数括号
- **环境**：Windows；会话库全量扫描（927 个会话）共发现 2 例，均为「声明 `Bash` 但传内容搜索参数」这一种错配

## 涉及文件

- `cc-agent/src/agent/executor/tool_dispatch.rs` —— 自动改道（schema 错配救援）替换运行时调用身份的位置
- `cc-tui-core/src/message_view/mod.rs` —— 历史回放时按 `tool_call_id` 回查工具名与入参的展示逻辑
- `docs/2026-11-11-tool-mismatch-reroute-display-hybrid.md` —— 完整机制与根因分析（补充材料）

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-11-11 | — | Open | agent | 创建 |

## 修复记录

### 修复 #1（2026-11-11）

- **操作人**：agent
- **用户原意**：历史回放时工具块显示的名字应与实际执行的工具一致，不再出现声明名与输出的杂交体
- **修复内容**：`BaseMessage::Tool` 新增 `tool_name` 字段记录实际执行名（构造结果消息时填入改道后的执行名），展示层优先取该字段、旧记录缺省回退声明名回查；AI 消息保留声明名，LLM 历史口径不变
- **涉及 commit**：2df212fe
- **验证状态**：待验证
