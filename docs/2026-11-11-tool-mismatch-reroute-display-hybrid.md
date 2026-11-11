# 工具错配自动改道后，历史回放把调用显示成「原声明名 + 改道后输出」的杂交体

- 发现日期：2026-11-11
- 状态：已修复（方向 2：`BaseMessage::Tool` 记录实际执行名，展示层优先取用）
- 现象来源：真实会话（`~/.cc-code/threads/threads.db`，thread `01a1c956`，内核 `dsh`）
- 严重程度：低（纯展示问题，不影响执行结果与模型上下文）
- 相关代码：`cc-agent/src/agent/executor/tool_dispatch.rs`、`cc-tui-core/src/message_view/mod.rs`、`cc-tui/src/app/message_pipeline/transform.rs`、`cc-tui-core/src/tool_display.rs`

## 一、现象

消息区出现这样一个工具块：

```
● Bash
  ⎿ Found 3 files
    .\spec\features\agent-command-center-prd.md
    .\cc-tui\src\app\agent_center\render.rs
    … +1 line (ctrl+o to expand)
```

头部写着 `Bash`，正文却是 `Grep`（`files_with_matches` 模式）的输出。两个可观察的破绽：

1. **头部没有参数摘要括号**。同一位置的正常工具块形如 `● Search(pattern: "…")`，这里只有 `● Bash`，后面空无一物。
2. **正文格式与工具名对不上**。`Found N files` + 文件路径列表是 Grep 的输出形态，Bash 不会产生这种正文。

## 二、事实基线（会话库原始记录）

同一条对话（thread `01a1c956`）里三层记录如下，可逐字复核：

```
AI 消息（模型声明）：
  tool_calls: name = "Bash"
              arguments = {"output_mode": "files_with_matches",
                           "pattern": "Task details|task_details|TaskDetails"}

工具结果消息：
  is_error = false
  content  = "Found 3 files
              .\spec\features\agent-command-center-prd.md
              .\cc-tui\src\app\agent_center\render.rs
              .\cc-tui\src\app\agent_center\center_test.rs"
```

关键点：模型声明的是 `Bash`，但参数里**没有 `command`**，取而是 `pattern` / `output_mode` —— 这是 Grep 的参数。带这种参数集的 `Bash` 调用只可能以「缺少必填参数 `command`」失败。

但结果 `is_error = false`，且正文是货真价实的 Grep 输出。说明**实际执行的确实是 Grep**，只是记录在 AI 消息里的名字仍是被改道前的 `Bash`。

## 三、机制

### 3.1 自动改道会替换运行时身份，但不动 AI 消息

`cc-agent/src/agent/executor/tool_dispatch.rs:327` 的 `rescue_mismatched_call`（在 `:961` 的「阶段零」被调用）捕获 schema 校验失败，并按「参数 100% 符合唯一白名单工具」的判定（白名单见 `:222` `AUTO_RESCUE_WHITELIST = ["Grep", "Glob", "Read"]`，另有受只读判定约束的 `Bash` 例外）把调用改道到 `Grep`。

该函数 `:323-326` 的注释明确写了它的作用范围：

> 替换 `name` 是有意为之：事件（ToolPending/ToolStart/ToolEnd）、HITL 审批、熔断归因都读 `ToolCall.name`，只有在此处一次改对，下游才会一致地把调用当作真正执行的工具看待。返回给 LLM 的历史不受影响——`dispatch_tools` 的 AI 消息由 `reasoning.tool_calls`（模型原始声明）构造，tool_result 仅按 `tool_call_id` 关联。

即：改道只改**运行时**那份 `ToolCall`，用来驱动事件与审批；写进历史的 AI 消息仍是模型原始声明（`:691` 构造 `ai_msg`，`:735` `state.add_message(ai_msg)`）。

对模型而言这是对的（历史里保持声明的名字，避免污染上下文），但展示层**也**从那份声明上取名字。

### 3.2 展示层从 AI 消息回查工具名，于是拿到改道前的名字

`BaseMessage::Tool` 本身不存工具名（`cc-agent/src/messages/message.rs:82-89`，只有 `tool_call_id` / `content` / `is_error`）。展示层必须回查前一条 AI 消息：

- `cc-tui/src/app/message_pipeline/transform.rs:142` → `prev_ai_tool_calls` 取自 AI 消息的 `tool_calls`
- `cc-tui-core/src/message_view/mod.rs:1078` → 按 `tool_call_id` 查出 `(tool_name, input)`，得到 `Bash` 与那份 Grep 参数

然后：

- `cc-tui-core/src/tool_display.rs:432` → `"Bash" => input["command"]`，而这次入参里没有 `command` → 返回 `None` → **头部不带参数括号**（现象 1 的直接来源）
- 正文照旧渲染工具结果 → Grep 的输出（现象 2 的直接来源）

### 3.3 为什么实时看着正常、过一会儿就变了

改道发生在「阶段零」，早于所有事件（`:947-949` 注释即为此），所以流式事件携带的是改道后的名字，当场渲染为 `● Search(pattern: "…")`。等 `StateSnapshot` 到达、reconcile 从历史重建视图（`cc-tui/src/app/message_pipeline/reconcile.rs:266-267` 用 `ct.name` 生成显示名，而 `ct.name` 来自事件）——两条路径的名字来源不同，于是同一个块在两个时刻显示成两个名字。

`CHANGELOG.md` 中该改道机制从「执行前阶段二」前移到「阶段零」的初衷，正是要消掉「原名 + 改道后参数」的杂交体（见 v0.6.265 条目）。它治好了事件与审批，**漏了历史回放这条展示路径**。

