# cli_print：`-p --output-format stream-json` 的 tool_use 事件 input 恒为 null

**状态**：Fixed
**优先级**：中
**创建日期**：2026-10-14
**Issue**：[#496](https://github.com/cc-claws/cc-code/issues/496)
**PR**：[#497](https://github.com/cc-claws/cc-code/pull/497)

---

## 一、问题背景与现场

使用 `cc-code -p "<prompt>" --output-format stream-json` 时，每次工具调用输出的 `tool_use` 事件里 `input` 字段恒为 `null`，只带工具名 `name` 和调用 id `id`。消费方（脚本、自动化评测、CI 断言）拿不到任何工具参数，无法校验模型是否传了正确参数。

实测（`--bare --no-session-persistence --permission-mode bypass`）输出恒为：

```json
{"type":"tool_use","id":"call_00_EHKgkbEUEgdPtZnzsQbu9031","name":"Bash","input":null}
```

期望：`input` 携带真实参数对象，例如 `{"command":"pwd && ls -la","description":"show dir"}`。

## 二、根因

- `AgentEvent::ToolStart` 本身已携带完整 `input: serde_json::Value`（`cc-agent/src/agent/events.rs`，在 `tool_dispatch.rs` 的 before_tool 之后 emit）。
- 但 `cc-tui/src/cli_print.rs` 的 `PrintCollector::handle_event` 在 StreamJson 分支 match `E::ToolStart` 时用 `..` 丢弃了 `input`，序列化时硬编码 `null`。

## 三、涉及文件

- `cc-tui/src/cli_print.rs` —— `PrintCollector::handle_event` 的 StreamJson 分支（输出 tool_use 事件）
- `cc-agent/src/agent/events.rs` —— `AgentEvent::ToolStart` 定义（已含 `input`）

## 四、修复

- `cli_print.rs`：解构出 `input` 并原样序列化（`input: null` → `input: input`），其余字段与事件形态不变。
- `cli_print_test.rs`（新增）：3 项测试——工具参数透传、空对象参数保持 `{}`、`tool_result` 结构回归。

## 五、影响

- 仅影响 `-p --output-format stream-json` 的输出内容；`text` / `json` 与 TUI 路径不受影响。
- `tool_use` 的 `id` / `name` / `type` 字段形态不变；`input` 此前恒为 `null`，消费方本就无法依赖其值，属纯增强。

## 六、状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-10-14 | — | Open | agent | 实测发现，创建 Issue #496 |
| 2026-10-14 | Open | Fixed | agent | 提交 PR #497（cli_print.rs + 新增 cli_print_test.rs） |