## 四、影响范围

扫全部会话库（927 个会话）中可关联到 AI 声明的工具结果：

| 结果形态 | 声明为 `Grep` | 声明为 `Bash` | 声明为 `Glob` | 声明为 `Read` |
|---|---|---|---|---|
| `Found N files`（Grep 独有） | 1182 | **2** | 0 | 0 |

即该杂交体共 **2 例**，均为「模型声明 `Bash` 但传 Grep 参数」这一种错配（截图中所示即其中一例，另一例在 thread `01a1c943`，参数为 `{"-i": true, "output_mode": "files_with_matches", "path": "…", "pattern": "kernel"}`）。

发生率低的原因是独特性判定很窄：`{pattern, output_mode}` 需**恰好**只满足 `Grep`（若同时满足 `Glob` 会按歧义放弃改道，见 `:294-301`）；而 `{pattern, path}` 这类常见组合恰好会被判为歧义、不改道。所以能触发的是「只带 `pattern`、不带 `path`」这类窄形态。

其余以 `Found` 开头的命中（如 `Found 8 rows:` / `Found in new-api: …`）经核对是 Bash 命令自身 stdout 恰好以 "Found" 起始，**不是**本缺陷，已剔除。

## 五、复现条件

- **复现频率**：偶发（取决于模型是否恰好产出「只带 `pattern` / `output_mode`」的错配调用）
- **触发步骤**：
  1. 任意内核下（本例为 `dsh`），让模型发起一次错配调用：声明 `Bash` 但入参为 `{"pattern": "...", "output_mode": "files_with_matches"}`（无 `command`）
  2. 观察该工具块：流式期间显示 `● Search(…)`，`StateSnapshot` 到达后变为 `● Bash` + Grep 正文、且无参数括号
- **环境**：peri 本仓库；Windows；`dsh` 内核会话（`~/.cc-code/settings.json` 的 `config.agent_kernel`）
- **验证依据**：`~/.cc-code/logs/agent-tui.log` 中有同机制的 WARN 先例可对照 —— `Schema 校验失败后自动改道执行：Glob -> Read tool.from=Glob tool.to=Read call_id=call_01_6ho1qow23znrylmhwuzf2737`（2026-11-11T06:43:38Z）

## 六、涉及文件

- `cc-agent/src/agent/executor/tool_dispatch.rs:327` —— `rescue_mismatched_call`，改道并替换运行时的 `ToolCall.name`/`input`（`:359-363`）
- `cc-agent/src/agent/executor/tool_dispatch.rs:961` —— 阶段零调用点（早于所有事件与审批）
- `cc-agent/src/agent/executor/tool_dispatch.rs:222` —— `AUTO_RESCUE_WHITELIST`
- `cc-agent/src/agent/executor/tool_dispatch.rs:691`、`:735` —— AI 消息由 `reasoning.tool_calls`（原始声明）构造并写入 state
- `cc-agent/src/messages/message.rs:82-89` —— `BaseMessage::Tool` 不存工具名，只存 `tool_call_id`
- `cc-tui/src/app/message_pipeline/transform.rs:142` —— `prev_ai_tool_calls` 从 AI 消息的 `tool_calls` 提取
- `cc-tui-core/src/message_view/mod.rs:1078` —— 按 `tool_call_id` 回查工具名与入参
- `cc-tui/src/app/message_pipeline/reconcile.rs:266-267` —— reconcile 路径用事件带来的 `ct.name` 生成显示名（与回放路径来源不同，即 3.3 的差异点）
- `cc-tui-core/src/tool_display.rs:432` —— `"Bash" => input["command"]`，`command` 缺失即无参数括号
- `cc-tui-core/src/tool_display.rs:530-540`（`search_args_summary`）—— Grep/Glob 的参数摘要在被改道路径下不会走到

## 七、修复方向（已实施：方向 2）

核心矛盾：**运行时的真实工具名（事件里是 `Grep`）没有落到任何持久化字段上**，展示层只能从 AI 消息的原始声明回查，于是回查结果与实际执行不一致。

可选方向：

1. **改道时同步改写 AI 消息的 `tool_calls`**。写点存在（`collect_tool_results` 跑在 `state.add_message` 之前），但需要先定「喂给 LLM 的历史该看到声明名还是执行名」——目前是有意保留声明名，改动会影响 LLM 上下文语义。
2. **另存「实际执行名」供展示用**。例如在 `BaseMessage::Tool` 或 ToolResult 上带一个 `resolved_tool_name`，展示层优先取它、缺失时回退到 AI 消息回查。改动面小且不动 LLM 历史语义，但需新增字段与序列化（含向后兼容）。
3. **仅在 `is_error == false` 且下游工具名与声明名不一致时**做展示层修正。需要执行名可从某处获得——回到方向 1/2 的前置条件。

建议优先评估方向 2：它把「实际执行了什么」显式记录下来，既修展示，也不改变现有的 LLM 历史口径。

## 八、验证边界

- 本文结论基于**会话库原始记录 + 源码静态分析**，未启动 TUI 实跑复现；3.3 中「先显示 `Search` 后变 `Bash`」由两条代码路径（事件携带改道名 / 回放回查声明名）推得，未逐帧验证。
- 影响范围的两例由会话库全量扫描得出（927 会话），扫描口径为「AI 声明名 ≠ `Grep` 且工具结果首行为 `Found N files`」，并以 `Found N rows:` 等 Bash 自身输出做了反向排除。
- 未检查该杂交体是否会同时影响状态栏工具计数（`✓ Name ×N`）——未做验证。
